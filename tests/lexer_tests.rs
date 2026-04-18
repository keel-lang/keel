use keel_lang::lexer::{lex, Token};
use miette::NamedSource;

fn tokens(source: &str) -> Vec<Token> {
    let named = NamedSource::new("test.keel", source.to_string());
    lex(source, &named)
        .unwrap()
        .into_iter()
        .map(|(tok, _span)| tok)
        .collect()
}

fn tokens_with_newlines(source: &str) -> Vec<Token> {
    tokens(source)
}

// ─── Keywords ────────────────────────────────────────────────────────────────

#[test]
fn lex_declaration_keywords() {
    let toks = tokens("agent task interface type extern");
    assert_eq!(
        toks,
        vec![
            Token::Agent,
            Token::Task,
            Token::Interface,
            Token::Type,
            Token::Extern,
        ]
    );
}

#[test]
fn lex_module_keywords() {
    let toks = tokens("use from");
    assert_eq!(toks, vec![Token::Use, Token::From]);
}

#[test]
fn lex_agent_body_keywords() {
    let toks = tokens("state on self");
    assert_eq!(toks, vec![Token::State, Token::On, Token::SelfKw]);
}

#[test]
fn lex_control_flow_keywords() {
    let toks = tokens("if else when where for in try catch return");
    assert_eq!(
        toks,
        vec![
            Token::If,
            Token::Else,
            Token::When,
            Token::Where,
            Token::For,
            Token::In,
            Token::Try,
            Token::Catch,
            Token::Return,
        ]
    );
}

#[test]
fn lex_cast_and_logic_keywords() {
    let toks = tokens("as and or not");
    assert_eq!(
        toks,
        vec![Token::As, Token::And, Token::Or, Token::Not]
    );
}

#[test]
fn lex_value_literals() {
    let toks = tokens("true false none now");
    assert_eq!(
        toks,
        vec![Token::True, Token::False, Token::None_, Token::Now]
    );
}

#[test]
fn lex_set_literal_keyword() {
    let toks = tokens("set [1, 2]");
    assert_eq!(
        toks,
        vec![
            Token::Set,
            Token::LBracket,
            Token::Integer("1".into()),
            Token::Comma,
            Token::Integer("2".into()),
            Token::RBracket,
        ]
    );
}

// ─── Prelude names are identifiers ───────────────────────────────────────────
//
// These look keyword-like but are prelude function names, attribute names,
// or stdlib namespaces — all ordinary identifiers.

#[test]
fn removed_keywords_are_identifiers() {
    let src = "classify draft every fetch send ask confirm notify role model tools";
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Ident("classify".into()),
            Token::Ident("draft".into()),
            Token::Ident("every".into()),
            Token::Ident("fetch".into()),
            Token::Ident("send".into()),
            Token::Ident("ask".into()),
            Token::Ident("confirm".into()),
            Token::Ident("notify".into()),
            Token::Ident("role".into()),
            Token::Ident("model".into()),
            Token::Ident("tools".into()),
        ]
    );
}

// ─── Identifiers ─────────────────────────────────────────────────────────────

#[test]
fn lex_identifiers() {
    let toks = tokens("foo bar_baz MyAgent _private x123 Ai Io Schedule");
    assert_eq!(
        toks,
        vec![
            Token::Ident("foo".into()),
            Token::Ident("bar_baz".into()),
            Token::Ident("MyAgent".into()),
            Token::Ident("_private".into()),
            Token::Ident("x123".into()),
            Token::Ident("Ai".into()),
            Token::Ident("Io".into()),
            Token::Ident("Schedule".into()),
        ]
    );
}

#[test]
fn lex_keyword_prefix_is_ident() {
    // "agents" should be an Ident, not Agent + "s"
    let toks = tokens("agents tasks selfish");
    assert_eq!(
        toks,
        vec![
            Token::Ident("agents".into()),
            Token::Ident("tasks".into()),
            Token::Ident("selfish".into()),
        ]
    );
}

