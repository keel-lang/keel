//! Keel source analysis, LSP diagnostic conversion, and per-document semantic
//! index.
//!
//! [`analyze_document`] runs the full lex → parse → HIR → type-check pipeline,
//! produces LSP diagnostics, and builds a [`SemanticIndex`] that lets hover,
//! go-to-definition, and rename handlers answer queries without reparsing.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::hir::{self, Hir};
use crate::ide::hover::build_types;
use crate::lexer::{Span, Token};
use crate::session;
use crate::types::checker;
use logos::Logos;

use super::position::byte_range_to_lsp;

// ---------------------------------------------------------------------------
// Semantic index
// ---------------------------------------------------------------------------

/// Per-document semantic data extracted once at `did_open` / `did_change` so
/// hover, go-to-definition, and rename can answer queries without reparsing.
#[derive(Clone)]
pub(crate) struct SemanticIndex {
    /// Reference span → type-description string (scope-correct via HIR SymbolId).
    pub(crate) span_types: HashMap<Span, String>,
    /// Name-keyed fallback for binding declaration sites not tracked as
    /// references in the HIR (e.g. the LHS of a `let` statement).
    /// Prelude namespace and primitive type labels are *not* stored here;
    /// they are resolved at query time from process-wide static data.
    pub(crate) name_types: HashMap<String, String>,
    /// Reference span or declaration span → declaration span (go-to-definition).
    pub(crate) definitions: HashMap<Span, Span>,
    /// Spans of references and declarations that resolve to top-level symbols.
    /// Used as the rename-allowed gate (replaces the `is_top_level_symbol` reparse).
    pub(crate) top_level_refs: HashSet<Span>,
    /// Name → all usage spans (pre-built token scan for rename).
    pub(crate) usages: HashMap<String, Vec<Span>>,
}

// ---------------------------------------------------------------------------
// Document analysis
// ---------------------------------------------------------------------------

/// Run the full analysis pipeline for `text` and return LSP diagnostics plus,
/// on parse success, a populated [`SemanticIndex`].
///
/// On parse failure `semantic_index` is `None`; handlers fall back to the
/// existing reparse-on-demand helpers in that case.
pub(crate) fn analyze_document(text: &str) -> (Vec<Diagnostic>, Option<SemanticIndex>) {
    match session::parse_source(text, "file") {
        Err(report) => {
            let diags = spans_from_report(&report)
                .into_iter()
                .map(|(msg, span)| diag(text, span, msg, DiagnosticSeverity::ERROR))
                .collect();
            (diags, None)
        }
        Ok((program, _source)) => {
            // Lower once; reuse the HIR for both type-checking and the semantic index.
            let hir = hir::lower_ast(&program);
            let type_diags = checker::check(&hir);
            let lsp_diags = type_diags
                .iter()
                .map(|err| {
                    diag(
                        text,
                        err.span().clone(),
                        err.message(),
                        DiagnosticSeverity::ERROR,
                    )
                })
                .collect();
            let index = build_semantic_index(text, &program, &hir);
            (lsp_diags, Some(index))
        }
    }
}

fn build_semantic_index(text: &str, program: &crate::ast::Program, hir: &Hir<'_>) -> SemanticIndex {
    // ── 1. Single Checker pass for both SymbolId-keyed and name-keyed types ──
    let (symbol_types, name_types) = build_types(program, hir);

    // ── 2. Single pass over references → span_types, definitions, top_level_refs ──
    let mut span_types: HashMap<Span, String> = HashMap::new();
    let mut definitions: HashMap<Span, Span> = HashMap::new();
    let mut top_level_refs: HashSet<Span> = HashSet::new();
    for (ref_span, resolution) in hir.iter_references() {
        let Some(id) = resolution.symbol else {
            continue;
        };
        if let Some(ty_str) = symbol_types.get(&id) {
            span_types.insert(ref_span.clone(), ty_str.clone());
        }
        if let Some(symbol) = hir.symbol(id) {
            definitions.insert(ref_span.clone(), symbol.span.clone());
            if symbol.kind.is_top_level() {
                top_level_refs.insert(ref_span);
            }
        }
    }

    // ── 3. Declaration sites in definitions/top_level_refs + usages seed ───
    // Single pass: declaration sites, top-level refs, and usages map keys.
    let mut usages: HashMap<String, Vec<Span>> = HashMap::new();
    for symbol in hir.symbols() {
        if symbol.kind.is_definition() {
            definitions
                .entry(symbol.span.clone())
                .or_insert_with(|| symbol.span.clone());
        }
        if symbol.kind.is_top_level() {
            top_level_refs.insert(symbol.span.clone());
            usages.entry(symbol.name.clone()).or_default();
        }
    }

    // ── 4. Single-pass usages collection ─────────────────────────────────
    // One tokenizer pass over the file fills every pre-seeded entry,
    // replacing N independent `usages_of` calls (one per top-level symbol).
    if !usages.is_empty() {
        for (result, span) in Token::lexer(text).spanned() {
            if let Ok(Token::Ident(name)) = result
                && let Some(spans) = usages.get_mut(&name)
            {
                spans.push(span);
            }
        }
    }

    SemanticIndex {
        span_types,
        name_types,
        definitions,
        top_level_refs,
        usages,
    }
}

