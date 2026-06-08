//! Lexer for the Keel language.
//!
//! Tokenises source text using [`logos`]. Newlines are normalised to a single
//! `Token::Newline` variant and used as statement separators by the parser.
//! The [`Span`] type alias (`Range<usize>`) is the shared source-position
//! currency across the whole pipeline.

use logos::Logos;
use miette::NamedSource;
use std::fmt;
use std::ops::Range;

pub type Span = Range<usize>;
pub type Spanned<T> = (T, Span);

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------
//
// Keel v0.1 reserves 30 words. Everything else is an identifier, including
// stdlib namespaces (`Ai`, `Io`, …), attribute names (`@role`, `@model`, …),
// and duration units (`minutes`, `hours`, …).
//
// See SPEC.md §10.

#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r]+")]
pub enum Token {
    // ── Declarations ─────────────────────────────────────────────────
    #[token("agent")]
    Agent,
    #[token("task")]
    Task,
    #[token("interface")]
    Interface,
    #[token("impl")]
    Impl,
    #[token("type")]
    Type,
    #[token("extern")]
    Extern,

    // ── Modules ──────────────────────────────────────────────────────
    #[token("use")]
    Use,
    #[token("from")]
    From,

    // ── Agent body ───────────────────────────────────────────────────
    #[token("state")]
    State,
    #[token("on")]
    On,
    #[token("self")]
    SelfKw,

    // ── Control flow ─────────────────────────────────────────────────
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("when")]
    When,
    #[token("where")]
    Where,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("try")]
    Try,
    #[token("catch")]
    Catch,
    #[token("return")]
    Return,
    #[token("raise")]
    Raise,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("while")]
    While,

    // ── Cast / logic ─────────────────────────────────────────────────
    #[token("as")]
    As,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,

    // ── Value literals ───────────────────────────────────────────────
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("none")]
    None_,
    // ── Set literal form (`set[1, 2, 3]`) ────────────────────────────
    #[token("set")]
    Set,

    // ── Numbers ──────────────────────────────────────────────────────
    // Float must come before Integer by length. logos picks longest,
    // so "3.14" matches Float but "5.minutes" splits to Integer Dot Ident.
    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().to_string())]
    Float(String),

    #[regex(r"[0-9]+", |lex| lex.slice().to_string())]
    Integer(String),

    // ── String literal ───────────────────────────────────────────────
    // Triple-quoted form comes first because its opening `"""` would
    // otherwise be matched as empty-string + start-of-string. Logos
    // picks the longest match, so the three-quote opener wins.
    #[token("\"\"\"", lex_triple_string)]
    #[token("\"", lex_string)]
    StringLit(String),

    // ── Identifier ───────────────────────────────────────────────────
    // Must come after all keyword tokens; logos prioritises exact
    // `#[token]` matches over `#[regex]` of the same length.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // ── Multi-char operators ─────────────────────────────────────────
    #[token("=>")]
    FatArrow,
    #[token("->")]
    Arrow,
    #[token("|>")]
    Pipe,
    #[token("==")]
    EqEq,
    #[token("!=")]
    Neq,
    #[token("<=")]
    Lte,
    #[token(">=")]
    Gte,
    #[token("??")]
    NullCoalesce,
    #[token("?.")]
    NullDot,
    #[token("...")]
    DotDotDot,
    #[token("..")]
    DotDot,
    // Augmented assignment — must be listed before single-char `+` `-` `*` `/`
    // so the logos DFA picks the longer match first.
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("%=")]
    PercentEq,

    // ── Single-char operators ────────────────────────────────────────
    #[token("=")]
    Eq,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("!")]
    Bang,
    #[token(".")]
    Dot,
    #[token("|")]
    Bar,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token("?")]
    Question,
    #[token("@")]
    AtSign,

    // ── Delimiters ───────────────────────────────────────────────────
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,

    // ── Newline (statement separator) ────────────────────────────────
    #[token("\n")]
    Newline,

    // ── Comments (skipped) ───────────────────────────────────────────
    #[regex(r"#[^\n]*", logos::skip)]
    Comment,
}

