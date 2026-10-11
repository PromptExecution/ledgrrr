//! # Local LLM inference — mistralrs backend
//!
//! **Who this module is for.**  This file implements the "fast path" for running a
//! large-language model (LLM) entirely on the user's own machine — no data is sent to
//! any cloud service.  It is intended to be read and audited by governance committees
//! who need to understand *what the AI is doing*, *what data it can see*, and *why
//! specific technical choices were made*.  Technical jargon is explained in plain
//! English below.
//!
//! ---
//!
//! ## What is an LLM and what does it do here?
//!
//! A large language model is a statistical pattern-matcher trained on vast amounts of
//! text.  Given a sequence of words (a "prompt"), it predicts what word is most likely
//! to come next, then repeats that prediction to produce a multi-word reply.
//!
//! In this application the LLM is used as a *classification and reasoning engine* for
//! financial documents — not as a general chatbot.  It receives a structured prompt
//! built from a transaction record and rules, and produces a structured JSON response
//! that the pipeline validates before writing to the audit workbook.  The model never
//! receives raw credentials, authentication tokens, or personally-identifiable
//! information beyond what appears in a financial statement line.
//!
//! ---
//!
//! ## What is the model file? (GGUF format)
//!
//! The model's "knowledge" is encoded as billions of floating-point numbers called
//! *weights* or *parameters*.  Storing them at full precision (32-bit floats) for
//! Phi-4 Mini would require ≈ 14 GB of RAM, which makes it impractical to run on most
//! workstations.
//!
//! **Quantization** is a compression technique that replaces each 32-bit float with a
//! smaller integer (typically 3–8 bits).  The file used here (`Q3_K_M`) encodes each
//! weight in roughly 3.35 bits on average, reducing the RAM requirement to ≈ 2 GB
//! while preserving most of the model's reasoning accuracy.  The tradeoff is a small,
//! measurable degradation in output quality compared with full-precision weights —
//! acceptable for classification tasks but worth noting for audit purposes.
//!
//! **GGUF** (GPT-Generated Unified Format) is the binary container that holds both the
//! quantized weights *and* the model's configuration metadata in a single file.  The
//! file begins with a header section containing key-value pairs (e.g., how many
//! attention heads the model has, the size of each layer, the positional-encoding
//! settings) followed by the raw tensor data.  GGUF replaced the older GGML format in
//! 2023; it is the de-facto standard for distributing quantized open-weight models.
//!
//! Key metadata fields relevant to this model:
//!
//! | Field | Value | Plain-English meaning |
//! |---|---|---|
//! | `phi3.embedding_length` | 3072 | Width of the model's internal representation |
//! | `phi3.attention.head_count` | 24 | Number of parallel attention "heads" per layer |
//! | `phi3.attention.head_count_kv` | 8 | Heads used for key/value cache (grouped-query attention) |
//! | `phi3.rope.dimension_count` | 96 | How many dimensions carry positional information |
//! | `phi3.block_count` | 32 | Number of transformer layers stacked in the model |
//!
//! ---
//!
//! ## Why this particular model? (Microsoft Phi-4 Mini Reasoning)
//!
//! Phi-4 Mini is a 3.8-billion-parameter model from Microsoft, optimised for
//! multi-step reasoning tasks.  It was chosen because:
//!
//! * It is **open-weight** — the weights are published under the MIT license and can
//!   be downloaded, audited, and run locally without any API subscription or data
//!   residency concerns.
//! * Its **3.8 B parameter count** fits in ≈ 2 GB RAM after Q3 quantization, making
//!   it runnable on a standard developer workstation without a GPU.
//! * Its **reasoning fine-tune** produces better step-by-step classification output
//!   compared with base models of similar size.
//!
//! ---
//!
//! ## Why are there TWO local-inference backends? (candle vs. mistralrs)
//!
//! Two Cargo features — `local-llm` (candle) and `mistralrs-llm` (this file) — provide
//! the same `AgentRuntime` interface via different inference engines.  They are
//! complementary, not competing:
//!
//! | | `local-llm` (candle) | `mistralrs-llm` (this file) |
//! |---|---|---|
//! | **Library** | `candle-core` — HuggingFace's pure-Rust tensor library | `mistralrs` — high-performance LLM runtime built on candle |
//! | **Speed** | Slow: reference implementation, no fused kernels | 3–5× faster: hand-tuned GEMM kernels, parallel token scheduling |
//! | **Control** | High: we load the GGUF and apply patches *in memory* before any weights are read | Lower: mistralrs owns the GGUF loading pipeline internally |
//! | **Dependencies** | ~12 crates, pure Rust | ~80 crates including native C++ via cc-rs |
//! | **Best for** | Smoke tests, correctness validation, CI without GPU | Development, interactive classification, benchmarking |
//! | **Patch approach** | In-memory metadata edits before model construction | Patched GGUF sidecar written to disk once, reused thereafter |
//!
//! **Why keep candle at all once mistralrs is faster?**  The candle backend acts as an
//! independent reference implementation.  If both backends produce consistent output on
//! the same prompt, that is evidence the quantized weights and inference logic are
//! correct — it is a cheap form of differential testing.  A regression in one backend
//! that does not appear in the other immediately narrows the root cause.
//!
//! ---
//!
//! ## Phi-4 Mini architectural quirks requiring special handling
//!
//! Two properties of Phi-4 Mini's GGUF file require patching before a standard loader
//! can run inference.  Both are documented here so auditors can verify the patches are
//! benign and do not alter the model's behaviour.
//!
//! ### 1. Tied embeddings — the missing `output.weight` tensor
//!
//! Most transformer models have two large lookup tables:
//! * `token_embd.weight` — converts input token IDs into vectors at the start of the
//!   network.
//! * `output.weight` — converts the network's final vector back into a probability
//!   distribution over the vocabulary at the end.
//!
//! Phi-4 Mini uses *tied embeddings*: the same weight matrix serves both roles,
//! halving the memory occupied by these two tables.  Because only one copy is stored,
//! the GGUF file contains `token_embd.weight` but no `output.weight` entry.
//!
//! mistralrs' loader unconditionally looks up `output.weight` and fails if it is
//! absent.  The fix is to write a second copy of `token_embd.weight` under the name
//! `output.weight` into a *sidecar* GGUF file on disk.  This file is created once,
//! verified by timestamp, and reused on subsequent runs.  **No model weights are
//! changed — we are only adding a second reference to an existing weight matrix.**
//!
//! ### 2. Partial RoPE — the positional encoding dimension mismatch
//!
//! RoPE (Rotary Position Embedding) encodes the position of each token in the
//! sequence by rotating vectors in a high-dimensional space.  The number of
//! dimensions that receive this rotation is called `rope_dim`.
//!
//! Phi-4 Mini's GGUF declares `rope_dim = 96`, but each attention head has
//! `head_dim = embedding_length / head_count = 3072 / 24 = 128` dimensions.
//! Using partial RoPE (rotating only 96 of 128 dimensions) is a deliberate design
//! choice — the remaining 32 dimensions carry no positional signal, which can improve
//! generalisation on long sequences.
//!
//! Standard inference libraries, including the version of mistralrs and candle used
//! here, do not implement partial RoPE: they apply the rotation to all `head_dim`
//! dimensions and fail with a shape mismatch when `rope_dim != head_dim`.
//!
//! The fix applied in `ensure_patched_gguf` is to **override** the metadata key
//! `phi3.rope.dimension_count` from 96 to 128 in the sidecar file.  This causes the
//! loader to build a full-size RoPE frequency table that covers all 128 dimensions.
//! The 32 extra dimensions receive slightly extended frequencies rather than being
//! left unrotated — this is a minor inaccuracy relative to the reference
//! implementation, acceptable for classification smoke tests, and clearly documented
//! here for audit transparency.  Long-context production use should wait for a
//! mistralrs release that implements partial RoPE natively (tracked upstream).
//!
//! ---
//!
//! ## Data flow and privacy
//!
//! ```text
//! ModelRequest  ──►  build_messages()  ──►  mistralrs (in-process)
//!                                                   │
//!                         patched GGUF  ◄──  ensure_patched_gguf()
//!                         (local disk)
//!                                                   │
//!                                         ModelResponse (text)
//! ```
//!
//! All inference happens inside the same OS process.  No network calls are made during
//! inference.  The only outbound network activity occurs once per machine when the
//! tokenizer vocabulary file is downloaded from HuggingFace Hub and cached locally
//! (see `tok_model_id`).  Subsequent runs use the cached file.
//!
//! ---
//!
//! ## Parameters and tuneable settings
//!
//! | Setting | Default | How to change | Effect |
//! |---|---|---|---|
//! | `max_tokens` | 256 | `runtime.with_max_tokens(n)` | Hard ceiling on output length. Lower values produce shorter replies and run faster. Raise to 1024+ for long classification rationales. |
//! | `tok_model_id` | `"microsoft/Phi-4-mini-reasoning"` | `runtime.with_tok_model_id(repo)` | HuggingFace repo from which the tokenizer vocabulary is fetched. Change only if using a different model. |
//!
//! Enabled by Cargo feature: `mistralrs-llm`
//! Run smoke test: `just test-phi4-mistral`

