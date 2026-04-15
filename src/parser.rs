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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn newlines() -> P<()> {
    just(Token::Newline).repeated().ignored().boxed()
}

fn sep() -> P<()> {
    just(Token::Newline).repeated().at_least(1).ignored().boxed()
}

/// Separator for struct fields/items: comma, newline, or comma+newline.
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

/// Identifier that also allows reserved words commonly used as field/key names.
fn field_name() -> P<String> {
    select! {
        Token::Ident(s) => s,
        Token::User => "user".to_string(),
        Token::Model => "model".to_string(),
        Token::Role => "role".to_string(),
        Token::Type => "type".to_string(),
        Token::State => "state".to_string(),
        Token::Format => "format".to_string(),
        Token::Rules => "rules".to_string(),
        Token::From => "from".to_string(),
        Token::To => "to".to_string(),
        Token::Send => "send".to_string(),
        Token::Search => "search".to_string(),
        Token::Set => "set".to_string(),
        Token::Memory => "memory".to_string(),
        Token::Config => "config".to_string(),
        Token::Tools => "tools".to_string(),
        Token::Agent => "agent".to_string(),
        Token::Task => "task".to_string(),
        Token::Connect => "connect".to_string(),
    }
    .boxed()
}

fn string_lit() -> P<String> {
    select! { Token::StringLit(s) => s }.boxed()
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

        choice((named, struct_ty, tuple_ty))
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
        let int_lit = select! { Token::Integer(s) => Expr::Integer(s.parse::<i64>().unwrap()) };
        let float_lit = select! { Token::Float(s) => Expr::Float(s.parse::<f64>().unwrap()) };
        let str_expr = string_lit().map(|s| Expr::StringLit(parse_interpolation(&s)));
        let bool_lit = just(Token::True)
            .to(Expr::Bool(true))
            .or(just(Token::False).to(Expr::Bool(false)));
        let none_lit = just(Token::None_).to(Expr::None_);
        let now_lit = just(Token::Now).to(Expr::Now);
        // Lambda: single-param shorthand `x => expr` or multi-param `(x, y) => expr`
        let lambda_single = ident()
            .then_ignore(just(Token::FatArrow))
            .then(expr.clone())
            .map(|(name, body)| Expr::Lambda {
                params: vec![LambdaParam { name, ty: None }],
                body: Box::new(body),
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
            .then(expr.clone())
            .map(|(params, body)| Expr::Lambda {
                params,
                body: Box::new(body),
            })
            .boxed();

        let ident_expr = ident().map(Expr::Ident);

        let env_access = just(Token::Env)
            .ignore_then(just(Token::Dot))
            .ignore_then(ident())
            .map(Expr::EnvAccess);

        let self_access = just(Token::SelfKw)
            .ignore_then(just(Token::Dot))
            .ignore_then(ident())
            .map(Expr::SelfAccess);

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

        let struct_lit = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(
                field_name()
                    .then_ignore(just(Token::Colon))
                    .then_ignore(newlines())
                    .then(expr.clone())
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map(Expr::StructLit);

        let paren_expr = just(Token::LParen)
            .ignore_then(newlines())
            .ignore_then(expr.clone())
            .then_ignore(newlines())
            .then_ignore(just(Token::RParen));

        // ── AI: classify ─────────────────────────────────────────
        let classify_target = ident()
            .map(ClassifyTarget::Named)
            .or(just(Token::LBracket)
                .ignore_then(ident().separated_by(just(Token::Comma)).at_least(1))
                .then_ignore(just(Token::RBracket))
                .map(ClassifyTarget::Inline))
            .boxed();

        let criteria_entry = string_lit()
            .then_ignore(just(Token::FatArrow))
            .then(ident())
            .boxed();

        let classify_expr = just(Token::Classify)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::As))
            .then(classify_target)
            .then(
                just(Token::Considering)
                    .ignore_then(just(Token::LBracket))
                    .ignore_then(newlines())
                    .ignore_then(
                        criteria_entry
                            .separated_by(field_sep())
                            .allow_trailing(),
                    )
                    .then_ignore(newlines())
                    .then_ignore(just(Token::RBracket))
                    .or_not(),
            )
            .then(just(Token::Fallback).ignore_then(expr.clone()).or_not())
            .then(just(Token::Using).ignore_then(string_lit()).or_not())
            .map(|((((input, target), criteria), fallback), model)| Expr::Classify {
                input: Box::new(input),
                target,
                criteria,
                fallback: fallback.map(Box::new),
                model,
            })
            .boxed();

        // ── AI: summarize ────────────────────────────────────────
        let summarize_expr = just(Token::Summarize)
            .ignore_then(expr.clone())
            .then(just(Token::In).ignore_then(integer_lit()).then(ident()).or_not())
            .then(just(Token::Format).ignore_then(ident()).or_not())
            .then(just(Token::Fallback).ignore_then(expr.clone()).or_not())
            .then(just(Token::Using).ignore_then(string_lit()).or_not())
            .map(|((((input, length), fmt), fallback), model)| Expr::Summarize {
                input: Box::new(input),
                length,
                format: fmt,
                fallback: fallback.map(Box::new),
                model,
            })
            .boxed();

        // ── AI: draft ────────────────────────────────────────────
        let draft_opts = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(
                field_name()
                    .then_ignore(just(Token::Colon))
                    .then_ignore(newlines())
                    .then(expr.clone())
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .boxed();

        let draft_expr = just(Token::Draft)
            .ignore_then(str_expr.clone())
            .then(draft_opts.or_not())
            .then(just(Token::Using).ignore_then(string_lit()).or_not())
            .map(|((desc, opts), model)| Expr::Draft {
                description: Box::new(desc),
                options: opts.unwrap_or_default(),
                model,
            })
            .boxed();

        // ── Human: ask ───────────────────────────────────────────
        let ask_expr = just(Token::Ask)
            .ignore_then(just(Token::User))
            .ignore_then(expr.clone())
            .then(just(Token::Options).ignore_then(expr.clone()).or_not())
            .map(|(prompt, options)| Expr::Ask {
                prompt: Box::new(prompt),
                options: options.map(Box::new),
            })
            .boxed();

        // confirm user expr (expression form, returns bool)
        let confirm_expr = just(Token::Confirm)
            .ignore_then(just(Token::User))
            .ignore_then(expr.clone())
            .map(|message| Expr::Confirm {
                message: Box::new(message),
            })
            .boxed();

        // ── fetch ────────────────────────────────────────────────
        let fetch_expr = just(Token::Fetch)
            .ignore_then(expr.clone())
            .then(just(Token::Where).ignore_then(ident()).or_not())
            .map(|(source, filter)| Expr::Fetch {
                source: Box::new(source),
                filter: filter.map(|f| Box::new(Expr::Ident(f))),
            })
            .boxed();

        // ── recall ───────────────────────────────────────────────
        let recall_expr = just(Token::Recall)
            .ignore_then(expr.clone())
            .then(
                just(Token::Ident("limit".to_string()))
                    .ignore_then(integer_lit())
                    .or_not(),
            )
            .map(|(query, limit)| Expr::Recall {
                query: Box::new(query),
                limit,
            })
            .boxed();

        // ── delegate ─────────────────────────────────────────────
        let delegate_expr = just(Token::Delegate)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::To))
            .then(ident())
            .map(|(task_call, agent)| Expr::Delegate {
                task_call: Box::new(task_call),
                agent,
            })
            .boxed();

        // ── prompt { ... } as Type ────────────────────────────────
        let prompt_expr = just(Token::Prompt)
            .ignore_then(just(Token::LBrace))
            .ignore_then(newlines())
            .ignore_then(
                field_name()
                    .then_ignore(just(Token::Colon))
                    .then_ignore(newlines())
                    .then(expr.clone())
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .then_ignore(just(Token::As))
            .then(ident())
            .map(|(config, target_type)| Expr::Prompt {
                config,
                target_type,
            })
            .boxed();

        // ── Primary ──────────────────────────────────────────────
        let primary = choice((
            classify_expr,
            summarize_expr,
            draft_expr,
            prompt_expr,
            ask_expr,
            confirm_expr,
            fetch_expr,
            recall_expr,
            delegate_expr,
            env_access,
            self_access,
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
            paren_expr,
            ident_expr,
        ))
        .boxed();

        // ── Postfix ──────────────────────────────────────────────
        let call_args = just(Token::LParen)
            .ignore_then(newlines())
            .ignore_then(
                call_arg(expr.clone())
                    .separated_by(field_sep())
                    .allow_trailing(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RParen))
            .boxed();

        let postfix = primary
            .then(
                choice((
                    just(Token::Dot)
                        .ignore_then(field_name())
                        .then(call_args.clone().or_not())
                        .map(PostfixOp::DotAccess),
                    just(Token::NullDot)
                        .ignore_then(field_name())
                        .map(PostfixOp::NullDotAccess),
                    just(Token::Bang).to(PostfixOp::NullAssert),
                    call_args.map(PostfixOp::Call),
                ))
                .repeated(),
            )
            .foldl(|expr, op| match op {
                PostfixOp::DotAccess((field, Some(args))) => Expr::MethodCall {
                    object: Box::new(expr),
                    method: field,
                    args,
                },
                PostfixOp::DotAccess((field, None)) => {
                    if let Expr::Integer(n) = &expr {
                        if let Some(unit) = parse_duration_unit(&field) {
                            return Expr::Duration {
                                value: Box::new(Expr::Integer(*n)),
                                unit,
                            };
                        }
                    }
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

        // ── ?? ───────────────────────────────────────────────────
        let nc = lor.clone()
            .then(just(Token::NullCoalesce).ignore_then(lor).repeated())
            .foldl(|l, r| Expr::NullCoalesce(Box::new(l), Box::new(r)))
            .boxed();

        // ── |> ───────────────────────────────────────────────────
        nc.clone()
            .then(just(Token::Pipe).ignore_then(nc).repeated())
            .foldl(|l, r| Expr::Pipeline(Box::new(l), Box::new(r)))
    })
    .boxed()
}

#[derive(Debug, Clone)]
enum PostfixOp {
    DotAccess((String, Option<Vec<CallArg>>)),
    NullDotAccess(String),
    NullAssert,
    Call(Vec<CallArg>),
}

fn call_arg(expr: impl Parser<Token, Expr, Error = Simple<Token>> + Clone + 'static) -> P<CallArg> {
    ident()
        .then_ignore(just(Token::Colon))
        .then(expr.clone())
        .map(|(name, value)| CallArg { name: Some(name), value })
        .or(expr.map(|value| CallArg { name: None, value }))
        .boxed()
}

fn parse_duration_unit(s: &str) -> Option<DurationUnit> {
    match s {
        "seconds" | "second" | "sec" | "s" => Some(DurationUnit::Seconds),
        "minutes" | "minute" | "min" | "m" => Some(DurationUnit::Minutes),
        "hours" | "hour" | "hr" | "h" => Some(DurationUnit::Hours),
        "days" | "day" | "d" => Some(DurationUnit::Days),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Statements — uses recursive() to handle stmt ↔ block mutual recursion
// ---------------------------------------------------------------------------

fn stmt_parser() -> P<Spanned<Stmt>> {
    recursive(|stmt: Recursive<Token, Spanned<Stmt>, Simple<Token>>| {
        // Block parser using the recursive stmt handle (breaks mutual recursion)
        let block = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(stmt.clone().separated_by(sep()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .boxed();

        let expr = expr_parser();

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
                    // if { } else { } ?? default  →  expression statement
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

        // when arm (defined inline to use block)
        let pattern = just(Token::Ident("_".to_string()))
            .to(Pattern::Wildcard)
            .or(ident()
                .then(
                    just(Token::LBrace)
                        .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
                        .then_ignore(just(Token::RBrace))
                        .or_not(),
                )
                .map(|(name, bindings)| match bindings {
                    Some(b) => Pattern::Variant { name, bindings: b },
                    None => Pattern::Ident(name),
                }))
            .or(string_lit().map(|s| Pattern::Literal(Expr::StringLit(vec![StringPart::Literal(s)]))))
            .or(integer_lit().map(|n| Pattern::Literal(Expr::Integer(n))))
            .boxed();

        // Single-line statement forms for when arm bodies
        let inline_archive = just(Token::Archive)
            .ignore_then(expr.clone())
            .map(|value| vec![(Stmt::Archive { value }, 0..0)])
            .boxed();
        let inline_notify = just(Token::Notify)
            .ignore_then(just(Token::User))
            .ignore_then(expr.clone())
            .map(|message| vec![(Stmt::Notify { message }, 0..0)])
            .boxed();
        let inline_send = just(Token::Send)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::To))
            .then(expr.clone())
            .map(|(value, target)| vec![(Stmt::Send { value, target }, 0..0)])
            .boxed();
        let inline_expr = expr.clone().map(|e| vec![(Stmt::Expr(e), 0..0)]).boxed();

        let when_arm = pattern
            .separated_by(just(Token::Comma))
            .at_least(1)
            .then(just(Token::Where).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::FatArrow))
            .then(choice((
                block.clone(),
                inline_archive,
                inline_notify,
                inline_send,
                inline_expr,
            )))
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

        let notify_stmt = just(Token::Notify)
            .ignore_then(just(Token::User))
            .ignore_then(expr.clone())
            .map(|message| Stmt::Notify { message })
            .boxed();

        let show_stmt = just(Token::Show)
            .ignore_then(just(Token::User))
            .ignore_then(expr.clone())
            .map(|value| Stmt::Show { value })
            .boxed();

        let send_stmt = just(Token::Send)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::To))
            .then(expr.clone())
            .map(|(value, target)| Stmt::Send { value, target })
            .boxed();

        let archive_stmt = just(Token::Archive)
            .ignore_then(expr.clone())
            .map(|value| Stmt::Archive { value })
            .boxed();

        let confirm_then = just(Token::Confirm)
            .ignore_then(just(Token::User))
            .ignore_then(expr.clone())
            .then_ignore(just(Token::Then))
            .then(
                block.clone().or(
                    just(Token::Send)
                        .ignore_then(expr.clone())
                        .then_ignore(just(Token::To))
                        .then(expr.clone())
                        .map(|(value, target)| vec![(Stmt::Send { value, target }, 0..0)]),
                ),
            )
            .map(|(message, then_body)| Stmt::ConfirmThen { message, then_body })
            .boxed();

        let remember_stmt = just(Token::Remember)
            .ignore_then(expr.clone())
            .map(|value| Stmt::Remember { value })
            .boxed();

        let after_stmt = just(Token::After)
            .ignore_then(expr.clone())
            .then(block.clone())
            .map(|(delay, body)| Stmt::After { delay, body })
            .boxed();

        // retry N times [with backoff] { body }
        let retry_stmt = just(Token::Retry)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::Times))
            .then(
                just(Token::With)
                    .ignore_then(just(Token::Backoff))
                    .or_not(),
            )
            .then(block.clone())
            .map(|((count, backoff), body)| Stmt::Retry {
                count,
                backoff: backoff.is_some(),
                body,
            })
            .boxed();

        // wait duration | wait until condition
        let wait_stmt = just(Token::Wait)
            .ignore_then(
                // wait until condition
                just(Token::Ident("until".to_string()))
                    .ignore_then(expr.clone())
                    .map(|cond| Stmt::Wait { duration: None, condition: Some(cond) })
                    // wait duration
                    .or(expr.clone().map(|dur| Stmt::Wait { duration: Some(dur), condition: None }))
            )
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
            confirm_then,
            notify_stmt,
            show_stmt,
            send_stmt,
            archive_stmt,
            remember_stmt,
            after_stmt,
            retry_stmt,
            wait_stmt,
            expr_stmt,
        ))
        .map_with_span(|stmt, span| (stmt, span))
    })
    .boxed()
}

