//! b00t-server API key provisioning (l3dg3rr#212).
//!
//! `ledgerr-llm`'s default base URL now points at `_b00t_`'s local
//! model-serving proxy (b00t-server), not real OpenAI. Unlike OpenAI,
//! b00t-server genuinely validates the bearer token against
//! `~/.b00t/server-keys.json` — so `ledgerr-llm` needs a real, valid
//! b00t-server API key of its own, not just a URL change.
//!
//! [`LlmConfig::provision`] resolves one, in this order:
//! 1. `OPENAI_API_KEY` — existing override path, for callers who genuinely
//!    want to point back at real OpenAI (combined with `LEDGERR_LLM_BASE_URL`).
//!    Read by [`crate::LlmConfig::from_env`], not this module.
//! 2. [`B00T_SERVER_KEY_ENV`] — explicit escape hatch for headless/CI use,
//!    where minting-and-persisting to a settings file isn't appropriate.
//! 3. A key already persisted in the ledgrrr settings store
//!    (`AppSettings::b00t_server_api_key`).
//! 4. Lazily mint one via `b00t server key create --consumer ledgrrr
//!    --access ...` and persist it to the settings store for next time.
//!
//! Minting happens lazily, on first use, rather than as a separate
//! desktop-agent install step: `ledgerr-llm`'s only current caller
//! (`ledgerr-mcp`, behind its optional `llm` feature) is a headless MCP
//! server/CLI with no dependency on the desktop-agent install flow, so
//! provisioning has to work standalone. The settings store is still the
//! persistence target — desktop and headless callers end up sharing state.
//!
//! Never falls back to an empty api_key on failure: every error path here
//! returns [`LlmError::KeyProvisioning`] with a clear, actionable message.

use std::process::Command;

use ledgrrr_settings::SettingsStore;

use crate::error::{LlmError, LlmResult};
use crate::LlmConfig;

/// Env var escape hatch: an operator-provisioned b00t-server key, bypassing
/// the settings store and lazy minting entirely. Intended for headless/CI use.
pub const B00T_SERVER_KEY_ENV: &str = "LEDGERR_B00T_SERVER_KEY";

const CONSUMER: &str = "ledgrrr";

/// Ontology-class access `b00t server key create` grants the minted key.
/// Matches what `server_llm.rs`'s `check_access` calls require:
/// `b00t:ChatModel:execute` for `/v1/chat/completions`, `b00t:Model:read` +
/// `b00t:ChatModel:execute` for `/v1/models`, `b00t:EmbeddingModel:execute`
/// for `/v1/embeddings`.
const ACCESS_CLASSES: &[&str] = &[
    "b00t:ChatModel:execute",
    "b00t:Model:read",
    "b00t:EmbeddingModel:execute",
];

impl LlmConfig {
    /// Full key-resolution chain (see module docs). Prefer this over
    /// [`LlmConfig::from_env`] whenever the default b00t-server base URL is
    /// in play — `from_env` alone never mints or reads a stored key.
    pub fn provision() -> LlmResult<Self> {
        let store = SettingsStore::new(ledgrrr_settings::default_settings_path());
        Self::provision_with(&store, "b00t")
    }

    /// Test/DI seam for [`Self::provision`]: takes the settings store and the
    /// `b00t` CLI binary name/path explicitly so tests can supply a hermetic,
    /// path-isolated store ([`SettingsStore::new_json_file`] — NOT `new`,
    /// which on Windows ignores its path and always prefers the real
    /// registry) and exercise "b00t not on PATH" without mutating the
    /// process-wide `PATH` env var.
    pub(crate) fn provision_with(store: &SettingsStore, b00t_bin: &str) -> LlmResult<Self> {
        let mut config = Self::from_env();
        if !config.api_key.is_empty() {
            return Ok(config); // OPENAI_API_KEY set — real-OpenAI override path.
        }
        if let Ok(key) = std::env::var(B00T_SERVER_KEY_ENV) {
            if !key.is_empty() {
                config.api_key = key;
                return Ok(config);
            }
        }

        if let Some(key) = read_stored_key(store)? {
            config.api_key = key;
            return Ok(config);
        }

        config.api_key = mint_and_store_key(b00t_bin, store)?;
        Ok(config)
    }
}

fn read_stored_key(store: &SettingsStore) -> LlmResult<Option<String>> {
    let settings = store
        .load()
        .map_err(|e| LlmError::KeyProvisioning(format!("failed to read settings store: {e}")))?;
    Ok(settings.b00t_server_api_key.filter(|k| !k.is_empty()))
}

