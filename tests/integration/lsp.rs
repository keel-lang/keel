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
fn lsp_goto_definition_finds_state_field_from_read_and_write_sites() {
    use keel_lang::types::checker;
    let src = "agent Counter {\n    state { count: int = 0 }\n    task tick() {\n        self.count = self.count + 1\n    }\n}\n";
    let declaration = src.find("count:").unwrap();
    let expected = declaration..declaration + "count".len();
    let write = src.find("self.count =").unwrap() + "self.".len() + 1;
    let read = src.rfind("self.count").unwrap() + "self.".len() + 1;

    assert_eq!(checker::definition_of(src, write), Some(expected.clone()));
    assert_eq!(checker::definition_of(src, read), Some(expected));
}

#[test]
fn lsp_goto_definition_uses_exact_method_declaration_span() {
    use keel_lang::types::checker;
    let src = "agent First {\n    task work() {}\n}\nagent Second {\n    task work() {}\n}\n";
    let declaration = src.rfind("work").unwrap();
    let expected = declaration..declaration + "work".len();

    assert_eq!(checker::definition_of(src, declaration + 1), Some(expected));
}

#[test]
fn lsp_rename_gate_allows_top_level_declaration_in_broken_file() {
    use keel_lang::types::checker;
    let src = "task stable() {}\ntask broken() {\n";
    let offset = src.find("stable").unwrap() + 1;

    assert!(checker::is_top_level_symbol(src, offset));
}

#[test]
fn lsp_rename_gate_rejects_agent_method_declaration_in_broken_file() {
    use keel_lang::types::checker;
    let src = "agent Bot {\n    task nested() {}\n";
    let offset = src.find("nested").unwrap() + 1;

    assert!(!checker::is_top_level_symbol(src, offset));
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
