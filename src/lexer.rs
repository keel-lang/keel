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