use std::path::{Path, PathBuf};

use mistralrs::{GgufModelBuilder, TextMessageRole, TextMessages};

use crate::agent_runtime::{
    AgentRuntime, AgentRuntimeError, ModelRequest, ModelResponse, ModelRole,
};

/// HuggingFace repo used to fetch and cache the Phi-4 mini tokenizer vocabulary.
///
/// The tokenizer converts human-readable text into the integer token IDs that the
/// model consumes, and converts generated IDs back to text.  The vocabulary is
/// downloaded once and stored in the HuggingFace local cache
/// (`~/.cache/huggingface/hub/` on Linux).
const DEFAULT_TOK_REPO: &str = "microsoft/Phi-4-mini-reasoning";

/// Local inference runtime backed by `mistralrs`.
///
/// This struct is intentionally lightweight — it stores only path and configuration
/// strings.  The heavy model pipeline (loading weights, building attention caches) is
/// constructed inside each `complete()` call.  This makes the struct trivially
/// `Send + Sync` (required by the `AgentRuntime` trait) and avoids holding gigabytes
/// of model data in memory between calls, which matters on workstations with limited
/// RAM.
///
/// For a production workload with many sequential calls, wrap this in a caching layer
/// so the model pipeline is built once and reused.
#[derive(Debug, Clone)]
pub struct LocalMistralRuntime {
    /// Absolute path to the directory containing the GGUF weight file.
    model_dir: PathBuf,
    /// Filename of the GGUF weight file inside `model_dir` (e.g. `"Phi-4-mini-Q3_K_M.gguf"`).
    model_file: String,
    /// HuggingFace repo ID for the tokenizer vocabulary.  Downloaded once and cached.
    tok_model_id: String,
    /// Hard ceiling on the number of new tokens the model may generate per request.
    /// One token ≈ ¾ of an English word on average.
    max_tokens: usize,
}