/// Scan a single-quoted string body starting just after the opening `"`.
/// Handles escapes (`\X`) and tracks brace depth inside `{...}`
/// interpolation slots, which themselves may contain nested string
/// literals (e.g. `"outer {"inner"}"`). Returns the inner body verbatim
/// — escape resolution happens later in `parse_interpolation`.
fn lex_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let rest = lex.remainder();
    let (content, consumed) = scan_string_body(rest)?;
    lex.bump(consumed);
    Some(content)
}

/// Recursively scan a string body up to the closing `"` (not included
/// in `content`, but included in `consumed`). Returns `None` if the
/// string is unterminated.
///
/// # Errors
///
/// Returns `None` for unterminated strings (missing closing `"` or unbalanced
/// interpolation braces).
fn scan_string_body(rest: &str) -> Option<(String, usize)> {
    let bytes = rest.as_bytes();
    let mut i = 0;
    // Estimate: most string bodies fit in the remaining bytes.
    let mut content = String::with_capacity(rest.len());

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            content.push('\\');
            let next = rest[i + 1..].chars().next()?;
            content.push(next);
            i += 1 + next.len_utf8();
        } else if b == b'"' {
            return Some((content, i + 1));
        } else if b == b'{' {
            content.push('{');
            i += 1;
            let mut depth: u32 = 1;
            while i < bytes.len() && depth > 0 {
                let bb = bytes[i];
                if bb == b'\\' && i + 1 < bytes.len() {
                    content.push('\\');
                    let next = rest[i + 1..].chars().next()?;
                    content.push(next);
                    i += 1 + next.len_utf8();
                } else if bb == b'{' {
                    depth += 1;
                    content.push('{');
                    i += 1;
                } else if bb == b'}' {
                    depth -= 1;
                    content.push('}');
                    i += 1;
                } else if bb == b'"' {
                    // Recursive scan of a nested string inside the slot.
                    content.push('"');
                    i += 1;
                    let (inner, consumed) = scan_string_body(&rest[i..])?;
                    content.push_str(&inner);
                    content.push('"');
                    i += consumed;
                } else {
                    let next = rest[i..].chars().next()?;
                    content.push(next);
                    i += next.len_utf8();
                }
            }
            if depth > 0 {
                return None;
            }
        } else {
            let next = rest[i..].chars().next()?;
            content.push(next);
            i += next.len_utf8();
        }
    }

    None
}

