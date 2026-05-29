//! Statement and block grammar for Keel.
//!
//! Provides `stmt_parser()`, `stmt_parser_with()` (used by `expr_parser` to
//! break the mutual-recursion construction cycle), and `block_toplevel()`
//! (used by declaration parsers for task/agent/impl bodies).

use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;

use super::common::{
    P, ident, integer_lit, newlines, plain_string, sep, struct_destruct_pat, tuple_destruct_pat,
};
use super::types::spanned_type_expr;

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

pub(super) fn stmt_parser() -> P<Node<Stmt>> {
    stmt_parser_with(super::expr::expr_parser())
}

/// Build a statement parser using a pre-constructed expression parser.
/// Used internally by `expr_parser` to break mutual parser-construction
/// recursion when building trailing-block / lambda-block support.
pub(super) fn stmt_parser_with(expr: P<SpannedExpr>) -> P<Node<Stmt>> {
    recursive(|stmt: Recursive<Token, Node<Stmt>, Simple<Token>>| {
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
            .map_with_span(|((field, op), rhs), span| Stmt::SelfAssign {
                field: field.clone(),
                value: Node::new(
                    Expr::BinaryOp {
                        left: Box::new(Node::new(Expr::SelfAccess(field), span.clone())),
                        op,
                        right: Box::new(rhs),
                    },
                    span,
                ),
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
            .then(just(Token::Colon).ignore_then(spanned_type_expr()).or_not())
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
            .map_with_span(|(((cond, then_body), else_body), null_coalesce), span| {
                if let Some(default) = null_coalesce {
                    // `if { } else { } ?? default` → expression statement.
                    let if_span = cond.span.start..default.span.start;
                    let if_expr = Node::new(
                        Expr::IfExpr {
                            cond: Box::new(cond),
                            then_body,
                            else_body: else_body.unwrap_or_default(),
                        },
                        if_span,
                    );
                    Stmt::Expr(Node::new(
                        Expr::NullCoalesce(Box::new(if_expr), Box::new(default)),
                        span,
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
            .or(plain_string().map_with_span(|s, span| {
                Pattern::Literal(Node::new(
                    Expr::StringLit(vec![StringPart::Literal(s)]),
                    span,
                ))
            }))
            .or(integer_lit()
                .map_with_span(|n, span| Pattern::Literal(Node::new(Expr::Integer(n), span))))
            .boxed();

        let when_arm_body = block
            .clone()
            .or(expr.clone().map(|e| {
                let span = e.span.clone();
                vec![Node::new(Stmt::Expr(e), span)]
            }))
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
            .then(spanned_type_expr())
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
        .map_with_span(Node::new)
    })
    .boxed()
}

/// A block at the top-level of a declaration body (task, impl method, on handler).
/// Distinct from the inner `block` built inside `stmt_parser_with`, which is
/// constructed with the in-progress recursive statement handle.
pub(super) fn block_toplevel() -> P<Block> {
    just(Token::LBrace)
        .ignore_then(newlines())
        .ignore_then(stmt_parser().separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .boxed()
}
