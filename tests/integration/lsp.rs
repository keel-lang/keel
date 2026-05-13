#[test]
fn lsp_hover_reports_let_binding_type() {
    use keel_lang::types::checker;
    let src = "agent A {\n    @on_start {\n        items = [1, 2, 3]\n    }\n}\n";
    // Cursor on `items` (line 2, column 8 → byte offset of `items` in source).
    let offset = src.find("items").unwrap() + 1;
    let label = checker::type_at(src, offset).expect("hover should resolve `items`");
    assert!(label.contains("list"), "expected list type, got: {label}");
    assert!(
        label.contains("int"),
        "expected int element type, got: {label}"
    );
}

#[test]
fn lsp_hover_reports_namespace() {
    use keel_lang::types::checker;
    let src = "agent A { @on_start { Io.show(\"x\") } }\n";
    let offset = src.find("Io").unwrap() + 1;
    let label = checker::type_at(src, offset).expect("hover on Io");
    assert!(
        label.contains("namespace"),
        "expected namespace label, got: {label}"
    );
}

#[test]
fn lsp_goto_definition_finds_task() {
    use keel_lang::types::checker;
    let src = "task greet() -> str {\n    \"hello\"\n}\nagent A {\n    @on_start {\n        r = greet()\n    }\n}\n";
    let offset = src.find("greet").unwrap() + 1;
    let span = checker::definition_of(src, offset);
    assert!(
        span.is_some(),
        "definition_of should find `task greet` declaration"
    );
    let s = span.unwrap();
    let name = &src[s.clone()];
    assert_eq!(
        name, "greet",
        "span should cover the identifier, got: {name:?}"
    );
}

#[test]
fn lsp_usages_of_finds_all_occurrences() {
    use keel_lang::types::checker;
    let src = "task foo() -> str { \"x\" }\nagent A { @on_start { r = foo() s = foo() } }\n";
    let spans = checker::usages_of(src, "foo");
    assert!(
        spans.len() >= 3,
        "expected at least 3 occurrences of `foo` (decl + 2 calls), got {}",
        spans.len()
    );
}
