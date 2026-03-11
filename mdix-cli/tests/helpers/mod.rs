// mdix-cli/tests/helpers/mod.rs
//! Shared helpers for CLI integration tests.

use std::path::{Path, PathBuf};

/// Return the absolute path to `tests/fixtures/<name>`.
pub fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

/// Return (creating if necessary) a stable output directory under
/// `<workspace_root>/test_results/<category>/`.
///
/// Tests write their generated files here so results are inspectable
/// after a run rather than disappearing with a TempDir.
pub fn results_dir(category: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or(Path::new("."))
        .join("test_results")
        .join(category);
    std::fs::create_dir_all(&dir)
        .unwrap_or_else(|e| panic!("failed to create test_results/{}: {}", category, e));
    dir
}

/// Return a path inside the results directory for a specific test output file.
///
/// Example: `results_file("convert", "basic_to_json.json")`
pub fn results_file(category: &str, filename: &str) -> String {
    results_dir(category)
        .join(filename)
        .to_string_lossy()
        .to_string()
}