/// Shells out to `<b00t_bin> server key create`, validates the printed key,
/// and persists it into the settings store (load-modify-save, so other
/// settings fields are preserved — this must never reset the operator's
/// notification/tray preferences back to defaults).
fn mint_and_store_key(b00t_bin: &str, store: &SettingsStore) -> LlmResult<String> {
    let mut cmd = Command::new(b00t_bin);
    cmd.args(["server", "key", "create", "--consumer", CONSUMER]);
    for class in ACCESS_CLASSES {
        cmd.args(["--access", class]);
    }

    let output = cmd.output().map_err(|e| {
        LlmError::KeyProvisioning(format!(
            "could not run `{b00t_bin} server key create` (is the b00t CLI on PATH?): {e}. \
             Install b00t-cli, or set {B00T_SERVER_KEY_ENV} to an already-minted key."
        ))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(LlmError::KeyProvisioning(format!(
            "`{b00t_bin} server key create` exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let key = validate_minted_key(&output.stdout, b00t_bin)?;

    let mut settings = store
        .load()
        .map_err(|e| LlmError::KeyProvisioning(format!("failed to read settings store: {e}")))?;
    settings.b00t_server_api_key = Some(key.clone());
    store
        .save(&settings)
        .map_err(|e| LlmError::KeyProvisioning(format!("failed to persist minted key: {e}")))?;

    Ok(key)
}

/// `b00t server key create` prints ONLY the bare key value to stdout
/// (everything human-readable goes to stderr) — validate that contract holds
/// rather than trusting arbitrary stdout as a bearer token.
fn validate_minted_key(stdout: &[u8], b00t_bin: &str) -> LlmResult<String> {
    let key = String::from_utf8_lossy(stdout).trim().to_string();
    if key.is_empty() || !key.starts_with("b00t-sk-") {
        return Err(LlmError::KeyProvisioning(format!(
            "`{b00t_bin} server key create` did not print a valid b00t-sk- key on stdout (got: {key:?})"
        )));
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    // provision_with reads OPENAI_API_KEY / LEDGERR_B00T_SERVER_KEY from
    // process env — serialize these tests via the crate-level mutex, shared
    // with `crate::tests` (lib.rs), which mutates the same env vars. A mutex
    // local to just this module would NOT exclude those, since they run as
    // separate threads within the same test binary.
    use crate::ENV_TEST_MUTEX;

    fn clear_key_env() {
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var(B00T_SERVER_KEY_ENV);
    }

    #[test]
    fn openai_api_key_env_short_circuits_before_touching_settings_or_minting() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        std::env::set_var("OPENAI_API_KEY", "sk-real-openai-key");
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new_json_file(dir.path().join("settings.json"));

        // b00t_bin points at a binary that doesn't exist — if this path were
        // reached (it must not be), the test would fail with a
        // KeyProvisioning error instead of returning the OPENAI_API_KEY.
        let config =
            LlmConfig::provision_with(&store, "definitely-not-a-real-binary-xyz").unwrap();

        std::env::remove_var("OPENAI_API_KEY");
        assert_eq!(config.api_key, "sk-real-openai-key");
    }

    #[test]
    fn b00t_server_key_env_escape_hatch_short_circuits_before_minting() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        std::env::set_var(B00T_SERVER_KEY_ENV, "b00t-sk-headless-ci-key");
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new_json_file(dir.path().join("settings.json"));

        let config =
            LlmConfig::provision_with(&store, "definitely-not-a-real-binary-xyz").unwrap();

        std::env::remove_var(B00T_SERVER_KEY_ENV);
        assert_eq!(config.api_key, "b00t-sk-headless-ci-key");
    }

    #[test]
    fn reads_a_previously_stored_key_without_minting_a_new_one() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new_json_file(dir.path().join("settings.json"));
        let mut settings = store.load().unwrap();
        settings.b00t_server_api_key = Some("b00t-sk-already-stored".to_string());
        store.save(&settings).unwrap();

        // Binary that doesn't exist — proves the stored key was used instead
        // of attempting to mint a new one.
        let config =
            LlmConfig::provision_with(&store, "definitely-not-a-real-binary-xyz").unwrap();

        assert_eq!(config.api_key, "b00t-sk-already-stored");
    }

    #[test]
    fn missing_b00t_cli_fails_gracefully_with_a_clear_error_never_an_empty_key() {
        let _lock = ENV_TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        clear_key_env();
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new_json_file(dir.path().join("settings.json"));

        let result =
            LlmConfig::provision_with(&store, "definitely-not-a-real-binary-xyz-12345");

        let err = result.expect_err("must fail, never silently proceed with an empty key");
        let message = err.to_string();
        assert!(
            message.contains("definitely-not-a-real-binary-xyz-12345"),
            "error should name the binary it tried to run: {message}"
        );
        assert!(
            message.to_lowercase().contains("path")
                || message.to_lowercase().contains("not found")
                || message.to_lowercase().contains("no such file"),
            "error should explain the binary wasn't runnable: {message}"
        );
    }

    #[test]
    fn validate_minted_key_rejects_empty_stdout() {
        let result = validate_minted_key(b"", "b00t");
        assert!(result.is_err());
    }

    #[test]
    fn validate_minted_key_rejects_output_missing_the_expected_prefix() {
        let result = validate_minted_key(b"not-a-key\n", "b00t");
        assert!(result.is_err());
    }

    #[test]
    fn validate_minted_key_accepts_and_trims_a_well_formed_key() {
        let result = validate_minted_key(b"b00t-sk-abc123\n", "b00t").unwrap();
        assert_eq!(result, "b00t-sk-abc123");
    }

    #[test]
    fn mint_and_store_key_persists_without_clobbering_other_settings() {
        // Exercises the load-modify-save path directly (without a real
        // subprocess) by writing settings first, then simulating what
        // mint_and_store_key does after a successful mint: load, set the
        // key, save. Confirms other fields survive the round trip.
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let store = SettingsStore::new_json_file(settings_path);

        let mut settings = store.load().unwrap();
        settings.toast_enabled = false;
        store.save(&settings).unwrap();

        let mut reloaded = store.load().unwrap();
        reloaded.b00t_server_api_key = Some("b00t-sk-minted".to_string());
        store.save(&reloaded).unwrap();

        let final_settings = store.load().unwrap();
        assert_eq!(
            final_settings.b00t_server_api_key.as_deref(),
            Some("b00t-sk-minted")
        );
        assert!(
            !final_settings.toast_enabled,
            "minting a key must not reset unrelated settings"
        );
    }
}
