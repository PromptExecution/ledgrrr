use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn unique_workbook_path(label: &str) -> PathBuf {
    let suffix = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "ledgerr-{label}-{}-{suffix}.xlsx",
        std::process::id()
    ))
}

pub fn manifest_for_workbook(workbook_path: &Path, active_year: i32) -> String {
    format!(
        "[session]\nworkbook_path=\"{}\"\nactive_year={active_year}\n",
        toml_escape_path(workbook_path)
    )
}

/// Escapes a path for embedding in a TOML basic string (`"..."`).
///
/// Windows temp paths are full of `\` separators; naively interpolating
/// `Path::display()` into a `"..."` TOML string works by luck on Linux/macOS
/// (no backslashes) but breaks on Windows any time a path segment happens to
/// contain a backslash followed by a character that isn't one of TOML's
/// recognized escapes (`\b \t \n \f \r \" \\ \uXXXX \UXXXXXXXX`) — which,
/// given how many segments a Windows temp path has, is the common case, not
/// the exception. Backslash- and quote-escaping here is what a real TOML
/// serializer would do for a basic string.
pub fn toml_escape_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[allow(dead_code)]
pub fn stdio_test_manifest(label: &str) -> String {
    format!(
        "{}\n[accounts]\nWF-BH-CHK = {{ institution = \"Wells Fargo\", type = \"checking\", currency = \"USD\" }}\n",
        manifest_for_workbook(&unique_workbook_path(label), 2023)
    )
}