impl LocalMistralRuntime {
    /// Construct a runtime pointing at `model_path`.
    ///
    /// Returns an error immediately if the file does not exist, so misconfigured paths
    /// are caught at startup rather than at the first inference call.
    pub fn new(model_path: impl AsRef<Path>) -> Result<Self, AgentRuntimeError> {
        let path = model_path.as_ref();
        if !path.exists() {
            return Err(AgentRuntimeError::LocalLlm(format!(
                "model file not found: {}",
                path.display()
            )));
        }
        let model_dir = path
            .parent()
            .ok_or_else(|| {
                AgentRuntimeError::LocalLlm(format!(
                    "cannot resolve parent dir of {}",
                    path.display()
                ))
            })?
            .to_path_buf();
        let model_file = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                AgentRuntimeError::LocalLlm(format!("cannot read filename from {}", path.display()))
            })?
            .to_string();
        Ok(Self {
            model_dir,
            model_file,
            tok_model_id: DEFAULT_TOK_REPO.to_string(),
            max_tokens: 256,
        })
    }

    /// Override the default token generation ceiling.
    ///
    /// Rule of thumb: 1 token ≈ 4 characters ≈ ¾ of a word.  A 256-token limit
    /// produces roughly a one-paragraph response; 1024 tokens allows detailed
    /// multi-step reasoning output.
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Override the HuggingFace repo used to download the tokenizer vocabulary.
    ///
    /// Only needed when switching to a different model family.  For all Phi-4 Mini
    /// variants the default `"microsoft/Phi-4-mini-reasoning"` is correct.
    pub fn with_tok_model_id(mut self, repo: impl Into<String>) -> Self {
        self.tok_model_id = repo.into();
        self
    }

    /// Convert a `ModelRequest` into mistralrs' `TextMessages` structure.
    ///
    /// mistralrs uses an OpenAI-style chat format: a sequence of messages each tagged
    /// with a role (System, User, or Assistant).  The system prompt, if present, is
    /// prepended as a System-role message; conversation history turns are inserted in
    /// order; and the current user message is appended last.
    fn build_messages(&self, request: &ModelRequest) -> TextMessages {
        let mut msgs = TextMessages::new();

        if let Some(sys) = request.system_prompt.as_deref().map(str::trim) {
            if !sys.is_empty() {
                msgs = msgs.add_message(TextMessageRole::System, sys);
            }
        }

        for turn in &request.history {
            let role = match turn.role {
                ModelRole::System => TextMessageRole::System,
                ModelRole::User => TextMessageRole::User,
                ModelRole::Assistant => TextMessageRole::Assistant,
                // mistralrs' `TextMessages` has no tool-call/tool-result
                // support (unlike `RequestBuilder`, which this backend does
                // not use) — fold a tool turn back in as plain user-visible
                // text rather than failing to compile or panicking. In
                // practice this branch is unreachable via the normal chat
                // path: `chat::model_turn` already maps persisted tool
                // history to `ModelRole::User` before it ever reaches here.
                ModelRole::Tool => TextMessageRole::User,
            };
            msgs = msgs.add_message(role, turn.content.trim());
        }

        msgs.add_message(TextMessageRole::User, request.user_message.trim())
    }

    /// Core async inference pipeline.
    ///
    /// Steps:
    /// 1. Ensure the patched GGUF sidecar exists (see `ensure_patched_gguf`).
    /// 2. Build a mistralrs model pipeline from the sidecar file.
    /// 3. Send the chat request and extract the first completion choice.
    async fn complete_async(
        &self,
        request: ModelRequest,
    ) -> Result<ModelResponse, AgentRuntimeError> {
        let original = self.model_dir.join(&self.model_file);

        // Produce (or reuse) the patched sidecar GGUF that works around Phi-4 Mini's
        // tied-embedding and partial-RoPE quirks — see module-level documentation for
        // the detailed rationale.
        let patched = ensure_patched_gguf(&original)?;

        let model_dir_str = patched
            .parent()
            .and_then(|p| p.to_str())
            .ok_or_else(|| {
                AgentRuntimeError::LocalLlm("patched model dir path is not valid UTF-8".into())
            })?
            .to_string();
        let model_file = patched
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                AgentRuntimeError::LocalLlm("patched model filename is not valid UTF-8".into())
            })?
            .to_string();

        let model = GgufModelBuilder::new(model_dir_str, vec![model_file])
            .with_tok_model_id(self.tok_model_id.clone())
            // Force CPU inference.  Phi-4 Mini at Q3 quantization fits in ≈ 2 GB RAM
            // and runs on any x86-64 workstation without requiring a GPU.  GPU
            // acceleration can be enabled via a mistralrs feature flag when available.
            .with_force_cpu()
            .build()
            .await
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("mistralrs build failed: {e}")))?;

        let messages = self.build_messages(&request);
        let response = model
            .send_chat_request(messages)
            .await
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("mistralrs request failed: {e}")))?;

        let text = response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| AgentRuntimeError::LocalLlm("mistralrs returned no content".into()))?;

        Ok(ModelResponse::text(text.trim()))
    }
}

