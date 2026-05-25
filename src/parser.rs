//! Parser for the Keel language.
//!
//! Built on [`chumsky`] 0.9. All sub-parsers return [`BoxedParser`] to avoid
//! the macOS linker crash caused by deeply nested chumsky type parameters.
//! Newlines serve as statement separators — the grammar is newline-sensitive
//! rather than semicolon-delimited.
#![allow(clippy::result_large_err)]

use chumsky::Stream;
use chumsky::prelude::*;
use miette::NamedSource;

use crate::ast::*;
use crate::lexer::{Span, Spanned, Token, normalize_newlines};

type P<T> = BoxedParser<'static, Token, T, Simple<Token>>;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a complete Keel program from a token stream.
///
/// # Errors
///
/// Returns a miette error with source-span labels if the token stream does not
/// form a valid Keel program.
pub fn parse(
    tokens: Vec<(Token, Span)>,
    source_len: usize,
    named_src: &NamedSource<String>,
) -> miette::Result<Program> {
    let eoi = source_len..source_len + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    match program_parser().parse(stream) {
        Ok(program) => Ok(program),
        Err(errors) => {
            let err = &errors[0];
            let span = err.span();
            Err(miette::miette!(
                labels = vec![miette::LabeledSpan::at(span, err.to_string())],
                "Parse error: {}",
                err
            )
            .with_source_code(named_src.clone()))
        }
    }
}