/// Run lex/parse/type-check and convert every failure into an LSP
/// diagnostic. Empty vec means a clean file.
///
/// Used by the unit tests in this module; production code uses
/// [`analyze_document`] which also builds the semantic index.
#[cfg(test)]
pub(crate) fn analyze(text: &str) -> Vec<Diagnostic> {
    match session::parse_source(text, "file") {
        Err(report) => spans_from_report(&report)
            .into_iter()
            .map(|(msg, span)| diag(text, span, msg, DiagnosticSeverity::ERROR))
            .collect(),
        Ok((program, source)) => {
            let checked = session::check_source(program, source);
            checked
                .diagnostics
                .iter()
                .map(|err| {
                    diag(
                        text,
                        err.span().clone(),
                        err.message(),
                        DiagnosticSeverity::ERROR,
                    )
                })
                .collect()
        }
    }
}

/// Extract `(label, span)` pairs from a miette::Report. Keel's lexer
/// and parser both emit LabeledSpans attached to their errors.
pub(crate) fn spans_from_report(report: &miette::Report) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    if let Some(labels) = report.labels() {
        for label in labels {
            let span = label.inner();
            let range = span.offset()..span.offset() + span.len();
            let msg = label
                .label()
                .map(str::to_string)
                .unwrap_or_else(|| report.to_string());
            out.push((msg, range));
        }
    }
    if out.is_empty() {
        out.push((report.to_string(), 0..0));
    }
    out
}

