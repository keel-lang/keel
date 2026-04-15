use logos::Logos;
use miette::NamedSource;
use std::fmt;
use std::ops::Range;

pub type Span = Range<usize>;
pub type Spanned<T> = (T, Span);

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r]+")]
pub enum Token {
    // ── Keywords: core ────────────────────────────────────────────────
    #[token("agent")]
    Agent,
    #[token("task")]
    Task,
    #[token("role")]
    Role,
    #[token("model")]
    Model,
    #[token("tools")]
    Tools,
    #[token("connect")]
    Connect,
    #[token("memory")]
    Memory,
    #[token("state")]
    State,
    #[token("config")]
    Config,
    #[token("type")]
    Type,
    #[token("run")]
    Run,
    #[token("stop")]
    Stop,
    #[token("self")]
    SelfKw,
    #[token("env")]
    Env,
    #[token("user")]
    User,

    // ── Keywords: AI primitives ──────────────────────────────────────
    #[token("classify")]
    Classify,
    #[token("extract")]
    Extract,
    #[token("summarize")]
    Summarize,
    #[token("draft")]
    Draft,
    #[token("translate")]
    Translate,
    #[token("decide")]
    Decide,
    #[token("prompt")]
    Prompt,

    // ── Keywords: AI modifiers ───────────────────────────────────────
    #[token("as")]
    As,
    #[token("considering")]
    Considering,
    #[token("fallback")]
    Fallback,
    #[token("using")]
    Using,
    #[token("format")]
    Format,

    // ── Keywords: human interaction ──────────────────────────────────
    #[token("ask")]
    Ask,
    #[token("confirm")]
    Confirm,
    #[token("notify")]
    Notify,
    #[token("show")]
    Show,

    // ── Keywords: web / external ─────────────────────────────────────
    #[token("fetch")]
    Fetch,
    #[token("send")]
    Send,
    #[token("search")]
    Search,
    #[token("archive")]
    Archive,
    #[token("http")]
    Http,
    #[token("sql")]
    Sql,

    // ── Keywords: time / scheduling ──────────────────────────────────
    #[token("every")]
    Every,
    #[token("after")]
    After,
    #[token("at")]
    At,
    #[token("wait")]
    Wait,

    // ── Keywords: control flow ───────────────────────────────────────
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("when")]
    When,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("where")]
    Where,
    #[token("return")]
    Return,
    #[token("try")]
    Try,
    #[token("catch")]
    Catch,
    #[token("retry")]
    Retry,
    #[token("then")]
    Then,

    // ── Keywords: multi-agent (v2, recognized now) ───────────────────
    #[token("delegate")]
    Delegate,
    #[token("broadcast")]
    Broadcast,
    #[token("team")]
    Team,

    // ── Keywords: memory (v2, recognized now) ────────────────────────
    #[token("remember")]
    Remember,
    #[token("recall")]
    Recall,
    #[token("forget")]
    Forget,

    // ── Keywords: misc ───────────────────────────────────────────────
    #[token("use")]
    Use,
    #[token("from")]
    From,
    #[token("to")]
    To,
    #[token("via")]
    Via,
    #[token("with")]
    With,
    #[token("extern")]
    Extern,
    #[token("parallel")]
    Parallel,
    #[token("race")]
    Race,
    #[token("set")]
    Set,
    #[token("on")]
    On,
    #[token("options")]
    Options,
    #[token("background")]
    Background,
    #[token("persistent")]
    Persistent,
    #[token("session")]
    Session,
    #[token("rules")]
    Rules,
    #[token("limits")]
    Limits,
    #[token("times")]
    Times,
    #[token("backoff")]
    Backoff,

    // ── Built-in value literals ──────────────────────────────────────
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("none")]
    None_,
    #[token("now")]
    Now,

    // ── Boolean operators ────────────────────────────────────────────
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,

    // ── Number literals ──────────────────────────────────────────────
    // Float must come before Integer in matching priority.
    // logos picks the longest match, so "3.14" matches Float (4 chars)
    // while "3" matches Integer (1 char). "5.minutes" → Integer(5), Dot, Ident.
    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice().to_string())]
    Float(String),

    #[regex(r"[0-9]+", |lex| lex.slice().to_string())]
    Integer(String),

    // ── String literal ───────────────────────────────────────────────
    // Captures content between double quotes, handling escape sequences.
    // Interpolation ({expr}) is stored raw and processed during parsing.
    #[regex(r#""([^"\\]|\\.)*""#, lex_string)]
    StringLit(String),

    // ── Identifier ───────────────────────────────────────────────────
    // Must come after all keyword tokens — logos gives priority to
    // exact `#[token]` matches over `#[regex]` at the same length.
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

    // ── Newline (significant for statement separation) ───────────────
    #[token("\n")]
    Newline,

    // ── Comments (skipped, but newline after is preserved) ───────────
    #[regex(r"#[^\n]*", logos::skip)]
    Comment,
}

