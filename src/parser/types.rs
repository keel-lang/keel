//! Type expression grammar for Keel.
//!
//! Handles named types, generics, nullable types, struct types,
//! tuple/function types, and the special `dynamic` type.

use chumsky::prelude::*;

use crate::ast::{Field, Node, TypeExpr};
use crate::lexer::Token;

use super::common::{P, field_name, field_sep, ident, newlines};

/// Wrap a [`type_expr`] production with its source span.
///
/// Returns a `Node<TypeExpr>`. Use this at every position in the grammar where
/// the span of a type annotation is semantically useful (parameter types,
/// return types, field types, cast targets, etc.).
pub(super) fn spanned_type_expr() -> P<Node<TypeExpr>> {
    type_expr().map_with_span(Node::new).boxed()
}

pub(super) fn type_expr() -> P<TypeExpr> {
    recursive(|ty: Recursive<Token, TypeExpr, Simple<Token>>| {
        let named = ident().map(TypeExpr::Named);

        let dynamic_ty = just(Token::Ident("dynamic".to_string())).to(TypeExpr::Dynamic);

        let struct_ty = just(Token::LBrace)
            .ignore_then(newlines())
            .ignore_then(
                field_name()
                    .then_ignore(just(Token::Colon))
                    .then(ty.clone().map_with_span(Node::new))
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
