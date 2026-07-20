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

/// M1-scalar-only fixtures (issue #136) — nested under `fixtures/` so
/// [`discover_corpus`]'s flat, non-recursive listing never picks them up:
/// they run under a separate comparison (interpreter vs. compiled), not the
/// interpreter-vs-interpreter determinism loop the rest of the corpus does.
/// Only used when the compiled engine actually exists.
#[cfg(feature = "build-backend")]
pub fn m1_scalar_fixtures_dir() -> PathBuf {
    fixtures_dir().join("m1_scalar")
}

/// Discovers the M1-scalar-only fixture set, sorted by stem.
#[cfg(feature = "build-backend")]
pub fn discover_m1_scalar_fixtures() -> Vec<CorpusEntry> {
    let mut entries = Vec::new();
    collect_flat_keel_files(&m1_scalar_fixtures_dir(), &mut entries);
    entries.sort_by(|a, b| a.stem.cmp(&b.stem));
    entries
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