/// Consume a triple-quoted string starting from the opening `"""`.
/// Scans the remainder until the next `"""`, returns the interior as
/// the token payload, and advances the lexer past the closing delimiter.
/// Multi-line content is preserved verbatim; interior `"` characters
/// are allowed as long as they aren't three in a row.
fn lex_triple_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let rest = lex.remainder();
    match rest.find("\"\"\"") {
        Some(end) => {
            let content = rest[..end].to_string();
            lex.bump(end + 3);
            Some(content)
        }
        None => None, // unterminated triple-quoted string
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Agent => write!(f, "agent"),
            Token::Task => write!(f, "task"),
            Token::Interface => write!(f, "interface"),
            Token::Impl => write!(f, "impl"),
            Token::Type => write!(f, "type"),
            Token::Extern => write!(f, "extern"),
            Token::Use => write!(f, "use"),
            Token::From => write!(f, "from"),
            Token::State => write!(f, "state"),
            Token::On => write!(f, "on"),
            Token::SelfKw => write!(f, "self"),
            Token::If => write!(f, "if"),
            Token::Else => write!(f, "else"),
            Token::When => write!(f, "when"),
            Token::Where => write!(f, "where"),
            Token::For => write!(f, "for"),
            Token::In => write!(f, "in"),
            Token::Try => write!(f, "try"),
            Token::Catch => write!(f, "catch"),
            Token::Return => write!(f, "return"),
            Token::Raise => write!(f, "raise"),
            Token::Break => write!(f, "break"),
            Token::Continue => write!(f, "continue"),
            Token::While => write!(f, "while"),
            Token::As => write!(f, "as"),
            Token::And => write!(f, "and"),
            Token::Or => write!(f, "or"),
            Token::Not => write!(f, "not"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::None_ => write!(f, "none"),
            Token::Set => write!(f, "set"),
            Token::Float(v) => write!(f, "{v}"),
            Token::Integer(v) => write!(f, "{v}"),
            Token::StringLit(s) => write!(f, "\"{s}\""),
            Token::Ident(s) => write!(f, "{s}"),
            Token::FatArrow => write!(f, "=>"),
            Token::Arrow => write!(f, "->"),
            Token::Pipe => write!(f, "|>"),
            Token::EqEq => write!(f, "=="),
            Token::Neq => write!(f, "!="),
            Token::Lte => write!(f, "<="),
            Token::Gte => write!(f, ">="),
            Token::NullCoalesce => write!(f, "??"),
            Token::NullDot => write!(f, "?."),
            Token::DotDotDot => write!(f, "..."),
            Token::DotDot => write!(f, ".."),
            Token::PlusEq => write!(f, "+="),
            Token::MinusEq => write!(f, "-="),
            Token::StarEq => write!(f, "*="),
            Token::SlashEq => write!(f, "/="),
            Token::PercentEq => write!(f, "%="),
            Token::Eq => write!(f, "="),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Percent => write!(f, "%"),
            Token::Bang => write!(f, "!"),
            Token::Dot => write!(f, "."),
            Token::Bar => write!(f, "|"),
            Token::Comma => write!(f, ","),
            Token::Colon => write!(f, ":"),
            Token::Question => write!(f, "?"),
            Token::AtSign => write!(f, "@"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::Newline => write!(f, "newline"),
            Token::Comment => write!(f, "comment"),
        }
    }
}

// ---------------------------------------------------------------------------
// Lexer entry point
// ---------------------------------------------------------------------------

/// Lex the source string into a vector of spanned tokens with normalised newlines.
///
/// # Errors
///
/// Returns a miette error with source-span labels if any unexpected character
/// is encountered that doesn't match a valid token pattern.
pub fn lex(source: &str, named_src: &NamedSource<String>) -> miette::Result<Vec<Spanned<Token>>> {
    let lexer = Token::lexer(source);
    let mut raw_tokens: Vec<Spanned<Token>> = Vec::new();

    for (result, span) in lexer.spanned() {
        match result {
            Ok(token) => raw_tokens.push((token, span)),
            Err(()) => {
                return Err(miette::miette!(
                    labels = vec![miette::LabeledSpan::at(span, "unexpected character")],
                    "Unexpected character in source",
                )
                .with_source_code(named_src.clone()));
            }
        }
    }

    Ok(normalize_newlines(raw_tokens))
}

// ---------------------------------------------------------------------------
// Newline normalization
// ---------------------------------------------------------------------------
// Keel uses newlines as statement separators. This pass:
//   1. Removes newlines where they can't be statement boundaries
//      (after opening delimiters, operators, commas, etc.)
//   2. Removes newlines before closing delimiters, `else`, `catch`
//   3. Collapses consecutive newlines into one
//   4. Strips leading/trailing newlines

pub(crate) fn normalize_newlines(tokens: Vec<Spanned<Token>>) -> Vec<Spanned<Token>> {
    if tokens.is_empty() {
        return tokens;
    }

    let mut result: Vec<Spanned<Token>> = Vec::with_capacity(tokens.len());

    for (token, span) in tokens {
        if token == Token::Newline {
            if result.is_empty() {
                continue;
            }
            if let Some((prev, _)) = result.last()
                && continues_to_next_line(prev)
            {
                continue;
            }
            if let Some((Token::Newline, _)) = result.last() {
                continue;
            }
            result.push((token, span));
        } else {
            if continues_from_prev_line(&token)
                && let Some((Token::Newline, _)) = result.last()
            {
                result.pop();
            }
            result.push((token, span));
        }
    }

    if let Some((Token::Newline, _)) = result.last() {
        result.pop();
    }

    result
}