/// Block parser for use in top-level declarations (task, agent items).
/// This calls stmt_parser() once and is NOT called from within stmt_parser.
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
    just(Token::Type)
        .ignore_then(ident())
        .then(
            just(Token::Eq)
                .ignore_then(
                    just(Token::Bar)
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
                                .map(|(name, fields)| EnumVariant { name, fields })
                                .separated_by(newlines().then(just(Token::Bar)))
                                .at_least(1),
                        )
                        .map(TypeDef::RichEnum)
                        .or(ident().separated_by(just(Token::Bar)).at_least(1).map(TypeDef::SimpleEnum)),
                )
                .or(just(Token::LBrace)
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
                    .map(TypeDef::Struct)),
        )
        .map(|(name, def)| Decl::Type(TypeDecl { name, def }))
        .boxed()
}

fn connect_decl() -> P<Decl> {
    just(Token::Connect)
        .ignore_then(ident())
        .then_ignore(just(Token::Via))
        .then(ident())
        .then(
            just(Token::LBrace)
                .ignore_then(newlines())
                .ignore_then(
                    field_name()
                        .then_ignore(just(Token::Colon))
                        .then_ignore(newlines())
                        .then(expr_parser())
                        .separated_by(field_sep())
                        .allow_trailing(),
                )
                .then_ignore(newlines())
                .then_ignore(just(Token::RBrace))
                .or_not(),
        )
        .map(|((name, protocol), config)| {
            Decl::Connect(ConnectDecl {
                name,
                protocol,
                config: config.unwrap_or_default(),
            })
        })
        .boxed()
}

