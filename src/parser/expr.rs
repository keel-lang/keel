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
    recursive(|expr: Recursive<Token, SpannedExpr, Simple<Token>>| {
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
        let int_lit = select! { Token::Integer(s) => s }
            .try_map(|s, span| {
                s.parse::<i64>().map(Expr::Integer).map_err(|_| {
                    Simple::custom(span, format!("integer literal `{s}` overflows i64"))
                })
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
                Expr::StringLit(super::strings::parse_interpolation(&s)),
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
                    .allow_trailing(),
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
            .ignore_then(ident())
            .map(Expr::SelfAccess)
            .map_with_span(Node::new);

        let self_ref = just(Token::SelfKw)
            .to(Expr::SelfRef)
            .map_with_span(Node::new);

        // ── Set literal: `set[1, 2, 3]` ──────────────────────────
        let set_lit = just(Token::Set)
            .ignore_then(just(Token::LBracket))
            .ignore_then(newlines())
            .ignore_then(expr.clone().separated_by(field_sep()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBracket))
            .map_with_span(|items, span| Node::new(Expr::SetLit(items), span));

        // ── List ─────────────────────────────────────────────────
        let list_lit = just(Token::LBracket)
            .ignore_then(newlines())
            .ignore_then(expr.clone().separated_by(field_sep()).allow_trailing())
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
                            .allow_trailing(),
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
                    .repeated(),
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
                    .allow_trailing(),
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
        // Recursive so that `else if` chains work: the else-branch is either
        // another if-expression (wrapped as a single spanned Stmt::Expr) or a { block }.
        let if_expr = recursive(|if_expr: Recursive<Token, SpannedExpr, Simple<Token>>| {
            just(Token::If)
                .ignore_then(expr.clone())
                .then(inner_block.clone())
                .then(
                    just(Token::Else)
                        .ignore_then(
                            if_expr
                                .map_with_span(|e, span| vec![Node::new(Stmt::Expr(e), span)])
                                .or(inner_block.clone()),
                        )
                        .or_not(),
                )
                .map_with_span(|((cond, then_body), else_body), span| {
                    Node::new(
                        Expr::IfExpr {
                            cond: Box::new(cond),
                            then_body,
                            else_body: else_body.unwrap_or_default(),
                        },
                        span,
                    )
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
            .or(plain_string().map_with_span(|s, span| {
                Pattern::Literal(Node::new(
                    Expr::StringLit(vec![StringPart::Literal(s)]),
                    span,
                ))
            }))
            .or(integer_lit()
                .map_with_span(|n, span| Pattern::Literal(Node::new(Expr::Integer(n), span))))
            .boxed();

        let we_arm_body = inner_block
            .clone()
            .or(expr
                .clone()
                .map_with_span(|e, span| vec![Node::new(Stmt::Expr(e), span)]))
            .boxed();

        let we_arm = we_pattern
            .separated_by(just(Token::Comma))
            .at_least(1)
            .then(just(Token::Where).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::FatArrow))
            .then(we_arm_body)
            .map(|((patterns, guard), body)| WhenArm {
                patterns,
                guard,
                body,
            })
            .boxed();

        let when_expr = just(Token::When)
            .ignore_then(expr.clone())
            .then_ignore(just(Token::LBrace))
            .then_ignore(newlines())
            .then(we_arm.separated_by(newlines()).allow_trailing())
            .then_ignore(newlines())
            .then_ignore(just(Token::RBrace))
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

        // Each postfix operation is tagged with its own source span so that
        // the foldl step can compute the full span of the composed expression.
        // These `(PostfixOp, Span)` pairs are parser-internal only; they are
        // not stored in the AST.
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
                .ignore_then(spanned_type_expr())
                .map(PostfixOp::Cast),
            subscript,
        ))
        .map_with_span(|op, s| (op, s))
        .boxed();

        let postfix = primary
            .then(postfix_op.repeated())
            .foldl(|lhs_spanned, (op, op_span)| {
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
            .then(postfix)
            .foldr(|(op, op_span), inner_spanned| {
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
            .then(
                just(Token::Star)
                    .to(BinOp::Mul)
                    .or(just(Token::Slash).to(BinOp::Div))
                    .or(just(Token::Percent).to(BinOp::Mod))
                    .then(unary)
                    .repeated(),
            )
            .foldl(|l, (op, r)| {
                let span = l.span.start..r.span.end;
                Node::new(
                    Expr::BinaryOp {
                        left: Box::new(l),
                        op,
                        right: Box::new(r),
                    },
                    span,
                )
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
            .foldl(|l, (op, r)| {
                let span = l.span.start..r.span.end;
                Node::new(
                    Expr::BinaryOp {
                        left: Box::new(l),
                        op,
                        right: Box::new(r),
                    },
                    span,
                )
            })
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
            .foldl(|l, (op, r)| {
                let span = l.span.start..r.span.end;
                Node::new(
                    Expr::BinaryOp {
                        left: Box::new(l),
                        op,
                        right: Box::new(r),
                    },
                    span,
                )
            })
            .boxed();

        // ── and ──────────────────────────────────────────────────
        let land = cmp
            .clone()
            .then(just(Token::And).to(BinOp::And).then(cmp).repeated())
            .foldl(|l, (op, r)| {
                let span = l.span.start..r.span.end;
                Node::new(
                    Expr::BinaryOp {
                        left: Box::new(l),
                        op,
                        right: Box::new(r),
                    },
                    span,
                )
            })
            .boxed();

        // ── or ───────────────────────────────────────────────────
        let lor = land
            .clone()
            .then(just(Token::Or).to(BinOp::Or).then(land).repeated())
            .foldl(|l, (op, r)| {
                let span = l.span.start..r.span.end;
                Node::new(
                    Expr::BinaryOp {
                        left: Box::new(l),
                        op,
                        right: Box::new(r),
                    },
                    span,
                )
            })
            .boxed();

        // ── |> ───────────────────────────────────────────────────
        // Pipeline has lower precedence than `??` — SPEC §18.
        let pipeline = lor
            .clone()
            .then(just(Token::Pipe).ignore_then(lor).repeated())
            .foldl(|l, r| {
                let span = l.span.start..r.span.end;
                Node::new(Expr::Pipeline(Box::new(l), Box::new(r)), span)
            })
            .boxed();

        // ── ?? ───────────────────────────────────────────────────
        // Null-coalesce is the top of the expression chain.
        pipeline
            .clone()
            .then(just(Token::NullCoalesce).ignore_then(pipeline).repeated())
            .foldl(|l, r| {
                let span = l.span.start..r.span.end;
                Node::new(Expr::NullCoalesce(Box::new(l), Box::new(r)), span)
            })
    })
    .boxed()
}