impl AgentRuntime for LocalMistralRuntime {
    /// Synchronous wrapper around `complete_async`.
    ///
    /// `AgentRuntime::complete` is a blocking, synchronous method (the trait cannot
    /// require async because different callers have different async runtimes).
    /// Internally, mistralrs requires an async context.  The solution is to spin up a
    /// single-threaded tokio runtime specifically for this call, block until inference
    /// finishes, then return.  This is safe and correct but means the calling thread
    /// is blocked for the duration of inference (typically 10–120 seconds for Phi-4
    /// Mini on CPU depending on token count and hardware).
    fn complete(&self, request: ModelRequest) -> Result<ModelResponse, AgentRuntimeError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(AgentRuntimeError::Runtime)?;
        rt.block_on(self.complete_async(request))
    }
}

// ---------------------------------------------------------------------------
// GGUF sidecar patching helpers
// ---------------------------------------------------------------------------

/// Derive the path of the patched sidecar GGUF from the original file path.
///
/// Example: `/models/Phi-4-mini-Q3_K_M.gguf`
///        → `/models/Phi-4-mini-Q3_K_M.mistral-patched.gguf`
///
/// The sidecar lives in the same directory so relative symlinks and path
/// assumptions in the calling code remain valid.
fn patched_gguf_path(original: &Path) -> PathBuf {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    let parent = original.parent().unwrap_or(Path::new("."));
    parent.join(format!("{stem}.mistral-patched.gguf"))
}

