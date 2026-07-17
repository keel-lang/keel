//! Corpus discovery: `examples/*.keel` + `tests/conformance/fixtures/*.keel`.
//!
//! Flat (non-recursive) directory listing only, matching the existing
//! precedent for treating `examples/` as a corpus
//! (`pipeline::tests::examples_all_parse` in the root crate, and
//! `tests/common::run_example_subcommand`). `examples/inbox_assistant/` and
//! `examples/inbox_modules/` are multi-file directories, not standalone
//! `.keel` entry points, and are out of scope for the same reason: this
//! harness runs one `.keel` file as an entry module per corpus entry.

use std::path::{Path, PathBuf};

pub struct CorpusEntry {
    /// File stem — the key `main.rs`'s skip list matches against.
    pub stem: String,
    pub path: PathBuf,
}

pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/fixtures")
}

/// Discovers the corpus, sorted by stem for stable, reproducible test
/// output.
pub fn discover_corpus() -> Vec<CorpusEntry> {
    let mut entries = Vec::new();
    collect_flat_keel_files(&project_root().join("examples"), &mut entries);
    collect_flat_keel_files(&fixtures_dir(), &mut entries);
    entries.sort_by(|a, b| a.stem.cmp(&b.stem));
    entries
}

fn collect_flat_keel_files(dir: &Path, out: &mut Vec<CorpusEntry>) {
    let read_dir =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in read_dir {
        let path = entry.expect("read corpus dir entry").path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("keel") {
            let stem = path
                .file_stem()
                .expect("`.keel` file has a stem")
                .to_string_lossy()
                .into_owned();
            out.push(CorpusEntry { stem, path });
        }
    }
}
