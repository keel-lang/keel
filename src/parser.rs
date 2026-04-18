use chumsky::prelude::*;
use chumsky::Stream;
use miette::NamedSource;

use crate::ast::*;
use crate::lexer::{Span, Token};

type P<T> = BoxedParser<'static, Token, T, Simple<Token>>;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

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
    just(Token::Newline).repeated().at_least(1).ignored().boxed()
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
    }
    .boxed()
}

/// Map / struct-literal key: a field_name or a string literal (strings
/// are raw-decoded into the key).
fn map_key() -> P<String> {
    field_name().or(plain_string()).boxed()
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
            Some('n') => { chars.next(); out.push('\n'); }
            Some('t') => { chars.next(); out.push('\t'); }
            Some('r') => { chars.next(); out.push('\r'); }
            Some('\\') => { chars.next(); out.push('\\'); }
            Some('"') => { chars.next(); out.push('"'); }
            Some('{') => { chars.next(); out.push('{'); }
            Some('}') => { chars.next(); out.push('}'); }
            Some(_) | None => out.push('\\'),
        }
    }
    out
}

fn plain_string() -> P<String> {
    string_lit().map(|s| unescape_plain(&s)).boxed()
}

fn integer_lit() -> P<i64> {
    select! { Token::Integer(s) => s.parse::<i64>().unwrap() }.boxed()
}

// ---------------------------------------------------------------------------
// Type expressions
// ---------------------------------------------------------------------------

