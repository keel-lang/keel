use keel_lang::lsp::analyze;
use tower_lsp::lsp_types::DiagnosticSeverity;

#[test]
fn clean_program_has_no_diagnostics() {
    let diags = analyze(r#"
agent Greeter {
  @role "hi"
}

run(Greeter)
"#);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
}

#[test]
fn parse_error_emits_diagnostic() {
    let diags = analyze("task t() {\n  x = \n}");
    assert!(!diags.is_empty());
    assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(diags[0].source.as_deref(), Some("keel"));
}

#[test]
fn type_error_emits_diagnostic() {
    let diags = analyze(r#"
task t() {
  x = undefined_name
}
"#);
    assert!(!diags.is_empty(), "expected a type diagnostic");
    assert!(diags.iter().any(|d| d.message.contains("undefined")));
}

#[test]
fn non_exhaustive_when_emits_diagnostic() {
    let diags = analyze(r#"
type U = low | medium | high

task t(u: U) {
  when u {
    low => { return }
  }
}
"#);
    assert!(diags.iter().any(|d| d.message.contains("non-exhaustive")));
}

#[test]
fn diagnostic_location_tracks_line() {
    // Undefined on line 3 (0-indexed: 2).
    let diags = analyze("\n\ntask t() { x = bogus }\n");
    let msg = diags
        .iter()
        .find(|d| d.message.contains("undefined"));
    assert!(msg.is_some(), "expected undefined diagnostic; got {diags:?}");
}