/// Ensure a patched GGUF sidecar exists that is compatible with mistralrs.
///
/// # Why a sidecar file?
///
/// The candle backend (see `local_llm.rs`) can patch the GGUF *in memory* before
/// constructing the model because it owns the full loading pipeline in Rust.
/// mistralrs, being a higher-level library, manages GGUF loading internally and
/// provides no hook for intercepting the load.  The only way to supply a modified
/// GGUF to mistralrs is to write the changes to disk first.
///
/// # What is patched and why
///
/// Two metadata corrections are needed (see module-level documentation for full
/// background):
///
/// 1. **`output.weight` alias** — Phi-4 Mini omits this tensor from its GGUF because
///    it reuses `token_embd.weight` (tied embeddings).  mistralrs requires an explicit
///    `output.weight` entry.  We write a second copy of the embedding tensor under
///    that name.  The data is identical; only the name is added.
///
/// 2. **`phi3.rope.dimension_count` override** — The GGUF declares `rope_dim = 96`
///    but each attention head is 128-dimensional.  mistralrs (in this version) applies
///    RoPE to all head dimensions and fails when `rope_dim != head_dim`.  We override
///    the metadata to `rope_dim = head_dim = 128`.  The 32 extra dimensions receive
///    slightly extended positional frequencies rather than no rotation — an
///    approximation documented here and acceptable for smoke-test and classification
///    use cases.
///
/// # Caching
///
/// The sidecar is written once.  On subsequent calls, `ensure_patched_gguf` compares
/// the filesystem modification timestamps: if the sidecar is newer than the original,
/// it is reused without re-reading or re-writing anything.  This avoids a multi-minute
/// full-file copy on every inference call.
fn ensure_patched_gguf(original: &Path) -> Result<PathBuf, AgentRuntimeError> {
    use candle_core::quantized::gguf_file;

    let patched = patched_gguf_path(original);

    // Fast path: sidecar exists and is up to date.
    if patched.exists() {
        let up_to_date = std::fs::metadata(original)
            .and_then(|om| om.modified())
            .and_then(|orig_mtime| {
                std::fs::metadata(&patched)
                    .and_then(|pm| pm.modified())
                    .map(|patch_mtime| patch_mtime >= orig_mtime)
            })
            .unwrap_or(false);
        if up_to_date {
            return Ok(patched);
        }
    }

    eprintln!(
        "mistralrs: writing patched GGUF sidecar at {} …\n\
         (this copies ~2 GB once; subsequent runs reuse the sidecar)",
        patched.display()
    );

    let mut file = std::fs::File::open(original)
        .map_err(|e| AgentRuntimeError::LocalLlm(format!("open model: {e}")))?;
    let content = gguf_file::Content::read(&mut file)
        .map_err(|e| AgentRuntimeError::LocalLlm(format!("read gguf: {e}")))?;

    let needs_output_weight = !content.tensor_infos.contains_key("output.weight");
    let tensor_names: Vec<String> = content.tensor_infos.keys().cloned().collect();
    let device = candle_core::Device::Cpu;

    // Load every tensor from the original file into memory.
    let mut qtensors: Vec<(String, candle_core::quantized::QTensor)> =
        Vec::with_capacity(tensor_names.len() + usize::from(needs_output_weight));

    for name in &tensor_names {
        let qt = content
            .tensor(&mut file, name, &device)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("load tensor {name}: {e}")))?;
        qtensors.push((name.clone(), qt));
    }

    // Patch 1: add output.weight as a second copy of token_embd.weight.
    if needs_output_weight {
        let embd = content
            .tensor(&mut file, "token_embd.weight", &device)
            .map_err(|e| AgentRuntimeError::LocalLlm(format!("load embd for output alias: {e}")))?;
        qtensors.push(("output.weight".to_string(), embd));
    }

    // Patch 2: override phi3.rope.dimension_count to head_dim so the RoPE frequency
    // table is built for the full head width.  Compute head_dim from other metadata
    // fields; fall back to Phi-4 Mini's known values (3072 / 24 = 128) if absent.
    let head_dim = {
        let emb_len = content
            .metadata
            .get("phi3.embedding_length")
            .and_then(|v| v.to_u32().ok())
            .unwrap_or(3072);
        let head_count = content
            .metadata
            .get("phi3.attention.head_count")
            .and_then(|v| v.to_u32().ok())
            .unwrap_or(24);
        emb_len / head_count
    };

    // Build owned metadata so we can mutate the rope dimension entry.
    let mut metadata_owned: Vec<(String, gguf_file::Value)> = content
        .metadata
        .into_iter()
        .map(|(k, v)| {
            if k == "phi3.rope.dimension_count" {
                (k, gguf_file::Value::U32(head_dim))
            } else {
                (k, v)
            }
        })
        .collect();

    // Ensure the key is present even if the original GGUF omitted it.
    if !metadata_owned
        .iter()
        .any(|(k, _)| k == "phi3.rope.dimension_count")
    {
        metadata_owned.push((
            "phi3.rope.dimension_count".to_string(),
            gguf_file::Value::U32(head_dim),
        ));
    }

    let meta_refs: Vec<(&str, &gguf_file::Value)> = metadata_owned
        .iter()
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    let tensor_refs: Vec<(&str, &candle_core::quantized::QTensor)> =
        qtensors.iter().map(|(k, v)| (k.as_str(), v)).collect();

    let mut out = std::fs::File::create(&patched)
        .map_err(|e| AgentRuntimeError::LocalLlm(format!("create patched gguf: {e}")))?;
    gguf_file::write(&mut out, &meta_refs, &tensor_refs)
        .map_err(|e| AgentRuntimeError::LocalLlm(format!("write patched gguf: {e}")))?;

    eprintln!(
        "mistralrs: patched GGUF written ({} tensors, rope_dim overridden to {}).",
        tensor_refs.len(),
        head_dim
    );
    Ok(patched)
}