/// Parse a sequence of statements (REPL mode).
///
/// # Errors
///
/// Returns a miette error if the token stream does not form valid statements.
pub fn parse_stmts(
    tokens: Vec<(Token, Span)>,
    source_len: usize,
    named_src: &NamedSource<String>,
) -> miette::Result<Vec<Spanned<Stmt>>> {
    let eoi = source_len..source_len + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    let parser = newlines()
        .ignore_then(stmt_parser().separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(end());

    match parser.parse(stream) {
        Ok(stmts) => Ok(stmts),
        Err(errors) => {
            let err = &errors[0];
            let span = err.span();
            Err(miette::miette!(
                labels = vec![miette::LabeledSpan::at(span, err.to_string())],
                "Parse error: {}",
                err
            )
            .with_source_code(named_src.clone()))
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn newlines() -> P<()> {
    just(Token::Newline).repeated().ignored().boxed()
}

fn sep() -> P<()> {
    just(Token::Newline)
        .repeated()
        .at_least(1)
        .ignored()
        .boxed()
}

/// Separator for struct fields and items: comma, newline, or both.
fn field_sep() -> P<()> {
    just(Token::Comma)
        .then_ignore(newlines())
        .ignored()
        .or(sep())
        .boxed()
}

fn ident() -> P<String> {
    select! { Token::Ident(s) => s }.boxed()
}

/// Parses `{ field }` or `{ field: rename, field2, ... }`.
/// Returns `Vec<(source_field, local_name)>`.
/// Uses `field_name()` so keyword-named fields like `from` are accepted.
/// Used for struct destructure patterns in let bindings, for loops, and task params.
fn struct_destruct_pat() -> P<Vec<(String, String)>> {
    // Rename target must be a plain ident (not a keyword) to become a valid variable name.
    let entry = field_name()
        .then(just(Token::Colon).ignore_then(ident()).or_not())
        .map(|(src, rename)| {
            let local = rename.unwrap_or_else(|| src.clone());
            (src, local)
        });

    just(Token::LBrace)
        .ignore_then(newlines())
        .ignore_then(entry.separated_by(field_sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .boxed()
}

/// Parses `( a, b, c )` with at least 2 elements.
/// Returns `Vec<String>` of positional names.
fn tuple_destruct_pat() -> P<Vec<String>> {
    just(Token::LParen)
        .ignore_then(newlines())
        .ignore_then(
            ident()
                .separated_by(just(Token::Comma).then_ignore(newlines()))
                .at_least(2)
                .allow_trailing(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RParen))
        .boxed()
}

fn string_lit() -> P<String> {
    select! { Token::StringLit(s) => s }.boxed()
}

/// Identifier OR a small set of contextual keywords that users routinely
/// want as field / argument names (e.g. `{from: str}`, `{type: "x"}`,
/// `Email.fetch(from: box)`). These remain reserved in their normal
/// positions — only here do we allow them as names.
fn field_name() -> P<String> {
    select! {
        Token::Ident(s) => s,
        Token::From => "from".to_string(),
        Token::As => "as".to_string(),
        Token::In => "in".to_string(),
        Token::Where => "where".to_string(),
        Token::Type => "type".to_string(),
        Token::On => "on".to_string(),
        Token::State => "state".to_string(),
        Token::For => "for".to_string(),
        Token::Return => "return".to_string(),
        Token::Set => "set".to_string(),
    }
    .boxed()
}

/// Map / struct-literal key: a field_name or a string literal (strings
/// are raw-decoded into the key).  Used for `StructSpreadUpdate` overrides,
/// which are always identifier-shaped.
fn map_key() -> P<String> {
    field_name().or(plain_string()).boxed()
}

/// Full map-literal key parser — extends `map_key` with integer and boolean
/// literals so that `{1: "one"}` and `{true: "on"}` parse correctly.
fn map_lit_key() -> P<MapLitKey> {
    let ident_key = field_name().map(MapLitKey::Ident);
    let str_key = plain_string().map(MapLitKey::Str);
    let int_key = select! { Token::Integer(s) => s }
        .try_map(|s, span| {
            s.parse::<i64>()
                .map(MapLitKey::Int)
                .map_err(|_| Simple::custom(span, format!("integer key `{s}` overflows i64")))
        });
    let bool_key = just(Token::True)
        .to(MapLitKey::Bool(true))
        .or(just(Token::False).to(MapLitKey::Bool(false)));
    ident_key.or(str_key).or(int_key).or(bool_key).boxed()
}

/// Decode `\n`, `\t`, `\\`, `\"`, `\{`, `\}` in a raw string literal (no
/// interpolation). Used for attribute values, criteria keys, etc.
fn unescape_plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.peek() {
            Some('n') => {
                chars.next();
                out.push('\n');
            }
            Some('t') => {
                chars.next();
                out.push('\t');
            }
            Some('r') => {
                chars.next();
                out.push('\r');
            }
            Some('\\') => {
                chars.next();
                out.push('\\');
            }
            Some('"') => {
                chars.next();
                out.push('"');
            }
            Some('{') => {
                chars.next();
                out.push('{');
            }
            Some('}') => {
                chars.next();
                out.push('}');
            }
            Some(_) | None => out.push('\\'),
        }
    }
    out
}

fn plain_string() -> P<String> {
    string_lit().map(|s| unescape_plain(&s)).boxed()
}

fn integer_lit() -> P<i64> {
    select! { Token::Integer(s) => s }
        .try_map(|s, span| {
            s.parse::<i64>()
                .map_err(|_| Simple::custom(span, format!("integer literal `{s}` overflows i64")))
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Type expressions
// ---------------------------------------------------------------------------

fn type_expr() -> P<TypeExpr> {
    recursive(|ty: Recursive<Token, TypeExpr, Simple<Token>>| {
        let named = ident().map(TypeExpr::Named);

        let dynamic_ty = just(Token::Ident("dynamic".to_string())).to(TypeExpr::Dynamic);

        let struct_ty = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(
                field_name()
                    .then_ignore(just(Token::Colon))
                    .then(ty.clone())
                    .map(|(n, t)| Field { name: n, ty: t })
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map(TypeExpr::Struct);

        // Parenthesised types: `(T1, T2)` → Tuple, `(T1, T2) -> Ret` → Func.
        // Parsed as a single branch to avoid backtracking: consume the param
        // list once, then branch on whether `->` follows.
        let paren_ty = just(Token::LParen)
            .ignore_then(ty.clone().separated_by(just(Token::Comma)))
            .then_ignore(just(Token::RParen))
            .then(just(Token::Arrow).ignore_then(ty.clone()).or_not())
            .map(|(params, ret)| match ret {
                Some(ret_ty) => TypeExpr::Func(params, Box::new(ret_ty)),
                None => TypeExpr::Tuple(params),
            });

        choice((dynamic_ty, named, struct_ty, paren_ty))
            .then(
                just(Token::LBracket)
                    .ignore_then(ty.separated_by(just(Token::Comma)).at_least(1))
                    .then_ignore(just(Token::RBracket))
                    .or_not(),
            )
            .then(just(Token::Question).or_not())
            .map(|((base, generic_args), nullable)| {
                let resolved = match (&base, generic_args) {
                    (TypeExpr::Named(n), Some(args)) if n == "list" && args.len() == 1 => {
                        TypeExpr::List(Box::new(
                            args.into_iter()
                                .next()
                                .expect("list[T] parser branch guarantees one type argument"),
                        ))
                    }
                    (TypeExpr::Named(n), Some(mut args)) if n == "map" && args.len() == 2 => {
                        let v = args
                            .pop()
                            .expect("map[K, V] parser branch guarantees value type");
                        let k = args
                            .pop()
                            .expect("map[K, V] parser branch guarantees key type");
                        TypeExpr::Map(Box::new(k), Box::new(v))
                    }
                    (TypeExpr::Named(n), Some(args)) if n == "set" && args.len() == 1 => {
                        TypeExpr::Set(Box::new(
                            args.into_iter()
                                .next()
                                .expect("set[T] parser branch guarantees one type argument"),
                        ))
                    }
                    (TypeExpr::Named(n), Some(args)) => TypeExpr::Generic(n.clone(), args),
                    _ => base,
                };
                if nullable.is_some() {
                    TypeExpr::Nullable(Box::new(resolved))
                } else {
                    resolved
                }
            })
    })
    .boxed()
}

// ---------------------------------------------------------------------------
// Expression parser
// ---------------------------------------------------------------------------

fn expr_parser() -> P<Expr> {
    recursive(|expr: Recursive<Token, Expr, Simple<Token>>| {
        // ── Inner block parser for trailing-block calls ──────────
        //
        // Blocks inside expressions (lambda bodies, trailing blocks on
        // method calls) need to contain full statements, which in turn
        // contain expressions. To avoid construction-time mutual
        // recursion between `expr_parser` and `stmt_parser`, we build
        // the statement parser here with our own `expr` handle.
        let inner_stmt = stmt_parser_with(expr.clone().boxed());
        let inner_block = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(inner_stmt.separated_by(sep()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .boxed();

        // ── Literals ─────────────────────────────────────────────
        let int_lit = select! { Token::Integer(s) => s }.try_map(|s, span| {
            s.parse::<i64>()
                .map(Expr::Integer)
                .map_err(|_| Simple::custom(span, format!("integer literal `{s}` overflows i64")))
        });
        let float_lit = select! { Token::Float(s) => Expr::Float(s.parse::<f64>().expect("lexer regex guarantees valid f64 literal")) };
        let str_expr = string_lit().map(|s| Expr::StringLit(parse_interpolation(&s)));
        let bool_lit = just(Token::True)
            .to(Expr::Bool(true))
            .or(just(Token::False).to(Expr::Bool(false)));
        let none_lit = just(Token::None_).to(Expr::None_);
        // ── Lambda ───────────────────────────────────────────────
        let lambda_body = expr
            .clone()
            .map(|e| LambdaBody::Expr(Box::new(e)))
            .or(inner_block.clone().map(LambdaBody::Block))
            .boxed();

        let lambda_single = ident()
            .then_ignore(just(Token::FatArrow))
            .then(lambda_body.clone())
            .map(|(name, body)| Expr::Lambda {
                params: vec![LambdaParam { name, ty: None }],
                body,
            })
            .boxed();

        let lambda_multi = just(Token::LParen)
            .ignore_then(
                ident()
                    .map(|name| LambdaParam { name, ty: None })
                    .separated_by(just(Token::Comma))
                    .allow_trailing(),
            )
            .then_ignore(just(Token::RParen))
            .then_ignore(just(Token::FatArrow))
            .then(lambda_body)
            .map(|(params, body)| Expr::Lambda { params, body })
            .boxed();

        // ── Identifier / self ────────────────────────────────────
        let ident_expr = ident().map(Expr::Ident);

        let self_access = just(Token::SelfKw)
            .ignore_then(just(Token::Dot))
            .ignore_then(ident())
            .map(Expr::SelfAccess);

        let self_ref = just(Token::SelfKw).map(|_| Expr::SelfRef);

        // ── Set literal: `set[1, 2, 3]` ──────────────────────────
        let set_lit = just(Token::Set)
            .ignore_then(just(Token::LBracket))
            .ignore_then(newlines())
            .ignore_then(expr.clone().separated_by(field_sep()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBracket))
            .map(Expr::SetLit);

        // ── List ─────────────────────────────────────────────────
        let list_lit = just(Token::LBracket)
            .ignore_then(newlines())
            .ignore_then(expr.clone().separated_by(field_sep()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBracket))
            .map(Expr::ListLit);

        // ── Struct spread-update: `{ ...base, field: val, ... }` ──
        // Must be tried before struct_lit (both open with `{`).
        // Exactly one spread at the front; zero or more overrides follow.
        let struct_spread_update = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(just(Token::DotDotDot))
            .ignore_then(newlines())
            .ignore_then(expr.clone())
            .then(
                field_sep()
                    .ignore_then(
                        map_key()
                            .then_ignore(just(Token::Colon))
                            .then_ignore(newlines())
                            .then(expr.clone())
                            .separated_by(field_sep())
                            .allow_trailing(),
                    )
                    .or_not()
                    .map(|o| o.unwrap_or_default()),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map(|(base, overrides)| Expr::StructSpreadUpdate {
                base: Box::new(base),
                overrides,
            });

        // ── Struct / map literal: `{key: expr, ...}` ────────────
        // Keys may be identifiers, contextual keywords, string literals,
        // integer literals, or boolean literals.  The AST stores all as
        // StructLit; the type checker resolves struct vs. map.
        let struct_lit = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(
                map_lit_key()
                    .then_ignore(just(Token::Colon))
                    .then_ignore(newlines())
                    .then(expr.clone())
                    .separated_by(field_sep())
                    .at_least(1)
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map(Expr::StructLit);

        // ── Tuple / parenthesised ────────────────────────────────
        // Tuple requires 2+ elements; single paren is grouping.
        let tuple_or_paren = just(Token::LParen)
            .ignore_then(newlines())
            .ignore_then(expr.clone())
            .then(
                just(Token::Comma)
                    .ignore_then(newlines())
                    .ignore_then(expr.clone())
                    .repeated(),
            )
            .then_ignore(just(Token::Comma).or_not())
            .then_ignore(newlines())
            .then_ignore(just(Token::RParen))
            .map(|(first, rest)| {
                if rest.is_empty() {
                    first
                } else {
                    let mut items = vec![first];
                    items.extend(rest);
                    Expr::TupleLit(items)
                }
            });

        // ── Rich enum variant construction ───────────────────────
        // `Action.reply { to: "x", tone: "friendly" }` — unambiguous
        // thanks to the braced field list. Must be tried before
        // ident_expr / struct_lit in `primary` so the `IDENT . IDENT {`
        // shape is recognised as a whole. If the pattern doesn't fully
        // match, chumsky backtracks and the shorter forms take over.
        let rich_enum_variant = ident()
            .then_ignore(just(Token::Dot))
            .then(field_name())
            .then_ignore(just(Token::LBrace))
            .then_ignore(newlines())
            .then(
                map_key()
                    .then_ignore(just(Token::Colon))
                    .then_ignore(newlines())
                    .then(expr.clone())
                    .separated_by(field_sep())
                    .at_least(1)
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map(|((ty, variant), fields)| Expr::EnumVariant {
                ty,
                variant,
                fields,
            })
            .boxed();

        // ── If-expression (usable on any RHS) ────────────────────
        // Recursive so that `else if` chains work: the else-branch is either
        // another if-expression (wrapped as a single spanned Stmt::Expr) or a { block }.
        let if_expr = recursive(|if_expr: Recursive<Token, Expr, Simple<Token>>| {
            just(Token::If)
                .ignore_then(expr.clone())
                .then(inner_block.clone())
                .then(
                    just(Token::Else)
                        .ignore_then(
                            if_expr
                                .map_with_span(|e, span| vec![(Stmt::Expr(e), span)])
                                .or(inner_block.clone()),
                        )
                        .or_not(),
                )
                .map(|((cond, then_body), else_body)| Expr::IfExpr {
                    cond: Box::new(cond),
                    then_body,
                    else_body: else_body.unwrap_or_default(),
                })
        })
        .boxed();

        let we_pattern = just(Token::Ident("_".to_string()))
            .to(Pattern::Wildcard)
            .or(ident()
                .then(
                    just(Token::LBrace)
                        .ignore_then(
                            ident()
                                .or(just(Token::Ident("_".to_string())).to("_".to_string()))
                                .separated_by(just(Token::Comma))
                                .allow_trailing(),
                        )
                        .then_ignore(just(Token::RBrace))
                        .or_not(),
                )
                .map(|(name, bindings)| match bindings {
                    Some(b) => Pattern::Variant { name, bindings: b },
                    None => Pattern::Ident(name),
                }))
            .or(plain_string()
                .map(|s| Pattern::Literal(Expr::StringLit(vec![StringPart::Literal(s)]))))
            .or(integer_lit().map(|n| Pattern::Literal(Expr::Integer(n))))
            .boxed();

        let we_arm_body = inner_block
            .clone()
            .or(expr.clone().map(|e| vec![(Stmt::Expr(e), 0..0)]))
            .boxed();

        let we_arm = we_pattern
            .separated_by(just(Token::Comma))
            .at_least(1)
            .then(just(Token::Where).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::FatArrow))
            .then(we_arm_body)
            .map(|((patterns, guard), body)| WhenArm { patterns, guard, body })
            .boxed();

        let when_expr = just(Token::When)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::LBrace))
            .then_ignore(newlines())
            .then(we_arm.separated_by(newlines()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map(|(subject, arms)| Expr::WhenExpr {
                subject: Box::new(subject),
                arms,
            })
            .boxed();

        // ── Primary ──────────────────────────────────────────────
        let primary = choice((
            if_expr,
            when_expr,
            rich_enum_variant,
            self_access,
            self_ref,
            set_lit,
            float_lit,
            int_lit,
            bool_lit,
            none_lit,
            str_expr,
            list_lit,
            struct_spread_update,
            struct_lit,
            lambda_single,
            lambda_multi,
            tuple_or_paren,
            ident_expr,
        ))
        .boxed();

        // ── Call args (named or positional) ──────────────────────
        // Argument labels accept any identifier OR a soft set of
        // contextual keywords (`as`, `from`, `in`, `where`) — these
        // read naturally as named args even though they're reserved
        // elsewhere (`Ai.classify(x, as: T)`, `Email.fetch(from: box)`).
        let arg_label = select! {
            Token::Ident(s) => s,
            Token::As => "as".to_string(),
            Token::From => "from".to_string(),
            Token::In => "in".to_string(),
            Token::Where => "where".to_string(),
        };

        let call_arg_parser = arg_label
            .then_ignore(just(Token::Colon))
            .then(expr.clone())
            .map(|(name, value)| CallArg {
                name: Some(name),
                value,
                spread: false,
            })
            .or(just(Token::DotDotDot)
                .ignore_then(expr.clone())
                .map(|value| CallArg {
                    name: None,
                    value,
                    spread: true,
                }))
            .or(expr.clone().map(|value| CallArg {
                name: None,
                value,
                spread: false,
            }))
            .boxed();

        let call_args = just(Token::LParen)
            .ignore_then(newlines())
            .ignore_then(call_arg_parser.separated_by(field_sep()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RParen))
            .boxed();

        // ── Postfix operations ───────────────────────────────────
        //
        // Trailing-closure syntax (`f(args) { body }`) was considered but
        // dropped for v0.1: it creates an unresolvable ambiguity with
        // control-flow body parsing (`if f(x) { ... }` — is `{...}` a
        // trailing closure on `f(x)` or the `if` body?). Use explicit
        // lambda syntax `() => { ... }` to pass a block to a function.
        let subscript = just(Token::LBracket)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::RBracket))
            .map(PostfixOp::Index)
            .boxed();

        let postfix_op = choice((
            just(Token::Dot)
                .ignore_then(field_name())
                .then(call_args.clone().or_not())
                .map(|(field, args)| PostfixOp::DotAccess { field, args }),
            just(Token::NullDot)
                .ignore_then(field_name())
                .map(PostfixOp::NullDotAccess),
            just(Token::Bang).to(PostfixOp::NullAssert),
            call_args.clone().map(PostfixOp::Call),
            just(Token::As)
                .ignore_then(type_expr())
                .map(PostfixOp::Cast),
            subscript,
        ))
        .boxed();

        let postfix = primary
            .then(postfix_op.repeated())
            .foldl(|expr, op| match op {
                PostfixOp::DotAccess {
                    field,
                    args: Some(args),
                } => Expr::MethodCall {
                    object: Box::new(expr),
                    method: field,
                    args,
                },
                PostfixOp::DotAccess { field, args: None } => {
                    // Duration sugar: `5.minutes` after an Integer primary.
                    if let Expr::Integer(n) = &expr
                        && let Some(unit) = parse_duration_unit(&field)
                    {
                        return Expr::Duration {
                            value: Box::new(Expr::Integer(*n)),
                            unit,
                        };
                    }
                    // `Urgency.medium` and `Http.ok` both emit FieldAccess;
                    // the type checker resolves enum variants vs. namespace
                    // members based on the identifier's bound type.
                    Expr::FieldAccess(Box::new(expr), field)
                }
                PostfixOp::NullDotAccess(field) => Expr::NullFieldAccess(Box::new(expr), field),
                PostfixOp::NullAssert => Expr::NullAssert(Box::new(expr)),
                PostfixOp::Call(args) => Expr::Call {
                    callee: Box::new(expr),
                    args,
                },
                PostfixOp::Cast(ty) => Expr::Cast {
                    expr: Box::new(expr),
                    ty,
                },
                PostfixOp::Index(idx) => Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(idx),
                },
            })
            .boxed();

        // ── Unary ────────────────────────────────────────────────
        let unary = just(Token::Not)
            .to(UnOp::Not)
            .or(just(Token::Minus).to(UnOp::Neg))
            .repeated()
            .then(postfix)
            .foldr(|op, expr| Expr::UnaryOp {
                op,
                expr: Box::new(expr),
            })
            .boxed();

        // ── * / % ────────────────────────────────────────────────
        let product = unary
            .clone()
            .then(
                just(Token::Star)
                    .to(BinOp::Mul)
                    .or(just(Token::Slash).to(BinOp::Div))
                    .or(just(Token::Percent).to(BinOp::Mod))
                    .then(unary)
                    .repeated(),
            )
            .foldl(|l, (op, r)| Expr::BinaryOp {
                left: Box::new(l),
                op,
                right: Box::new(r),
            })
            .boxed();

        // ── + - ──────────────────────────────────────────────────
        let sum = product
            .clone()
            .then(
                just(Token::Plus)
                    .to(BinOp::Add)
                    .or(just(Token::Minus).to(BinOp::Sub))
                    .then(product)
                    .repeated(),
            )
            .foldl(|l, (op, r)| Expr::BinaryOp {
                left: Box::new(l),
                op,
                right: Box::new(r),
            })
            .boxed();

        // ── .. ───────────────────────────────────────────────────
        // Range binds tighter than comparison: `a < 1..5` → `a < (1..5)`.
        let range = sum
            .clone()
            .then(just(Token::DotDot).ignore_then(sum.clone()).or_not())
            .map(|(start, end)| match end {
                Some(end) => Expr::Range(Box::new(start), Box::new(end)),
                None => start,
            })
            .boxed();

        // ── == != < > <= >= ──────────────────────────────────────
        let cmp = range
            .clone()
            .then(
                choice((
                    just(Token::EqEq).to(BinOp::Eq),
                    just(Token::Neq).to(BinOp::Neq),
                    just(Token::Lte).to(BinOp::Lte),
                    just(Token::Gte).to(BinOp::Gte),
                    just(Token::Lt).to(BinOp::Lt),
                    just(Token::Gt).to(BinOp::Gt),
                ))
                .then(range)
                .repeated(),
            )
            .foldl(|l, (op, r)| Expr::BinaryOp {
                left: Box::new(l),
                op,
                right: Box::new(r),
            })
            .boxed();

        // ── and ──────────────────────────────────────────────────
        let land = cmp
            .clone()
            .then(just(Token::And).to(BinOp::And).then(cmp).repeated())
            .foldl(|l, (op, r)| Expr::BinaryOp {
                left: Box::new(l),
                op,
                right: Box::new(r),
            })
            .boxed();

        // ── or ───────────────────────────────────────────────────
        let lor = land
            .clone()
            .then(just(Token::Or).to(BinOp::Or).then(land).repeated())
            .foldl(|l, (op, r)| Expr::BinaryOp {
                left: Box::new(l),
                op,
                right: Box::new(r),
            })
            .boxed();

        // ── |> ───────────────────────────────────────────────────
        // Pipeline has lower precedence than `??` — SPEC §18.
        let pipeline = lor
            .clone()
            .then(just(Token::Pipe).ignore_then(lor).repeated())
            .foldl(|l, r| Expr::Pipeline(Box::new(l), Box::new(r)))
            .boxed();

        // ── ?? ───────────────────────────────────────────────────
        // Null-coalesce is the top of the expression chain.
        pipeline
            .clone()
            .then(just(Token::NullCoalesce).ignore_then(pipeline).repeated())
            .foldl(|l, r| Expr::NullCoalesce(Box::new(l), Box::new(r)))
    })
    .boxed()
}

#[derive(Debug, Clone)]
enum PostfixOp {
    DotAccess {
        field: String,
        args: Option<Vec<CallArg>>,
    },
    NullDotAccess(String),
    NullAssert,
    Call(Vec<CallArg>),
    Cast(TypeExpr),
    Index(Expr),
}

fn parse_duration_unit(s: &str) -> Option<DurationUnit> {
    match s {
        "milliseconds" | "millisecond" | "millis" | "ms" => Some(DurationUnit::Milliseconds),
        "seconds" | "second" | "sec" | "s" => Some(DurationUnit::Seconds),
        "minutes" | "minute" | "min" | "m" => Some(DurationUnit::Minutes),
        "hours" | "hour" | "hr" | "h" => Some(DurationUnit::Hours),
        "days" | "day" | "d" => Some(DurationUnit::Days),
        "weeks" | "week" | "w" => Some(DurationUnit::Weeks),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Statement parser
// ---------------------------------------------------------------------------

fn stmt_parser() -> P<Spanned<Stmt>> {
    stmt_parser_with(expr_parser())
}

/// Build a statement parser using a pre-constructed expression parser.
/// Used internally by `expr_parser` to break mutual parser-construction
/// recursion when building trailing-block / lambda-block support.
fn stmt_parser_with(expr: P<Expr>) -> P<Spanned<Stmt>> {
    recursive(|stmt: Recursive<Token, Spanned<Stmt>, Simple<Token>>| {
        let block = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(stmt.clone().separated_by(sep()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .boxed();

        // Matches one augmented-assignment operator and returns its BinOp.
        let aug_op = choice([
            just(Token::PlusEq).to(BinOp::Add),
            just(Token::MinusEq).to(BinOp::Sub),
            just(Token::StarEq).to(BinOp::Mul),
            just(Token::SlashEq).to(BinOp::Div),
            just(Token::PercentEq).to(BinOp::Mod),
        ])
        .boxed();

        // self.field += expr  (desugars to self.field = self.field op expr)
        let aug_self_assign = just(Token::SelfKw)
            .ignore_then(just(Token::Dot))
            .ignore_then(ident())
            .then(aug_op.clone())
            .then(expr.clone())
            .map(|((field, op), rhs)| Stmt::SelfAssign {
                field: field.clone(),
                value: Expr::BinaryOp {
                    left: Box::new(Expr::SelfAccess(field)),
                    op,
                    right: Box::new(rhs),
                },
            })
            .boxed();

        // self.field = expr
        let self_assign = just(Token::SelfKw)
            .ignore_then(just(Token::Dot))
            .ignore_then(ident())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|(field, value)| Stmt::SelfAssign { field, value })
            .boxed();

        // x += expr, x -= expr, etc. — produces Stmt::AugAssign so the
        // interpreter can use env.set (mutation) rather than env.define
        // (shadow), which makes accumulation in for loops work correctly.
        let aug_let_stmt = ident()
            .then(aug_op)
            .then(expr.clone())
            .map(|((name, op), rhs)| Stmt::AugAssign { name, op, rhs })
            .boxed();

        // x = expr  or  x: Type = expr
        let let_stmt = ident()
            .then(just(Token::Colon).ignore_then(type_expr()).or_not())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|((name, ty), value)| Stmt::Let {
                binding: Binding::Ident(name),
                ty,
                value,
            })
            .boxed();

        // {a, b} = expr  or  {a: x} = expr  (struct destructure)
        let destruct_struct_let = struct_destruct_pat()
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|(fields, value)| Stmt::Let {
                binding: Binding::Destruct(DestructPat::Struct(fields)),
                ty: None,
                value,
            })
            .boxed();

        // (a, b) = expr  (tuple destructure)
        let destruct_tuple_let = tuple_destruct_pat()
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|(names, value)| Stmt::Let {
                binding: Binding::Destruct(DestructPat::Tuple(names)),
                ty: None,
                value,
            })
            .boxed();

        let return_stmt = just(Token::Return)
            .ignore_then(expr.clone().or_not())
            .map(Stmt::Return)
            .boxed();

        let raise_stmt = just(Token::Raise)
            .ignore_then(expr.clone())
            .map(Stmt::Raise)
            .boxed();

        let break_stmt = just(Token::Break).to(Stmt::Break).boxed();
        let continue_stmt = just(Token::Continue).to(Stmt::Continue).boxed();

        let for_stmt = just(Token::For)
            .ignore_then(ident())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(just(Token::If).ignore_then(expr.clone()).or_not())
            .then(block.clone())
            .map(|(((binding, iter), filter), body)| Stmt::For {
                binding: Binding::Ident(binding),
                iter,
                filter,
                body,
            })
            .boxed();

        // for {a, b} in expr [if pred] { ... }
        let destruct_for_stmt = just(Token::For)
            .ignore_then(struct_destruct_pat())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(just(Token::If).ignore_then(expr.clone()).or_not())
            .then(block.clone())
            .map(|(((fields, iter), filter), body)| Stmt::For {
                binding: Binding::Destruct(DestructPat::Struct(fields)),
                iter,
                filter,
                body,
            })
            .boxed();

        // for (a, b) in expr [if pred] { ... }
        let tuple_destruct_for_stmt = just(Token::For)
            .ignore_then(tuple_destruct_pat())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(just(Token::If).ignore_then(expr.clone()).or_not())
            .then(block.clone())
            .map(|(((names, iter), filter), body)| Stmt::For {
                binding: Binding::Destruct(DestructPat::Tuple(names)),
                iter,
                filter,
                body,
            })
            .boxed();

        let while_stmt = just(Token::While)
            .ignore_then(expr.clone())
            .then(block.clone())
            .map(|(cond, body)| Stmt::While { cond, body })
            .boxed();

        let if_stmt = just(Token::If)
            .ignore_then(expr.clone())
            .then(block.clone())
            .then(just(Token::Else).ignore_then(block.clone()).or_not())
            .then(just(Token::NullCoalesce).ignore_then(expr.clone()).or_not())
            .map(|(((cond, then_body), else_body), null_coalesce)| {
                if let Some(default) = null_coalesce {
                    // `if { } else { } ?? default` → expression statement.
                    Stmt::Expr(Expr::NullCoalesce(
                        Box::new(Expr::IfExpr {
                            cond: Box::new(cond),
                            then_body,
                            else_body: else_body.unwrap_or_default(),
                        }),
                        Box::new(default),
                    ))
                } else {
                    Stmt::If {
                        cond,
                        then_body,
                        else_body,
                    }
                }
            })
            .boxed();

        // when arm pattern
        let pattern = just(Token::Ident("_".to_string()))
            .to(Pattern::Wildcard)
            .or(ident()
                .then(
                    just(Token::LBrace)
                        .ignore_then(
                            ident()
                                .or(just(Token::Ident("_".to_string())).to("_".to_string()))
                                .separated_by(just(Token::Comma))
                                .allow_trailing(),
                        )
                        .then_ignore(just(Token::RBrace))
                        .or_not(),
                )
                .map(|(name, bindings)| match bindings {
                    Some(b) => Pattern::Variant { name, bindings: b },
                    None => Pattern::Ident(name),
                }))
            .or(plain_string()
                .map(|s| Pattern::Literal(Expr::StringLit(vec![StringPart::Literal(s)]))))
            .or(integer_lit().map(|n| Pattern::Literal(Expr::Integer(n))))
            .boxed();

        let when_arm_body = block
            .clone()
            .or(expr.clone().map(|e| vec![(Stmt::Expr(e), 0..0)]))
            .boxed();

        let when_arm = pattern
            .separated_by(just(Token::Comma))
            .at_least(1)
            .then(just(Token::Where).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::FatArrow))
            .then(when_arm_body)
            .map(|((patterns, guard), body)| WhenArm {
                patterns,
                guard,
                body,
            })
            .boxed();

        let when_stmt = just(Token::When)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::LBrace))
            .then_ignore(newlines())
            .then(when_arm.separated_by(newlines()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map(|(subject, arms)| Stmt::When { subject, arms })
            .boxed();

        let catch_clause = just(Token::Catch)
            .ignore_then(ident())
            .then_ignore(just(Token::Colon))
            .then(type_expr())
            .then(block.clone())
            .map(|((name, ty), body)| CatchClause { name, ty, body })
            .boxed();

        let try_catch = just(Token::Try)
            .ignore_then(block)
            .then(catch_clause.repeated().at_least(1))
            .map(|(body, catches)| Stmt::TryCatch { body, catches })
            .boxed();

        let expr_stmt = expr.map(Stmt::Expr).boxed();

        choice((
            aug_self_assign,
            self_assign,
            destruct_struct_let,
            destruct_tuple_let,
            aug_let_stmt,
            let_stmt,
            return_stmt,
            raise_stmt,
            break_stmt,
            continue_stmt,
            destruct_for_stmt,
            tuple_destruct_for_stmt,
            for_stmt,
            while_stmt,
            if_stmt,
            when_stmt,
            try_catch,
            expr_stmt,
        ))
        .map_with_span(|stmt, span| (stmt, span))
    })
    .boxed()
}

fn block_toplevel() -> P<Block> {
    just(Token::LBrace)
        .ignore_then(newlines())
        .ignore_then(stmt_parser().separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .boxed()
}

// ---------------------------------------------------------------------------
// Top-level declarations
// ---------------------------------------------------------------------------

fn type_decl() -> P<Decl> {
    let field_def = field_name()
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|(name, ty)| Field { name, ty });

    let rich_variant = ident()
        .then(
            just(Token::LBrace)
                .ignore_then(newlines())
                .ignore_then(field_def.clone().separated_by(field_sep()).allow_trailing())
                .then_ignore(newlines())
                .then_ignore(just(Token::RBrace))
                .or_not(),
        )
        .map(|(name, fields)| EnumVariant { name, fields });

    let rich_enum = just(Token::Bar)
        .ignore_then(rich_variant)
        .then(
            newlines()
                .ignore_then(just(Token::Bar))
                .ignore_then(
                    ident()
                        .then(
                            just(Token::LBrace)
                                .ignore_then(newlines())
                                .ignore_then(
                                    field_name()
                                        .then_ignore(just(Token::Colon))
                                        .then(type_expr())
                                        .map(|(n, t)| Field { name: n, ty: t })
                                        .separated_by(field_sep())
                                        .allow_trailing(),
                                )
                                .then_ignore(newlines())
                                .then_ignore(just(Token::RBrace))
                                .or_not(),
                        )
                        .map(|(name, fields)| EnumVariant { name, fields }),
                )
                .repeated(),
        )
        .map(|(first, rest)| {
            let mut variants = vec![first];
            variants.extend(rest);
            TypeDef::RichEnum(variants)
        });

    let simple_enum = ident()
        .then(just(Token::Bar).ignore_then(ident()).repeated())
        .map(|(first, rest)| {
            let mut names = vec![first];
            names.extend(rest);
            names
        })
        .try_map(|names, span| {
            if names.len() < 2 {
                Err(Simple::custom(span, "enum needs at least two variants"))
            } else {
                Ok(TypeDef::SimpleEnum(names))
            }
        });

    let struct_def = just(Token::LBrace)
        .ignore_then(newlines())
        .ignore_then(
            field_name()
                .then_ignore(just(Token::Colon))
                .then(type_expr())
                .map(|(n, t)| Field { name: n, ty: t })
                .separated_by(field_sep())
                .allow_trailing(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(TypeDef::Struct);

    let alias = type_expr().map(TypeDef::Alias);

    let after_eq = choice((rich_enum, simple_enum, alias));

    let type_params = just(Token::LBracket)
        .ignore_then(ident().separated_by(just(Token::Comma)).at_least(1))
        .then_ignore(just(Token::RBracket))
        .or_not()
        .map(|p| p.unwrap_or_default());

    just(Token::Type)
        .ignore_then(ident())
        .then(type_params)
        .then(just(Token::Eq).ignore_then(after_eq).or(struct_def))
        .map(|((name, type_params), def)| {
            Decl::Type(TypeDecl {
                name,
                type_params,
                def,
            })
        })
        .boxed()
}

fn interface_decl() -> P<Decl> {
    let self_param = just(Token::SelfKw).to(Param {
        name: Binding::Ident("self".to_string()),
        ty: TypeExpr::Named("__impl_self__".to_string()),
        default: None,
        variadic: false,
    });
    let typed_param = ident()
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|(name, ty)| Param {
            name: Binding::Ident(name),
            ty,
            default: None,
            variadic: false,
        });
    let any_param = choice((self_param, typed_param)).boxed();

    let task_sig = just(Token::Task)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(any_param.separated_by(field_sep()).allow_trailing())
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(type_expr()).or_not())
        .map(|((name, params), return_type)| TaskSig {
            name,
            params,
            return_type,
        });

    just(Token::Interface)
        .ignore_then(ident())
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(task_sig.separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(|(name, methods)| Decl::Interface(InterfaceDecl { name, methods }))
        .boxed()
}

fn extern_decl() -> P<Decl> {
    let param = ident()
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|(name, ty)| Param {
            name: Binding::Ident(name),
            ty,
            default: None,
            variadic: false,
        });

    just(Token::Extern)
        .ignore_then(just(Token::Task))
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(param.separated_by(field_sep()).allow_trailing())
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then_ignore(just(Token::Arrow))
        .then(type_expr())
        .then_ignore(just(Token::From))
        .then(plain_string())
        .map(|(((name, params), return_type), source)| {
            Decl::Extern(ExternDecl {
                name,
                params,
                return_type,
                source,
            })
        })
        .boxed()
}

fn use_decl() -> P<Decl> {
    let file = plain_string().map(UseKind::File);

    let symbol = ident()
        .then_ignore(just(Token::From))
        .then(plain_string())
        .map(|(name, source)| UseKind::Symbol { name, source });

    let package = ident()
        .then(
            just(Token::Slash)
                .ignore_then(ident())
                .repeated()
                .at_least(1),
        )
        .map(|(first, rest)| {
            let mut segments = vec![first];
            segments.extend(rest);
            UseKind::Package(segments)
        });

    just(Token::Use)
        .ignore_then(choice((symbol, package, file)))
        .map(|kind| Decl::Use(UseDecl { kind }))
        .boxed()
}

fn task_decl() -> P<TaskDecl> {
    let param_name = choice((
        struct_destruct_pat().map(|fields| Binding::Destruct(DestructPat::Struct(fields))),
        ident().map(Binding::Ident),
    ));
    // Ordinary param: `name: Type` with an optional `= default`.
    let regular_param = param_name
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .then(just(Token::Eq).ignore_then(expr_parser()).or_not())
        .map(|((name, ty), default)| Param {
            name,
            ty,
            default,
            variadic: false,
        });
    // Variadic param: `...name: Type` — no default allowed (defaults to []).
    let variadic_param = just(Token::DotDotDot)
        .ignore_then(ident())
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|(name, ty)| Param {
            name: Binding::Ident(name),
            ty,
            default: None,
            variadic: true,
        });
    // Each slot is either a variadic or a regular param.
    let any_param = choice((variadic_param, regular_param)).boxed();
    let param_list = just(Token::LParen)
        .ignore_then(newlines())
        .ignore_then(any_param.separated_by(field_sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RParen))
        .try_map(|params: Vec<Param>, span| {
            // Enforce: at most one variadic, and it must be the last parameter.
            let bad = params
                .iter()
                .enumerate()
                .find(|(i, p)| p.variadic && *i + 1 < params.len());
            if let Some((_, p)) = bad {
                let name = match &p.name {
                    Binding::Ident(s) => s.clone(),
                    _ => "?".into(),
                };
                Err(Simple::custom(
                    span,
                    format!("variadic parameter `...{name}` must be the last parameter"),
                ))
            } else {
                Ok(params)
            }
        });

    let type_params = just(Token::LBracket)
        .ignore_then(ident().separated_by(just(Token::Comma)).at_least(1))
        .then_ignore(just(Token::RBracket))
        .or_not()
        .map(|p| p.unwrap_or_default());

    just(Token::Task)
        .ignore_then(ident())
        .then(type_params)
        .then(param_list)
        .then(just(Token::Arrow).ignore_then(type_expr()).or_not())
        .then(block_toplevel())
        .map(
            |((((name, type_params), params), return_type), body)| TaskDecl {
                name,
                type_params,
                params,
                return_type,
                body,
            },
        )
        .boxed()
}

fn agent_item() -> P<AgentItem> {
    // `@name ...` — block-body attributes get a block, others get an expr.
    let block_attr = just(Token::AtSign)
        .ignore_then(ident().try_map(|name, span| {
            if BLOCK_BODY_ATTRIBUTES.contains(&name.as_str()) {
                Ok(name)
            } else {
                Err(Simple::custom(
                    span,
                    format!("'{}' is not a block attribute", name),
                ))
            }
        }))
        .then(block_toplevel())
        .map(|(name, body)| {
            AgentItem::Attribute(AttributeDecl {
                name,
                body: AttributeBody::Block(body),
            })
        })
        .boxed();

    // `@tools [Ns | Ns.method | Ns if expr | Ns.method if expr, ...]`
    let tool_entry = ident()
        .then(just(Token::Dot).ignore_then(ident()).or_not())
        .then(just(Token::If).ignore_then(expr_parser()).or_not())
        .map(|((namespace, method), condition)| ToolEntry {
            namespace,
            method,
            condition,
        });

    let tools_attr = just(Token::AtSign)
        .ignore_then(just(Token::Ident("tools".to_string())))
        .ignore_then(just(Token::LBracket))
        .ignore_then(newlines())
        .ignore_then(tool_entry.separated_by(field_sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBracket))
        .map(|entries| {
            AgentItem::Attribute(AttributeDecl {
                name: "tools".to_string(),
                body: AttributeBody::Tools(entries),
            })
        })
        .boxed();

    let expr_attr = just(Token::AtSign)
        .ignore_then(ident())
        .then(expr_parser())
        .map(|(name, body)| {
            AgentItem::Attribute(AttributeDecl {
                name,
                body: AttributeBody::Expr(body),
            })
        })
        .boxed();

    let state = just(Token::State)
        .ignore_then(just(Token::LBrace))
        .ignore_then(newlines())
        .ignore_then(
            ident()
                .then_ignore(just(Token::Colon))
                .then(
                    just(Token::Ident("readonly".to_string()))
                        .or_not()
                        .map(|opt| opt.is_some()),
                )
                .then(type_expr())
                .then_ignore(just(Token::Eq))
                .then(expr_parser())
                .map(|(((name, readonly), ty), default)| StateField {
                    name,
                    ty,
                    default,
                    readonly,
                })
                .separated_by(sep())
                .allow_trailing(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(AgentItem::State)
        .boxed();

    let task = task_decl().map(AgentItem::Task).boxed();

    let on_param_name = choice((
        struct_destruct_pat().map(|fields| Binding::Destruct(DestructPat::Struct(fields))),
        ident().map(Binding::Ident),
    ));
    let on_handler = just(Token::On)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(
                    on_param_name
                        .then_ignore(just(Token::Colon))
                        .then(type_expr())
                        .map(|(name, ty)| Param {
                            name,
                            ty,
                            default: None,
                            variadic: false,
                        }),
                )
                .then_ignore(just(Token::RParen))
                .or_not(),
        )
        .then(block_toplevel())
        .map(|((event, param), body)| AgentItem::On(OnHandler { event, param, body }))
        .boxed();

    choice((block_attr, tools_attr, expr_attr, state, task, on_handler)).boxed()
}

fn agent_decl() -> P<Decl> {
    just(Token::Agent)
        .ignore_then(ident())
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(agent_item().separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(|(name, items)| Decl::Agent(AgentDecl { name, items }))
        .boxed()
}

fn impl_decl() -> P<Decl> {
    // `self` as receiver param — type is filled in at registration time
    let self_param = just(Token::SelfKw).to(Param {
        name: Binding::Ident("self".to_string()),
        ty: TypeExpr::Named("__impl_self__".to_string()),
        default: None,
        variadic: false,
    });

    let typed_param = ident()
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|(name, ty)| Param {
            name: Binding::Ident(name),
            ty,
            default: None,
            variadic: false,
        });

    let any_param = choice((self_param, typed_param)).boxed();

    let impl_task = just(Token::Task)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(any_param.separated_by(field_sep()).allow_trailing())
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(type_expr()).or_not())
        .then(block_toplevel())
        .map(|(((name, params), return_type), body)| TaskDecl {
            name,
            type_params: vec![],
            params,
            return_type,
            body,
        })
        .boxed();

    just(Token::Impl)
        .ignore_then(ident())
        .then_ignore(just(Token::For))
        .then(ident())
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(impl_task.separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(|((interface_name, type_name), methods)| {
            Decl::Impl(ImplDecl {
                interface_name,
                type_name,
                methods,
            })
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

fn program_parser() -> P<Program> {
    let stmt_decl = stmt_parser().map(Decl::Stmt);

    let decl = choice((
        type_decl(),
        interface_decl(),
        impl_decl(),
        extern_decl(),
        task_decl().map(Decl::Task),
        agent_decl(),
        use_decl(),
        stmt_decl,
    ))
    .boxed();

    newlines()
        .ignore_then(decl.separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(end())
        .map(|declarations| Program {
            declarations: declarations.into_iter().map(|d| (d, 0..0)).collect(),
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// String interpolation
// ---------------------------------------------------------------------------

fn parse_interpolation(raw: &str) -> Vec<StringPart> {
    let mut parts = Vec::new();
    let mut current = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                match next {
                    'n' => {
                        chars.next();
                        current.push('\n');
                    }
                    't' => {
                        chars.next();
                        current.push('\t');
                    }
                    'r' => {
                        chars.next();
                        current.push('\r');
                    }
                    '\\' => {
                        chars.next();
                        current.push('\\');
                    }
                    '"' => {
                        chars.next();
                        current.push('"');
                    }
                    '{' => {
                        chars.next();
                        current.push('{');
                    }
                    '}' => {
                        chars.next();
                        current.push('}');
                    }
                    _ => {
                        current.push('\\');
                        current.push(next);
                        chars.next();
                    }
                }
            }
        } else if ch == '{' {
            if !current.is_empty() {
                parts.push(StringPart::Literal(std::mem::take(&mut current)));
            }
            let mut depth = 1;
            let mut expr_text = String::new();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    // Preserve escape sequences inside the slot so the
                    // nested expression lexer can resolve them.
                    expr_text.push(c);
                    if let Some(&n) = chars.peek() {
                        chars.next();
                        expr_text.push(n);
                    }
                } else if c == '"' {
                    // Skip over a nested string literal inside the slot
                    // so its `{...}` and `}` characters don't terminate
                    // this interpolation prematurely.
                    expr_text.push(c);
                    let mut inner_depth = 0;
                    while let Some(nc) = chars.next() {
                        if nc == '\\' {
                            expr_text.push(nc);
                            if let Some(&nn) = chars.peek() {
                                chars.next();
                                expr_text.push(nn);
                            }
                        } else if nc == '{' {
                            inner_depth += 1;
                            expr_text.push(nc);
                        } else if nc == '}' {
                            if inner_depth > 0 {
                                inner_depth -= 1;
                            }
                            expr_text.push(nc);
                        } else if nc == '"' && inner_depth == 0 {
                            expr_text.push(nc);
                            break;
                        } else {
                            expr_text.push(nc);
                        }
                    }
                } else if c == '{' {
                    depth += 1;
                    expr_text.push(c);
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    expr_text.push(c);
                } else {
                    expr_text.push(c);
                }
            }
            let (expr_src, fmt_spec) = split_format_spec(&expr_text);
            parts.push(StringPart::Interpolation(
                Box::new(parse_interp_expr(expr_src)),
                fmt_spec.map(|s| s.to_string()),
            ));
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        parts.push(StringPart::Literal(current));
    }
    if parts.is_empty() {
        parts.push(StringPart::Literal(String::new()));
    }
    parts
}

/// Split `"expr:spec"` into `("expr", Some("spec"))`, respecting brace/bracket/paren depth
/// so colons inside named-argument calls or struct literals are not treated as the
/// format-spec separator.  Returns `(full_text, None)` when no bare colon is found.
fn split_format_spec(text: &str) -> (&str, Option<&str>) {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if in_str {
            if c == '\\' {
                chars.next(); // skip escaped char
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            ':' if depth == 0 => {
                let expr_part = text[..i].trim();
                let spec_part = text[i + 1..].trim();
                if !spec_part.is_empty() {
                    return (expr_part, Some(spec_part));
                }
                // bare trailing colon with no spec — treat as no spec
                return (text, None);
            }
            _ => {}
        }
    }
    (text, None)
}

fn parse_interp_expr(text: &str) -> Expr {
    use logos::Logos;
    let text = text.trim();
    if text.is_empty() {
        return Expr::StringLit(vec![StringPart::Literal(String::new())]);
    }

    // Lex the interpolation content directly via logos, bypassing the
    // NamedSource wrapper used by the public `lex()` entry point.
    let raw: Vec<Spanned<Token>> = Token::lexer(text)
        .spanned()
        .filter_map(|(r, span)| r.ok().map(|t| (t, span)))
        .collect();

    if raw.is_empty() {
        return Expr::Ident(text.to_string());
    }

    let tokens = normalize_newlines(raw);
    let eoi = text.len()..text.len() + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    // On parse failure, fall back to treating the whole slot as an identifier
    // so a bad expression never silently produces a corrupt AST node.
    expr_parser()
        .parse(stream)
        .unwrap_or_else(|_| Expr::Ident(text.to_string()))
}