fn type_expr() -> P<TypeExpr> {
    recursive(|ty: Recursive<Token, TypeExpr, Simple<Token>>| {
        let named = ident().map(TypeExpr::Named);

        let dynamic_ty = just(Token::Ident("dynamic".to_string()))
            .to(TypeExpr::Dynamic);

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

        let tuple_ty = just(Token::LParen)
            .ignore_then(ty.clone().separated_by(just(Token::Comma)).at_least(2))
            .then_ignore(just(Token::RParen))
            .map(TypeExpr::Tuple);

        choice((dynamic_ty, named, struct_ty, tuple_ty))
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
                        TypeExpr::List(Box::new(args.into_iter().next().unwrap()))
                    }
                    (TypeExpr::Named(n), Some(mut args)) if n == "map" && args.len() == 2 => {
                        let v = args.pop().unwrap();
                        let k = args.pop().unwrap();
                        TypeExpr::Map(Box::new(k), Box::new(v))
                    }
                    (TypeExpr::Named(n), Some(args)) if n == "set" && args.len() == 1 => {
                        TypeExpr::Set(Box::new(args.into_iter().next().unwrap()))
                    }
                    (TypeExpr::Named(n), Some(args)) => {
                        TypeExpr::Generic(n.clone(), args)
                    }
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
            .ignore_then(
                inner_stmt.separated_by(sep()).allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .boxed();

        // ── Literals ─────────────────────────────────────────────
        let int_lit = select! { Token::Integer(s) => Expr::Integer(s.parse::<i64>().unwrap()) };
        let float_lit = select! { Token::Float(s) => Expr::Float(s.parse::<f64>().unwrap()) };
        let str_expr = string_lit().map(|s| Expr::StringLit(parse_interpolation(&s)));
        let bool_lit = just(Token::True)
            .to(Expr::Bool(true))
            .or(just(Token::False).to(Expr::Bool(false)));
        let none_lit = just(Token::None_).to(Expr::None_);
        let now_lit = just(Token::Now).to(Expr::Now);

        // ── Lambda ───────────────────────────────────────────────
        let lambda_body = expr.clone().map(|e| LambdaBody::Expr(Box::new(e)))
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

        // ── Set literal: `set[1, 2, 3]` ──────────────────────────
        let set_lit = just(Token::Set)
            .ignore_then(just(Token::LBracket))
            .ignore_then(newlines())
            .ignore_then(
                expr.clone()
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBracket))
            .map(Expr::SetLit);

        // ── List ─────────────────────────────────────────────────
        let list_lit = just(Token::LBracket)
            .ignore_then(newlines())
            .ignore_then(
                expr.clone()
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBracket))
            .map(Expr::ListLit);

        // ── Struct / map literal: `{key: expr, ...}` ────────────
        // Keys may be identifiers, contextual keywords, or string
        // literals (`{"foo": 1}`). The AST stores all as StructLit;
        // the type checker resolves struct vs. map.
        let struct_lit = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(
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

        // ── Primary ──────────────────────────────────────────────
        let primary = choice((
            self_access,
            set_lit,
            float_lit,
            int_lit,
            bool_lit,
            none_lit,
            now_lit,
            str_expr,
            list_lit,
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
            .map(|(name, value)| CallArg { name: Some(name), value })
            .or(expr.clone().map(|value| CallArg { name: None, value }))
            .boxed();

        let call_args = just(Token::LParen)
            .ignore_then(newlines())
            .ignore_then(
                call_arg_parser
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
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
        let postfix_op = choice((
            just(Token::Dot)
                .ignore_then(field_name())
                .then(call_args.clone().or_not())
                .map(|(field, args)| PostfixOp::DotAccess { field, args }),
            just(Token::NullDot)
                .ignore_then(field_name())
                .map(PostfixOp::NullDotAccess),
            just(Token::Bang).to(PostfixOp::NullAssert),
            call_args.clone()
                .map(PostfixOp::Call),
            just(Token::As)
                .ignore_then(type_expr())
                .map(PostfixOp::Cast),
        ))
        .boxed();

        let postfix = primary
            .then(postfix_op.repeated())
            .foldl(|expr, op| match op {
                PostfixOp::DotAccess { field, args: Some(args) } => Expr::MethodCall {
                    object: Box::new(expr),
                    method: field,
                    args,
                },
                PostfixOp::DotAccess { field, args: None } => {
                    // Duration sugar: `5.minutes` after an Integer primary.
                    if let Expr::Integer(n) = &expr {
                        if let Some(unit) = parse_duration_unit(&field) {
                            return Expr::Duration {
                                value: Box::new(Expr::Integer(*n)),
                                unit,
                            };
                        }
                    }
                    // `Urgency.medium` and `Http.ok` both emit FieldAccess;
                    // the type checker resolves enum variants vs. namespace
                    // members based on the identifier's bound type.
                    Expr::FieldAccess(Box::new(expr), field)
                }
                PostfixOp::NullDotAccess(field) => {
                    Expr::NullFieldAccess(Box::new(expr), field)
                }
                PostfixOp::NullAssert => Expr::NullAssert(Box::new(expr)),
                PostfixOp::Call(args) => Expr::Call {
                    callee: Box::new(expr),
                    args,
                },
                PostfixOp::Cast(ty) => Expr::Cast {
                    expr: Box::new(expr),
                    ty,
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
        let product = unary.clone()
            .then(
                just(Token::Star).to(BinOp::Mul)
                    .or(just(Token::Slash).to(BinOp::Div))
                    .or(just(Token::Percent).to(BinOp::Mod))
                    .then(unary)
                    .repeated(),
            )
            .foldl(|l, (op, r)| Expr::BinaryOp { left: Box::new(l), op, right: Box::new(r) })
            .boxed();

        // ── + - ──────────────────────────────────────────────────
        let sum = product.clone()
            .then(
                just(Token::Plus).to(BinOp::Add)
                    .or(just(Token::Minus).to(BinOp::Sub))
                    .then(product)
                    .repeated(),
            )
            .foldl(|l, (op, r)| Expr::BinaryOp { left: Box::new(l), op, right: Box::new(r) })
            .boxed();

        // ── == != < > <= >= ──────────────────────────────────────
        let cmp = sum.clone()
            .then(
                choice((
                    just(Token::EqEq).to(BinOp::Eq),
                    just(Token::Neq).to(BinOp::Neq),
                    just(Token::Lte).to(BinOp::Lte),
                    just(Token::Gte).to(BinOp::Gte),
                    just(Token::Lt).to(BinOp::Lt),
                    just(Token::Gt).to(BinOp::Gt),
                ))
                .then(sum)
                .repeated(),
            )
            .foldl(|l, (op, r)| Expr::BinaryOp { left: Box::new(l), op, right: Box::new(r) })
            .boxed();

        // ── and ──────────────────────────────────────────────────
        let land = cmp.clone()
            .then(just(Token::And).to(BinOp::And).then(cmp).repeated())
            .foldl(|l, (op, r)| Expr::BinaryOp { left: Box::new(l), op, right: Box::new(r) })
            .boxed();

        // ── or ───────────────────────────────────────────────────
        let lor = land.clone()
            .then(just(Token::Or).to(BinOp::Or).then(land).repeated())
            .foldl(|l, (op, r)| Expr::BinaryOp { left: Box::new(l), op, right: Box::new(r) })
            .boxed();

        // ── |> ───────────────────────────────────────────────────
        // Pipeline has lower precedence than `??` — SPEC §18.
        let pipeline = lor.clone()
            .then(just(Token::Pipe).ignore_then(lor).repeated())
            .foldl(|l, r| Expr::Pipeline(Box::new(l), Box::new(r)))
            .boxed();

        // ── ?? ───────────────────────────────────────────────────
        // Null-coalesce is the top of the expression chain.
        pipeline.clone()
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
}

fn parse_duration_unit(s: &str) -> Option<DurationUnit> {
    match s {
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

        // self.field = expr
        let self_assign = just(Token::SelfKw)
            .ignore_then(just(Token::Dot))
            .ignore_then(ident())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|(field, value)| Stmt::SelfAssign { field, value })
            .boxed();

        // x = expr  or  x: Type = expr
        let let_stmt = ident()
            .then(just(Token::Colon).ignore_then(type_expr()).or_not())
            .then_ignore(just(Token::Eq))
            .then(expr.clone())
            .map(|((name, ty), value)| Stmt::Let { name, ty, value })
            .boxed();

        let return_stmt = just(Token::Return)
            .ignore_then(expr.clone().or_not())
            .map(Stmt::Return)
            .boxed();

        let for_stmt = just(Token::For)
            .ignore_then(ident())
            .then_ignore(just(Token::In))
            .then(expr.clone())
            .then(just(Token::Where).ignore_then(expr.clone()).or_not())
            .then(block.clone())
            .map(|(((binding, iter), filter), body)| Stmt::For { binding, iter, filter, body })
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
                    Stmt::If { cond, then_body, else_body }
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
            .or(plain_string().map(|s| Pattern::Literal(Expr::StringLit(vec![StringPart::Literal(s)]))))
            .or(integer_lit().map(|n| Pattern::Literal(Expr::Integer(n))))
            .boxed();

        let when_arm_body = block.clone()
            .or(expr.clone().map(|e| vec![(Stmt::Expr(e), 0..0)]))
            .boxed();

        let when_arm = pattern
            .separated_by(just(Token::Comma))
            .at_least(1)
            .then(just(Token::Where).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::FatArrow))
            .then(when_arm_body)
            .map(|((patterns, guard), body)| WhenArm { patterns, guard, body })
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
            self_assign,
            let_stmt,
            return_stmt,
            for_stmt,
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
                .ignore_then(
                    field_def
                        .clone()
                        .separated_by(field_sep())
                        .allow_trailing(),
                )
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
                .ignore_then(ident().then(
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
                ).map(|(name, fields)| EnumVariant { name, fields }))
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

    just(Token::Type)
        .ignore_then(ident())
        .then(
            just(Token::Eq)
                .ignore_then(after_eq)
                .or(struct_def),
        )
        .map(|(name, def)| Decl::Type(TypeDecl { name, def }))
        .boxed()
}

fn interface_decl() -> P<Decl> {
    let param = ident()
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .map(|(name, ty)| Param { name, ty, default: None });

    let task_sig = just(Token::Task)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(param.separated_by(field_sep()).allow_trailing())
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(type_expr()).or_not())
        .map(|((name, params), return_type)| TaskSig { name, params, return_type });

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
        .map(|(name, ty)| Param { name, ty, default: None });

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
            Decl::Extern(ExternDecl { name, params, return_type, source })
        })
        .boxed()
}

fn use_decl() -> P<Decl> {
    let file = plain_string().map(|path| UseKind::File(path));

    let symbol = ident()
        .then_ignore(just(Token::From))
        .then(plain_string())
        .map(|(name, source)| UseKind::Symbol { name, source });

    let package = ident()
        .then(just(Token::Slash).ignore_then(ident()).repeated().at_least(1))
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
    let param = ident()
        .then_ignore(just(Token::Colon))
        .then(type_expr())
        .then(just(Token::Eq).ignore_then(expr_parser()).or_not())
        .map(|((name, ty), default)| Param { name, ty, default });

    just(Token::Task)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(param.separated_by(field_sep()).allow_trailing())
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(type_expr()).or_not())
        .then(block_toplevel())
        .map(|(((name, params), return_type), body)| TaskDecl {
            name,
            params,
            return_type,
            body,
        })
        .boxed()
}

fn agent_item() -> P<AgentItem> {
    // `@name ...` — block-body attributes get a block, others get an expr.
    let block_attr = just(Token::AtSign)
        .ignore_then(ident().try_map(|name, span| {
            if BLOCK_BODY_ATTRIBUTES.contains(&name.as_str()) {
                Ok(name)
            } else {
                Err(Simple::custom(span, format!("'{}' is not a block attribute", name)))
            }
        }))
        .then(block_toplevel())
        .map(|(name, body)| AgentItem::Attribute(AttributeDecl {
            name,
            body: AttributeBody::Block(body),
        }))
        .boxed();

    let expr_attr = just(Token::AtSign)
        .ignore_then(ident())
        .then(expr_parser())
        .map(|(name, body)| AgentItem::Attribute(AttributeDecl {
            name,
            body: AttributeBody::Expr(body),
        }))
        .boxed();

    let state = just(Token::State)
        .ignore_then(just(Token::LBrace))
        .ignore_then(newlines())
        .ignore_then(
            ident()
                .then_ignore(just(Token::Colon))
                .then(type_expr())
                .then_ignore(just(Token::Eq))
                .then(expr_parser())
                .map(|((name, ty), default)| StateField { name, ty, default })
                .separated_by(sep())
                .allow_trailing(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(AgentItem::State)
        .boxed();

    let task = task_decl().map(AgentItem::Task).boxed();

    let on_handler = just(Token::On)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(
                    ident()
                        .then_ignore(just(Token::Colon))
                        .then(type_expr())
                        .map(|(name, ty)| Param { name, ty, default: None }),
                )
                .then_ignore(just(Token::RParen))
                .or_not(),
        )
        .then(block_toplevel())
        .map(|((event, param), body)| AgentItem::On(OnHandler { event, param, body }))
        .boxed();

    choice((block_attr, expr_attr, state, task, on_handler)).boxed()
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

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

fn program_parser() -> P<Program> {
    let stmt_decl = stmt_parser().map(Decl::Stmt);

    let decl = choice((
        type_decl(),
        interface_decl(),
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
    let mut current = String::new();
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                match next {
                    'n' => { chars.next(); current.push('\n'); }
                    't' => { chars.next(); current.push('\t'); }
                    'r' => { chars.next(); current.push('\r'); }
                    '\\' => { chars.next(); current.push('\\'); }
                    '"' => { chars.next(); current.push('"'); }
                    '{' => { chars.next(); current.push('{'); }
                    '}' => { chars.next(); current.push('}'); }
                    _ => { current.push('\\'); current.push(next); chars.next(); }
                }
            }
        } else if ch == '{' {
            if !current.is_empty() {
                parts.push(StringPart::Literal(std::mem::take(&mut current)));
            }
            let mut depth = 1;
            let mut expr_text = String::new();
            while let Some(c) = chars.next() {
                if c == '{' { depth += 1; expr_text.push(c); }
                else if c == '}' {
                    depth -= 1;
                    if depth == 0 { break; }
                    expr_text.push(c);
                } else { expr_text.push(c); }
            }
            parts.push(StringPart::Interpolation(Box::new(parse_interp_expr(&expr_text))));
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

fn parse_interp_expr(text: &str) -> Expr {
    let text = text.trim();
    if text.is_empty() {
        return Expr::StringLit(vec![StringPart::Literal(String::new())]);
    }
    if text.contains('.') {
        let parts: Vec<&str> = text.split('.').collect();
        if parts[0] == "self" && parts.len() == 2 {
            return Expr::SelfAccess(parts[1].to_string());
        }
        let mut result = Expr::Ident(parts[0].to_string());
        for field in &parts[1..] {
            result = Expr::FieldAccess(Box::new(result), field.to_string());
        }
        return result;
    }
    Expr::Ident(text.to_string())
}
