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
// Keel v0.1 reserves 27 words. Everything else is an identifier, including
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
    #[token("now")]
    Now,

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
    #[regex(r#""([^"\\]|\\.)*""#, lex_string)]
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

/// Strip surrounding quotes from a string literal; escapes are resolved
/// later during parsing / interpolation processing.
fn lex_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let slice = lex.slice();
    let inner = &slice[1..slice.len() - 1];
    Some(inner.to_string())
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
            Token::As => write!(f, "as"),
            Token::And => write!(f, "and"),
            Token::Or => write!(f, "or"),
            Token::Not => write!(f, "not"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::None_ => write!(f, "none"),
            Token::Now => write!(f, "now"),
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

pub fn lex(
    source: &str,
    named_src: &NamedSource<String>,
) -> miette::Result<Vec<Spanned<Token>>> {
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

fn normalize_newlines(tokens: Vec<Spanned<Token>>) -> Vec<Spanned<Token>> {
    if tokens.is_empty() {
        return tokens;
    }

    let mut result: Vec<Spanned<Token>> = Vec::with_capacity(tokens.len());

    for (token, span) in tokens {
        if token == Token::Newline {
            if result.is_empty() {
                continue;
            }
            if let Some((prev, _)) = result.last() {
                if continues_to_next_line(prev) {
                    continue;
                }
            }
            if let Some((Token::Newline, _)) = result.last() {
                continue;
            }
            result.push((token, span));
        } else {
            if continues_from_prev_line(&token) {
                if let Some((Token::Newline, _)) = result.last() {
                    result.pop();
                }
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
