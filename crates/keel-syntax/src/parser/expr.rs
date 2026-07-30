//! Expression grammar and precedence climbing for Keel.
//!
//! Provides `expr_parser()`.  Internally calls `stmt::stmt_parser_with` with
//! the in-progress recursive expression handle to break the mutual-construction
//! dependency between expressions and statements (needed for lambda/block bodies).

use chumsky::prelude::*;

use super::common::KeelExt;

use crate::ast::*;
use crate::lexer::Token;

use super::common::{
    P, block_with, field_name, field_sep, ident, if_body, map_key, map_lit_key, newlines,
    postfix_field_name, string_lit, when_arm, when_body,
};
use super::types::spanned_type_expr;

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
    /// Type cast: `as T` — the target annotation carries its source span.
    Cast(Node<TypeExpr>),
    Index(SpannedExpr),
}

/// Build a left-associative `BinaryOp` node spanning operands `l`..`r`.
/// Shared by every binary-precedence `foldl` level (`* / %`, `+ -`,
/// comparisons, `and`, `or`).
fn binop(l: SpannedExpr, (op, r): (BinOp, SpannedExpr)) -> SpannedExpr {
    let span = l.span.start..r.span.end;
    Node::new(
        Expr::BinaryOp {
            left: Box::new(l),
            op,
            right: Box::new(r),
        },
        span,
    )
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

pub(super) fn expr_parser() -> P<SpannedExpr> {
    recursive(|expr| {
        // ── Inner block parser for trailing-block calls ──────────
        //
        // Blocks inside expressions need full statements, which in turn
        // contain expressions.  We build the statement parser with our own
        // `expr` handle to break the mutual-construction recursion, then
        // delegate the `{ stmts }` wrapping to the shared `block_with`.
        let inner_stmt = super::stmt::stmt_parser_with(expr.clone().boxed());
        let inner_block = block_with(inner_stmt);

        // ── Literals ─────────────────────────────────────────────
        let int_lit = select! { Token::Integer(s) => s }
            .try_map(|s, span| {
                s.parse::<i64>()
                    .map(Expr::Integer)
                    .map_err(|_| Rich::custom(span, format!("integer literal `{s}` overflows i64")))
            })
            .map_with_span(Node::new);
        let float_lit = select! {
            Token::Float(s) => Expr::Float(
                s.parse::<f64>().expect("lexer regex guarantees valid f64 literal"),
            )
        }
        .map_with_span(Node::new);
        let str_expr = string_lit().map_with_span(|s, span| {
            Node::new(
                Expr::StringLit(super::strings::parse_interpolation(&s, &span)),
                span,
            )
        });
        let bool_lit = just(Token::True)
            .to(Expr::Bool(true))
            .or(just(Token::False).to(Expr::Bool(false)))
            .map_with_span(Node::new);
        let none_lit = just(Token::None_).to(Expr::None_).map_with_span(Node::new);

        // ── Lambda ───────────────────────────────────────────────
        let lambda_body = expr
            .clone()
            .map(|e| LambdaBody::Expr(Box::new(e)))
            .or(inner_block.clone().map(LambdaBody::Block))
            .boxed();

        let lambda_single = ident()
            .then_ignore(just(Token::FatArrow))
            .then(lambda_body.clone())
            .map_with_span(|(name, body), span| {
                Node::new(
                    Expr::Lambda {
                        params: vec![LambdaParam { name, ty: None }],
                        body,
                    },
                    span,
                )
            })
            .boxed();

        let lambda_multi = just(Token::LParen)
            .ignore_then(
                ident()
                    .map(|name| LambdaParam { name, ty: None })
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(just(Token::RParen))
            .then_ignore(just(Token::FatArrow))
            .then(lambda_body)
            .map_with_span(|(params, body), span| Node::new(Expr::Lambda { params, body }, span))
            .boxed();

        // ── Identifier / self ────────────────────────────────────
        let ident_expr = ident().map(Expr::Ident).map_with_span(Node::new);

        let self_access = just(Token::SelfKw)
            .ignore_then(just(Token::Dot))
            .ignore_then(
                ident().map_with_span(|field, field_span| Expr::SelfAccess { field, field_span }),
            )
            .map_with_span(Node::new);

        let self_ref = just(Token::SelfKw)
            .to(Expr::SelfRef)
            .map_with_span(Node::new);

        // ── Set literal: `set[1, 2, 3]` ──────────────────────────
        let set_lit = just(Token::Set)
            .ignore_then(just(Token::LBracket))
            .ignore_then(newlines())
            .ignore_then(
                expr.clone()
                    .separated_by(field_sep())
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBracket))
            .map_with_span(|items, span| Node::new(Expr::SetLit(items), span));

        // ── List ─────────────────────────────────────────────────
        let list_lit = just(Token::LBracket)
            .ignore_then(newlines())
            .ignore_then(
                expr.clone()
                    .separated_by(field_sep())
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBracket))
            .map_with_span(|items, span| Node::new(Expr::ListLit(items), span));

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
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .or_not()
                    .map(|o| o.unwrap_or_default()),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map_with_span(|(base, overrides), span| {
                Node::new(
                    Expr::StructSpreadUpdate {
                        base: Box::new(base),
                        overrides,
                    },
                    span,
                )
            });

        // ── Struct / map literal: `{key: expr, ...}` ────────────
        // Keys may be identifiers, contextual keywords, string literals,
        // integer literals, or boolean literals.  The AST stores all as
        // StructLit; HIR lowering resolves struct vs. map once.
        let struct_lit = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(
                map_lit_key()
                    .then_ignore(just(Token::Colon))
                    .then_ignore(newlines())
                    .then(expr.clone())
                    .separated_by(field_sep())
                    .at_least(1)
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map_with_span(|fields, span| Node::new(Expr::StructLit(fields), span));

        // ── Tuple / parenthesised ────────────────────────────────
        // Tuple requires 2+ elements; single paren is grouping.
        let tuple_or_paren = just(Token::LParen)
            .ignore_then(newlines())
            .ignore_then(expr.clone())
            .then(
                just(Token::Comma)
                    .ignore_then(newlines())
                    .ignore_then(expr.clone())
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(just(Token::Comma).or_not())
            .then_ignore(newlines())
            .then_ignore(just(Token::RParen))
            .map_with_span(|(first, rest), outer_span| {
                if rest.is_empty() {
                    // Single-element paren: the result is the inner expression,
                    // but we re-span it to cover the parentheses.
                    Node::new(first.kind, outer_span)
                } else {
                    let mut items = vec![first];
                    items.extend(rest);
                    Node::new(Expr::TupleLit(items), outer_span)
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
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
            .map_with_span(|((ty, variant), fields), span| {
                Node::new(
                    Expr::EnumVariant {
                        ty,
                        variant,
                        fields,
                    },
                    span,
                )
            })
            .boxed();

        // ── If-expression (usable on any RHS) ────────────────────
        // Recursive so that `else if` chains work.  The else-block passed to
        // `if_body` either recurses into another `if_expr` (wrapped as a
        // single-statement block) or falls back to a plain `inner_block`.
        let if_expr = recursive(|if_expr| {
            let else_arm = if_expr
                .map_with_span(|e, span| vec![Node::new(Stmt::Expr(e), span)])
                .or(inner_block.clone())
                .boxed();
            if_body(expr.clone().boxed(), inner_block.clone(), else_arm).map_with_span(
                |(cond, then_body, else_body), span| {
                    Node::new(
                        Expr::IfExpr {
                            cond: Box::new(cond),
                            then_body,
                            else_body: else_body.unwrap_or_default(),
                        },
                        span,
                    )
                },
            )
        })
        .boxed();

        let arm = when_arm(expr.clone().boxed(), inner_block.clone());

        let when_expr = when_body(expr.clone().boxed(), arm)
            .map_with_span(|(subject, arms), span| {
                Node::new(
                    Expr::WhenExpr {
                        subject: Box::new(subject),
                        arms,
                    },
                    span,
                )
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
            .ignore_then(
                call_arg_parser
                    .separated_by(field_sep())
                    .allow_trailing()
                    .collect::<Vec<_>>(),
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
        let subscript = just(Token::LBracket)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::RBracket))
            .map(PostfixOp::Index)
            .boxed();

        // Each postfix operation is tagged with its own source span so that
        // the foldl step can compute the full span of the composed expression.
        // These `(PostfixOp, Span)` pairs are parser-internal only; they are
        // not stored in the AST.
        let postfix_op = choice((
            just(Token::Dot)
                .ignore_then(postfix_field_name())
                .then(call_args.clone().or_not())
                .map(|(field, args)| PostfixOp::DotAccess { field, args }),
            just(Token::NullDot)
                .ignore_then(postfix_field_name())
                .map(PostfixOp::NullDotAccess),
            just(Token::Bang).to(PostfixOp::NullAssert),
            call_args.clone().map(PostfixOp::Call),
            just(Token::As)
                .ignore_then(spanned_type_expr())
                .map(PostfixOp::Cast),
            subscript,
        ))
        .map_with_span(|op, s| (op, s))
        .boxed();

        let postfix = primary
            .foldl(postfix_op.repeated(), |lhs_spanned, (op, op_span)| {
                let full_span = lhs_spanned.span.start..op_span.end;
                match op {
                    PostfixOp::DotAccess {
                        field,
                        args: Some(args),
                    } => Node::new(
                        Expr::MethodCall {
                            object: Box::new(lhs_spanned),
                            method: field,
                            args,
                        },
                        full_span,
                    ),
                    PostfixOp::DotAccess { field, args: None } => {
                        // Duration sugar: `5.minutes` after an Integer primary.
                        if let Expr::Integer(n) = &lhs_spanned.kind
                            && let Some(unit) = parse_duration_unit(&field)
                        {
                            let n = *n;
                            let int_span = lhs_spanned.span.clone();
                            return Node::new(
                                Expr::Duration {
                                    value: Box::new(Node::new(Expr::Integer(n), int_span)),
                                    unit,
                                },
                                full_span,
                            );
                        }
                        // `Urgency.medium` and `Http.ok` both emit FieldAccess;
                        // the type checker resolves enum variants vs. namespace
                        // members based on the identifier's bound type.
                        Node::new(Expr::FieldAccess(Box::new(lhs_spanned), field), full_span)
                    }
                    PostfixOp::NullDotAccess(field) => Node::new(
                        Expr::NullFieldAccess(Box::new(lhs_spanned), field),
                        full_span,
                    ),
                    PostfixOp::NullAssert => {
                        Node::new(Expr::NullAssert(Box::new(lhs_spanned)), full_span)
                    }
                    PostfixOp::Call(args) => Node::new(
                        Expr::Call {
                            callee: Box::new(lhs_spanned),
                            args,
                        },
                        full_span,
                    ),
                    PostfixOp::Cast(ty) => Node::new(
                        Expr::Cast {
                            expr: Box::new(lhs_spanned),
                            ty,
                        },
                        full_span,
                    ),
                    PostfixOp::Index(idx) => Node::new(
                        Expr::Index {
                            object: Box::new(lhs_spanned),
                            index: Box::new(idx),
                        },
                        full_span,
                    ),
                }
            })
            .boxed();

        // ── Unary ────────────────────────────────────────────────
        // Tag each unary operator with its span so the foldr can extend
        // the full expression span leftward from the operator position.
        let unary = just(Token::Not)
            .to(UnOp::Not)
            .or(just(Token::Minus).to(UnOp::Neg))
            .map_with_span(|op, s| (op, s))
            .repeated()
            .foldr(postfix, |(op, op_span), inner_spanned| {
                let span = op_span.start..inner_spanned.span.end;
                Node::new(
                    Expr::UnaryOp {
                        op,
                        expr: Box::new(inner_spanned),
                    },
                    span,
                )
            })
            .boxed();

        // ── * / % ────────────────────────────────────────────────
        let product = unary
            .clone()
            .foldl(
                just(Token::Star)
                    .to(BinOp::Mul)
                    .or(just(Token::Slash).to(BinOp::Div))
                    .or(just(Token::Percent).to(BinOp::Mod))
                    .then(unary)
                    .repeated(),
                binop,
            )
            .boxed();

        // ── + - ──────────────────────────────────────────────────
        let sum = product
            .clone()
            .foldl(
                just(Token::Plus)
                    .to(BinOp::Add)
                    .or(just(Token::Minus).to(BinOp::Sub))
                    .then(product)
                    .repeated(),
                binop,
            )
            .boxed();

        // ── .. ───────────────────────────────────────────────────
        // Range binds tighter than comparison: `a < 1..5` → `a < (1..5)`.
        let range = sum
            .clone()
            .then(just(Token::DotDot).ignore_then(sum.clone()).or_not())
            .map(|(start, end_opt)| match end_opt {
                Some(end_spanned) => {
                    let span = start.span.start..end_spanned.span.end;
                    Node::new(Expr::Range(Box::new(start), Box::new(end_spanned)), span)
                }
                None => start,
            })
            .boxed();

        // ── == != < > <= >= ──────────────────────────────────────
        let cmp = range
            .clone()
            .foldl(
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
                binop,
            )
            .boxed();

        // ── and ──────────────────────────────────────────────────
        let land = cmp
            .clone()
            .foldl(just(Token::And).to(BinOp::And).then(cmp).repeated(), binop)
            .boxed();

        // ── or ───────────────────────────────────────────────────
        let lor = land
            .clone()
            .foldl(just(Token::Or).to(BinOp::Or).then(land).repeated(), binop)
            .boxed();

        // ── |> ───────────────────────────────────────────────────
        // Pipeline has lower precedence than `??` — SPEC §18.
        let pipeline = lor
            .clone()
            .foldl(just(Token::Pipe).ignore_then(lor).repeated(), |l, r| {
                let span = l.span.start..r.span.end;
                Node::new(Expr::Pipeline(Box::new(l), Box::new(r)), span)
            })
            .boxed();

        // ── ?? ───────────────────────────────────────────────────
        // Null-coalesce is the top of the expression chain.
        pipeline.clone().foldl(
            just(Token::NullCoalesce).ignore_then(pipeline).repeated(),
            |l, r| {
                let span = l.span.start..r.span.end;
                Node::new(Expr::NullCoalesce(Box::new(l), Box::new(r)), span)
            },
        )
    })
    .boxed()
}