fn task_decl() -> P<TaskDecl> {
    just(Token::Task)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(
                    ident()
                        .then_ignore(just(Token::Colon))
                        .then(type_expr())
                        .then(just(Token::Eq).ignore_then(expr_parser()).or_not())
                        .map(|((name, ty), default)| Param { name, ty, default })
                        .separated_by(field_sep())
                        .allow_trailing(),
                )
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

fn agent_item() -> P<AgentItem> {
    let role = just(Token::Role).ignore_then(string_lit()).map(AgentItem::Role).boxed();
    let model = just(Token::Model).ignore_then(string_lit()).map(AgentItem::Model).boxed();

    let tools = just(Token::Tools)
        .ignore_then(just(Token::LBracket))
        .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
        .then_ignore(just(Token::RBracket))
        .map(AgentItem::Tools)
        .boxed();

    let memory = just(Token::Memory)
        .ignore_then(choice((
            just(Token::None_).to(MemoryMode::None_),
            just(Token::Session).to(MemoryMode::Session),
            just(Token::Persistent).to(MemoryMode::Persistent),
        )))
        .map(AgentItem::Memory)
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

    let config = just(Token::Config)
        .ignore_then(just(Token::LBrace))
        .ignore_then(newlines())
        .ignore_then(
            field_name()
                .then_ignore(just(Token::Colon))
                .then(expr_parser())
                .separated_by(field_sep())
                .allow_trailing(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(AgentItem::Config)
        .boxed();

    let task = task_decl().map(AgentItem::Task).boxed();

    let every = just(Token::Every)
        .ignore_then(expr_parser())
        .then(block_toplevel())
        .map(|(interval, body)| AgentItem::Every(EveryBlock { interval, body }))
        .boxed();

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

    let team = just(Token::Team)
        .ignore_then(just(Token::LBracket))
        .ignore_then(ident().separated_by(just(Token::Comma)).allow_trailing())
        .then_ignore(just(Token::RBracket))
        .map(AgentItem::Team)
        .boxed();

    choice((role, model, tools, memory, state, config, task, every, on_handler, team)).boxed()
}

fn run_stmt() -> P<Decl> {
    just(Token::Run)
        .ignore_then(ident())
        .then(just(Token::In).ignore_then(just(Token::Background)).or_not())
        .map(|(agent, bg)| Decl::Run(RunStmt { agent, background: bg.is_some() }))
        .boxed()
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

fn program_parser() -> P<Program> {
    let decl = choice((
        type_decl(),
        connect_decl(),
        task_decl().map(Decl::Task),
        agent_decl(),
        run_stmt(),
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
        if parts[0] == "env" && parts.len() == 2 {
            return Expr::EnvAccess(parts[1].to_string());
        }
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
