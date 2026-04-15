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
    // Same as tokens() — newlines are preserved after normalization
    tokens(source)
}

// ─── Keywords ────────────────────────────────────────────────────────────────

#[test]
fn lex_keywords() {
    let toks = tokens("agent task role model type run");
    assert_eq!(
        toks,
        vec![
            Token::Agent,
            Token::Task,
            Token::Role,
            Token::Model,
            Token::Type,
            Token::Run,
        ]
    );
}

#[test]
fn lex_ai_keywords() {
    let toks = tokens("classify extract summarize draft translate decide");
    assert_eq!(
        toks,
        vec![
            Token::Classify,
            Token::Extract,
            Token::Summarize,
            Token::Draft,
            Token::Translate,
            Token::Decide,
        ]
    );
}

#[test]
fn lex_control_flow_keywords() {
    let toks = tokens("if else when for in where return try catch");
    assert_eq!(
        toks,
        vec![
            Token::If,
            Token::Else,
            Token::When,
            Token::For,
            Token::In,
            Token::Where,
            Token::Return,
            Token::Try,
            Token::Catch,
        ]
    );
}

#[test]
fn lex_human_keywords() {
    let toks = tokens("ask confirm notify show user");
    assert_eq!(
        toks,
        vec![
            Token::Ask,
            Token::Confirm,
            Token::Notify,
            Token::Show,
            Token::User,
        ]
    );
}

#[test]
fn lex_boolean_keywords() {
    let toks = tokens("true false none now and or not");
    assert_eq!(
        toks,
        vec![
            Token::True,
            Token::False,
            Token::None_,
            Token::Now,
            Token::And,
            Token::Or,
            Token::Not,
        ]
    );
}

// ─── Identifiers ─────────────────────────────────────────────────────────────

#[test]
fn lex_identifiers() {
    let toks = tokens("foo bar_baz MyAgent _private x123");
    assert_eq!(
        toks,
        vec![
            Token::Ident("foo".into()),
            Token::Ident("bar_baz".into()),
            Token::Ident("MyAgent".into()),
            Token::Ident("_private".into()),
            Token::Ident("x123".into()),
        ]
    );
}

#[test]
fn lex_keyword_prefix_is_ident() {
    // "agents" should be an Ident, not Agent + "s"
    let toks = tokens("agents tasks modeled");
    assert_eq!(
        toks,
        vec![
            Token::Ident("agents".into()),
            Token::Ident("tasks".into()),
            Token::Ident("modeled".into()),
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
    // 5.minutes should be Integer + Dot + Ident, NOT Float
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
    let toks = tokens("= + - * / % ! . , : | ?");
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
    assert_eq!(
        toks,
        vec![Token::Agent, Token::Newline, Token::Task]
    );
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
fn lex_agent_declaration() {
    let src = r#"agent Hello { role "greeter" }"#;
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Agent,
            Token::Ident("Hello".into()),
            Token::LBrace,
            Token::Role,
            Token::StringLit("greeter".into()),
            Token::RBrace,
        ]
    );
}

#[test]
fn lex_classify_expression() {
    let src = r#"classify email.body as Urgency fallback medium"#;
    let toks = tokens(src);
    assert_eq!(
        toks,
        vec![
            Token::Classify,
            Token::Ident("email".into()),
            Token::Dot,
            Token::Ident("body".into()),
            Token::As,
            Token::Ident("Urgency".into()),
            Token::Fallback,
            Token::Ident("medium".into()),
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