/// Extract string content between quotes, handling basic escape sequences.
fn lex_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let slice = lex.slice();
    // Strip surrounding quotes
    let inner = &slice[1..slice.len() - 1];
    Some(inner.to_string())
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Agent => write!(f, "agent"),
            Token::Task => write!(f, "task"),
            Token::Role => write!(f, "role"),
            Token::Model => write!(f, "model"),
            Token::Tools => write!(f, "tools"),
            Token::Connect => write!(f, "connect"),
            Token::Memory => write!(f, "memory"),
            Token::State => write!(f, "state"),
            Token::Config => write!(f, "config"),
            Token::Type => write!(f, "type"),
            Token::Run => write!(f, "run"),
            Token::Stop => write!(f, "stop"),
            Token::SelfKw => write!(f, "self"),
            Token::Env => write!(f, "env"),
            Token::User => write!(f, "user"),
            Token::Classify => write!(f, "classify"),
            Token::Extract => write!(f, "extract"),
            Token::Summarize => write!(f, "summarize"),
            Token::Draft => write!(f, "draft"),
            Token::Translate => write!(f, "translate"),
            Token::Decide => write!(f, "decide"),
            Token::Prompt => write!(f, "prompt"),
            Token::As => write!(f, "as"),
            Token::Considering => write!(f, "considering"),
            Token::Fallback => write!(f, "fallback"),
            Token::Using => write!(f, "using"),
            Token::Format => write!(f, "format"),
            Token::Ask => write!(f, "ask"),
            Token::Confirm => write!(f, "confirm"),
            Token::Notify => write!(f, "notify"),
            Token::Show => write!(f, "show"),
            Token::Fetch => write!(f, "fetch"),
            Token::Send => write!(f, "send"),
            Token::Search => write!(f, "search"),
            Token::Archive => write!(f, "archive"),
            Token::Http => write!(f, "http"),
            Token::Sql => write!(f, "sql"),
            Token::Every => write!(f, "every"),
            Token::After => write!(f, "after"),
            Token::At => write!(f, "at"),
            Token::Wait => write!(f, "wait"),
            Token::If => write!(f, "if"),
            Token::Else => write!(f, "else"),
            Token::When => write!(f, "when"),
            Token::For => write!(f, "for"),
            Token::In => write!(f, "in"),
            Token::Where => write!(f, "where"),
            Token::Return => write!(f, "return"),
            Token::Try => write!(f, "try"),
            Token::Catch => write!(f, "catch"),
            Token::Retry => write!(f, "retry"),
            Token::Then => write!(f, "then"),
            Token::Delegate => write!(f, "delegate"),
            Token::Broadcast => write!(f, "broadcast"),
            Token::Team => write!(f, "team"),
            Token::Remember => write!(f, "remember"),
            Token::Recall => write!(f, "recall"),
            Token::Forget => write!(f, "forget"),
            Token::Use => write!(f, "use"),
            Token::From => write!(f, "from"),
            Token::To => write!(f, "to"),
            Token::Via => write!(f, "via"),
            Token::With => write!(f, "with"),
            Token::Extern => write!(f, "extern"),
            Token::Parallel => write!(f, "parallel"),
            Token::Race => write!(f, "race"),
            Token::Set => write!(f, "set"),
            Token::On => write!(f, "on"),
            Token::Options => write!(f, "options"),
            Token::Background => write!(f, "background"),
            Token::Persistent => write!(f, "persistent"),
            Token::Session => write!(f, "session"),
            Token::Rules => write!(f, "rules"),
            Token::Limits => write!(f, "limits"),
            Token::Times => write!(f, "times"),
            Token::Backoff => write!(f, "backoff"),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::None_ => write!(f, "none"),
            Token::Now => write!(f, "now"),
            Token::And => write!(f, "and"),
            Token::Or => write!(f, "or"),
            Token::Not => write!(f, "not"),
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
            // Skip newline if there's nothing before it
            if result.is_empty() {
                continue;
            }
            // Skip newline if the previous token continues to the next line
            if let Some((prev, _)) = result.last() {
                if continues_to_next_line(prev) {
                    continue;
                }
            }
            // Skip consecutive newlines
            if let Some((Token::Newline, _)) = result.last() {
                continue;
            }
            result.push((token, span));
        } else {
            // Remove preceding newline if this token continues from previous line
            if continues_from_prev_line(&token) {
                if let Some((Token::Newline, _)) = result.last() {
                    result.pop();
                }
            }
            result.push((token, span));
        }
    }

    // Strip trailing newline
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
            | Token::Considering
            | Token::Fallback
            | Token::Using
            | Token::Then
            | Token::In
            | Token::As
            | Token::To
            | Token::Via
            | Token::With
            | Token::Where
            | Token::From
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
            | Token::Then
            | Token::FatArrow
            | Token::Pipe
            | Token::NullCoalesce
            | Token::Bar
    )
}
