//! Expression grammar and precedence climbing for Keel.
//!
//! Provides `expr_parser()`.  Internally calls `stmt::stmt_parser_with` with
//! the in-progress recursive expression handle to break the mutual-construction
//! dependency between expressions and statements (needed for lambda/block bodies).

use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;

use super::common::{
    P, field_name, field_sep, ident, integer_lit, map_key, map_lit_key, newlines, plain_string,
    sep, string_lit,
};
use super::types::type_expr;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

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
// Public entry point
// ---------------------------------------------------------------------------

pub(super) fn expr_parser() -> P<Expr> {
    recursive(|expr: Recursive<Token, Expr, Simple<Token>>| {
        // ── Inner block parser for trailing-block calls ──────────
        //
        // Blocks inside expressions (lambda bodies, trailing blocks on
        // method calls) need to contain full statements, which in turn
        // contain expressions. To avoid construction-time mutual
        // recursion between `expr_parser` and `stmt_parser`, we build
        // the statement parser here with our own `expr` handle.
        let inner_stmt = super::stmt::stmt_parser_with(expr.clone().boxed());
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
        let str_expr =
            string_lit().map(|s| Expr::StringLit(super::strings::parse_interpolation(&s)));
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
