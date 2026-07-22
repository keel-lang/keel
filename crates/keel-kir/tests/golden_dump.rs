//! Golden tests: `.keel` snippet in `tests/fixtures/*.keel` -> lowered KIR
//! textual dump, compared against `tests/fixtures/*.kir` (`designs/llvm-
//! compilation.md` §4 M0 exit criterion: "scalar-subset KIR golden-dump
//! tests pass").
//!
//! To regenerate a golden after an intentional dump-format change, set
//! `KEEL_KIR_BLESS=1` and run `cargo test -p keel-kir`; it overwrites the
//! `.kir` file with the freshly rendered dump instead of asserting.

use std::fs;
use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn dump_for(keel_source: &str, file_name: &str) -> String {
    let (program, _named) =
        keel_syntax::parse_source(keel_source, file_name).expect("fixture must parse");
    let (diagnostics, artifacts) =
        keel_compiler::types::checker::check_program_with_artifacts(&program, false);
    assert!(
        diagnostics.is_empty(),
        "golden fixture must type-check cleanly: {diagnostics:?}"
    );
    let kir = keel_kir::lower(&program, file_name, &artifacts).expect("fixture must lower to KIR");
    keel_kir::dump::dump(&kir)
}

#[test]
fn golden_dumps_match() {
    let dir = fixtures_dir();
    let mut stems: Vec<String> = fs::read_dir(&dir)
        .expect("read fixtures dir")
        .filter_map(|entry| {
            let path = entry.expect("read fixture entry").path();
            (path.extension().and_then(|e| e.to_str()) == Some("keel"))
                .then(|| path.file_stem().unwrap().to_string_lossy().into_owned())
        })
        .collect();
    stems.sort();
    assert!(!stems.is_empty(), "expected at least one fixture");

    let bless = std::env::var("KEEL_KIR_BLESS").as_deref() == Ok("1");

    for stem in stems {
        let keel_path = dir.join(format!("{stem}.keel"));
        let kir_path = dir.join(format!("{stem}.kir"));
        let source = fs::read_to_string(&keel_path).expect("read fixture .keel");
        let actual = dump_for(&source, &format!("{stem}.keel"));

        if bless {
            fs::write(&kir_path, &actual).expect("write golden .kir");
            continue;
        }

        let expected = fs::read_to_string(&kir_path).unwrap_or_else(|_| {
            panic!(
                "missing golden file {} — run with KEEL_KIR_BLESS=1 to generate it",
                kir_path.display()
            )
        });
        assert_eq!(
            actual, expected,
            "KIR dump mismatch for {stem}.keel (rerun with KEEL_KIR_BLESS=1 to update if intentional)"
        );
    }
}