pub(crate) fn diag(
    text: &str,
    span: Span,
    message: String,
    severity: DiagnosticSeverity,
) -> Diagnostic {
    Diagnostic {
        range: byte_range_to_lsp(text, &span),
        severity: Some(severity),
        source: Some("keel".into()),
        message,
        ..Diagnostic::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── diag ────────────────────────────────────────────────────────

    #[test]
    fn diag_has_correct_severity_and_source() {
        use tower_lsp::lsp_types::Position;
        let d = diag(
            "test",
            0..4,
            "something wrong".into(),
            DiagnosticSeverity::ERROR,
        );
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.source.as_deref(), Some("keel"));
        assert_eq!(d.message, "something wrong");
        assert_eq!(
            d.range.start,
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            d.range.end,
            Position {
                line: 0,
                character: 4
            }
        );
    }

    #[test]
    fn diag_warning_severity() {
        let d = diag("test", 0..0, "hint".into(), DiagnosticSeverity::WARNING);
        assert_eq!(d.severity, Some(DiagnosticSeverity::WARNING));
    }

    // ── spans_from_report ──────────────────────────────────────────

    #[test]
    fn spans_from_report_parser_error() {
        use miette::NamedSource;
        let src = NamedSource::new("test.keel", "task t() {\n  x =\n}".to_string());
        let tokens = crate::lexer::lex("task t() {\n  x =\n}", &src).expect("lex should pass");
        let err = crate::parser::parse(tokens, "task t() {\n  x =\n}".len(), &src).unwrap_err();
        let spans = spans_from_report(&err);
        assert!(!spans.is_empty(), "expected at least one span");
    }

    #[test]
    fn spans_from_report_empty_fallback() {
        // Create a miette report without labels
        let report: miette::Report = miette::miette!("bare error without labels");
        let spans = spans_from_report(&report);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].0, "bare error without labels");
        assert_eq!(spans[0].1, 0..0);
    }

    // ── analyze ─────────────────────────────────────────────────────

    #[test]
    fn analyze_empty_string() {
        let diags = analyze("");
        assert!(
            diags.is_empty(),
            "empty string should produce no diagnostics, got: {diags:?}"
        );
    }

    #[test]
    fn analyze_blank_lines() {
        let diags = analyze("\n\n\n");
        assert!(
            diags.is_empty(),
            "blank lines should produce no diagnostics, got: {diags:?}"
        );
    }

    #[test]
    fn analyze_lexer_error() {
        let diags = analyze("@invalid");
        assert!(!diags.is_empty(), "expected lexer error diagnostic");
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn analyze_multiple_parse_errors() {
        let diags = analyze("task a() {\n  x =\n}\ntask b() {\n  y =\n}\n");
        assert!(
            diags.len() >= 2,
            "expected ≥2 parse diagnostics for two broken tasks, got {}: {diags:?}",
            diags.len()
        );
        assert!(
            diags
                .iter()
                .all(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        );
    }

    #[test]
    fn analyze_multiple_type_errors() {
        let diags = analyze(
            r#"
task t() {
  a = unknown1
  b = unknown2
}
"#,
        );
        let count = diags
            .iter()
            .filter(|d| d.message.contains("undefined"))
            .count();
        assert!(
            count >= 2,
            "expected at least 2 undefined errors, got {count}: {diags:?}"
        );
    }

    #[test]
    fn analyze_type_error_has_source() {
        let diags = analyze("task t() { x = bogus }");
        assert!(!diags.is_empty());
        for d in &diags {
            assert_eq!(d.source.as_deref(), Some("keel"));
        }
    }
}

#[cfg(test)]
mod analyze_tests {
    use super::{analyze, analyze_document};
    use tower_lsp::lsp_types::DiagnosticSeverity;

    #[test]
    fn clean_program_has_no_diagnostics() {
        let diags = analyze(
            r#"
agent Greeter {
  @role "hi"
}

run(Greeter)
"#,
        );
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
        let diags = analyze(
            r#"
task t() {
  x = undefined_name
}
"#,
        );
        assert!(!diags.is_empty(), "expected a type diagnostic");
        assert!(diags.iter().any(|d| d.message.contains("undefined")));
    }

    #[test]
    fn non_exhaustive_when_emits_diagnostic() {
        let diags = analyze(
            r#"
type U = low | medium | high

task t(u: U) {
  when u {
    low => { return }
  }
}
"#,
        );
        assert!(diags.iter().any(|d| d.message.contains("non-exhaustive")));
    }

    #[test]
    fn diagnostic_location_tracks_line() {
        let diags = analyze("\n\ntask t() { x = bogus }\n");
        let msg = diags.iter().find(|d| d.message.contains("undefined"));
        assert!(
            msg.is_some(),
            "expected undefined diagnostic; got {diags:?}"
        );
    }

    /// Structural invariant: prelude primitives and namespace names must NOT be
    /// stored in `name_types`. They are resolved at hover time from process-wide
    /// static data. If someone reintroduces the insertion loops in
    /// `build_semantic_index` this test will fail.
    #[test]
    fn semantic_index_name_types_excludes_prelude_labels() {
        let (_, index) = analyze_document("task t() { x = 1 }\n");
        let idx = index.expect("clean program should produce an index");
        assert!(
            idx.name_types.contains_key("x"),
            "name_types must contain user binding `x` (build_types must be populating it)"
        );
        for &(name, _) in crate::types::ty::PRIMITIVE_TYPE_LABELS {
            assert!(
                !idx.name_types.contains_key(name),
                "name_types must not store prelude primitive `{name}`"
            );
        }
        for ns in crate::types::prelude::namespace_names() {
            assert!(
                !idx.name_types.contains_key(ns.as_str()),
                "name_types must not store prelude namespace `{ns}`"
            );
        }
    }
}