/// Tokens after which a newline is NOT a statement separator.
fn continues_to_next_line(token: &Token) -> bool {
    matches!(
        token,
        Token::LBrace
            | Token::LBracket
            | Token::LParen
            | Token::Comma
            | Token::Eq
            | Token::PlusEq
            | Token::MinusEq
            | Token::StarEq
            | Token::SlashEq
            | Token::PercentEq
            | Token::FatArrow
            | Token::Arrow
            | Token::Pipe
            | Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::EqEq
            | Token::Neq
            | Token::Lt
            | Token::Gt
            | Token::Lte
            | Token::Gte
            | Token::NullCoalesce
            | Token::And
            | Token::Or
            | Token::Bar
            | Token::Colon
            | Token::Dot
            | Token::NullDot
            | Token::DotDot
            | Token::In
            | Token::As
            | Token::Where
            | Token::From
            | Token::AtSign
    )
}

/// Tokens before which a newline is NOT a statement separator.
fn continues_from_prev_line(token: &Token) -> bool {
    matches!(
        token,
        Token::RBrace
            | Token::RBracket
            | Token::RParen
            | Token::Else
            | Token::Catch
            | Token::FatArrow
            | Token::Pipe
            | Token::NullCoalesce
            | Token::Bar
            | Token::Dot
            | Token::NullDot
    )
}

#[cfg(test)]
mod tests {
    use super::{Token, lex};
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
        assert_eq!(toks, vec![Token::As, Token::And, Token::Or, Token::Not]);
    }

    #[test]
    fn lex_value_literals() {
        let toks = tokens("true false none");
        assert_eq!(toks, vec![Token::True, Token::False, Token::None_]);
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

    #[test]
    fn removed_keywords_are_identifiers() {
        let src = "classify draft every fetch send ask confirm notify role model tools test mock setup assert";
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
                Token::Ident("test".into()),
                Token::Ident("mock".into()),
                Token::Ident("setup".into()),
                Token::Ident("assert".into()),
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
        assert!(!toks.contains(&Token::Newline));
    }

    #[test]
    fn newline_suppressed_before_closing_brace() {
        let toks = tokens_with_newlines("x\n}");
        assert!(!toks.contains(&Token::Newline));
    }

    #[test]
    fn newline_suppressed_after_comma() {
        let toks = tokens_with_newlines("a,\nb");
        assert!(!toks.contains(&Token::Newline));
    }

    #[test]
    fn newline_suppressed_after_equals() {
        let toks = tokens_with_newlines("x =\n42");
        assert!(!toks.contains(&Token::Newline));
    }

    #[test]
    fn newline_suppressed_before_null_coalesce() {
        let toks = tokens_with_newlines("x\n?? y");
        assert!(!toks.contains(&Token::Newline));
    }

    #[test]
    fn newline_suppressed_after_attribute_marker() {
        let toks = tokens_with_newlines("@\nrole");
        assert!(!toks.contains(&Token::Newline));
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

    // ─── Variadic / spread ────────────────────────────────────────────────────────

    #[test]
    fn lex_dot_dot_dot_distinct_from_dot_dot() {
        let toks = tokens("0..10 ...x");
        assert_eq!(
            toks,
            vec![
                Token::Integer("0".into()),
                Token::DotDot,
                Token::Integer("10".into()),
                Token::DotDotDot,
                Token::Ident("x".into()),
            ]
        );
    }

    #[test]
    fn lex_spread_in_call() {
        let toks = tokens("f(...items)");
        assert_eq!(
            toks,
            vec![
                Token::Ident("f".into()),
                Token::LParen,
                Token::DotDotDot,
                Token::Ident("items".into()),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn lex_impl_keyword() {
        let toks = tokens("impl Stringable for Point");
        assert_eq!(
            toks,
            vec![
                Token::Impl,
                Token::Ident("Stringable".to_string()),
                Token::For,
                Token::Ident("Point".to_string()),
            ]
        );
    }
}