// ─── Literals ────────────────────────────────────────────────────────────────

#[test]
fn lex_integers() {
    let toks = tokens("0 42 12345");
    assert_eq!(
        toks,
        vec![
            Token::Integer("0".into()),
            Token::Integer("42".into()),
            Token::Integer("12345".into()),
        ]
    );
}

#[test]
fn lex_floats() {
    let toks = tokens("3.14 0.5 100.0");
    assert_eq!(
        toks,
        vec![
            Token::Float("3.14".into()),
            Token::Float("0.5".into()),
            Token::Float("100.0".into()),
        ]
    );
}

#[test]
fn lex_duration_not_float() {
    // 5.minutes must tokenise as Integer, Dot, Ident — the parser
    // recognises the unit; the lexer doesn't.
    let toks = tokens("5.minutes");
    assert_eq!(
        toks,
        vec![
            Token::Integer("5".into()),
            Token::Dot,
            Token::Ident("minutes".into()),
        ]
    );
}

#[test]
fn lex_string_literal() {
    let toks = tokens(r#""hello world""#);
    assert_eq!(toks, vec![Token::StringLit("hello world".into())]);
}

#[test]
fn lex_string_with_interpolation() {
    let toks = tokens(r#""Hello, {name}!""#);
    assert_eq!(toks, vec![Token::StringLit("Hello, {name}!".into())]);
}

#[test]
fn lex_string_with_escapes() {
    let toks = tokens(r#""line1\nline2""#);
    assert_eq!(toks, vec![Token::StringLit(r"line1\nline2".into())]);
}

#[test]
fn lex_empty_string() {
    let toks = tokens(r#""""#);
    assert_eq!(toks, vec![Token::StringLit("".into())]);
}

#[test]
fn lex_triple_quoted_single_line() {
    let toks = tokens(r#""""hello world""""#);
    assert_eq!(toks, vec![Token::StringLit("hello world".into())]);
}

#[test]
fn lex_triple_quoted_multi_line() {
    let src = "\"\"\"first line\nsecond line\n  third\"\"\"";
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![Token::StringLit("first line\nsecond line\n  third".into())]
    );
}

#[test]
fn lex_triple_quoted_allows_interior_single_quote() {
    let src = "\"\"\"he said \"ok\" and left\"\"\"";
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![Token::StringLit("he said \"ok\" and left".into())]
    );
}

// ─── Operators ───────────────────────────────────────────────────────────────

#[test]
fn lex_multi_char_operators() {
    let toks = tokens("=> -> |> == != <= >= ?? ?.");
    assert_eq!(
        toks,
        vec![
            Token::FatArrow,
            Token::Arrow,
            Token::Pipe,
            Token::EqEq,
            Token::Neq,
            Token::Lte,
            Token::Gte,
            Token::NullCoalesce,
            Token::NullDot,
        ]
    );
}

#[test]
fn lex_single_char_operators() {
    let toks = tokens("= + - * / % ! . , : | ? @");
    assert_eq!(
        toks,
        vec![
            Token::Eq,
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::Percent,
            Token::Bang,
            Token::Dot,
            Token::Comma,
            Token::Colon,
            Token::Bar,
            Token::Question,
            Token::AtSign,
        ]
    );
}

#[test]
fn lex_delimiters() {
    let toks = tokens("{ } [ ] ( )");
    assert_eq!(
        toks,
        vec![
            Token::LBrace,
            Token::RBrace,
            Token::LBracket,
            Token::RBracket,
            Token::LParen,
            Token::RParen,
        ]
    );
}

// ─── Comments ────────────────────────────────────────────────────────────────

#[test]
fn lex_comments_skipped() {
    let toks = tokens("agent # this is a comment\ntask");
    assert_eq!(toks, vec![Token::Agent, Token::Newline, Token::Task]);
}

#[test]
fn lex_full_line_comment() {
    let toks = tokens("# full line comment");
    assert_eq!(toks, vec![]);
}

// ─── Newline normalization ───────────────────────────────────────────────────

#[test]
fn newline_as_separator() {
    let toks = tokens_with_newlines("x = 1\ny = 2");
    assert!(toks.contains(&Token::Newline), "Expected newline separator");
}

#[test]
fn newline_suppressed_after_opening_brace() {
    let toks = tokens_with_newlines("{\nx");
    assert!(!toks.iter().any(|t| *t == Token::Newline));
}

#[test]
fn newline_suppressed_before_closing_brace() {
    let toks = tokens_with_newlines("x\n}");
    assert!(!toks.iter().any(|t| *t == Token::Newline));
}

#[test]
fn newline_suppressed_after_comma() {
    let toks = tokens_with_newlines("a,\nb");
    assert!(!toks.iter().any(|t| *t == Token::Newline));
}

#[test]
fn newline_suppressed_after_equals() {
    let toks = tokens_with_newlines("x =\n42");
    assert!(!toks.iter().any(|t| *t == Token::Newline));
}

#[test]
fn newline_suppressed_before_null_coalesce() {
    let toks = tokens_with_newlines("x\n?? y");
    assert!(!toks.iter().any(|t| *t == Token::Newline));
}

#[test]
fn newline_suppressed_after_attribute_marker() {
    // `@role\n"foo"` should not separate — attribute bodies continue
    // onto the next line.
    let toks = tokens_with_newlines("@\nrole");
    assert!(!toks.iter().any(|t| *t == Token::Newline));
}

#[test]
fn consecutive_newlines_collapsed() {
    let toks = tokens_with_newlines("a\n\n\nb");
    let newline_count = toks.iter().filter(|t| **t == Token::Newline).count();
    assert_eq!(newline_count, 1);
}

// ─── Error handling ──────────────────────────────────────────────────────────

#[test]
fn lex_error_on_invalid_char() {
    let named = NamedSource::new("test.keel", "hello $ world".to_string());
    let result = lex("hello $ world", &named);
    assert!(result.is_err());
}

// ─── Complex token sequences ─────────────────────────────────────────────────

#[test]
fn lex_agent_declaration_with_attribute() {
    let src = r#"agent Hello { @role "greeter" }"#;
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Agent,
            Token::Ident("Hello".into()),
            Token::LBrace,
            Token::AtSign,
            Token::Ident("role".into()),
            Token::StringLit("greeter".into()),
            Token::RBrace,
        ]
    );
}

#[test]
fn lex_namespace_call() {
    let src = "Ai.classify(email.body, as: Urgency)";
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Ident("Ai".into()),
            Token::Dot,
            Token::Ident("classify".into()),
            Token::LParen,
            Token::Ident("email".into()),
            Token::Dot,
            Token::Ident("body".into()),
            Token::Comma,
            Token::As,
            Token::Colon,
            Token::Ident("Urgency".into()),
            Token::RParen,
        ]
    );
}

#[test]
fn lex_interface_declaration() {
    let src = "interface LlmProvider { task complete() -> str }";
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Interface,
            Token::Ident("LlmProvider".into()),
            Token::LBrace,
            Token::Task,
            Token::Ident("complete".into()),
            Token::LParen,
            Token::RParen,
            Token::Arrow,
            Token::Ident("str".into()),
            Token::RBrace,
        ]
    );
}

#[test]
fn lex_type_declaration() {
    let src = "type Urgency = low | medium | high | critical";
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Type,
            Token::Ident("Urgency".into()),
            Token::Eq,
            Token::Ident("low".into()),
            Token::Bar,
            Token::Ident("medium".into()),
            Token::Bar,
            Token::Ident("high".into()),
            Token::Bar,
            Token::Ident("critical".into()),
        ]
    );
}

#[test]
fn lex_null_chain() {
    let src = "email?.subject ?? \"(none)\"";
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Ident("email".into()),
            Token::NullDot,
            Token::Ident("subject".into()),
            Token::NullCoalesce,
            Token::StringLit("(none)".into()),
        ]
    );
}
