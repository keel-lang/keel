//! Top-level declaration parsers for Keel.
//!
//! Covers `type`, `interface`, `extern task`, `use`, `task`, `agent`,
//! and `impl` declarations, plus the top-level `program_parser`.

use chumsky::prelude::*;

use super::common::KeelExt;

use crate::ast::*;
use crate::lexer::{Span, Token};

use super::common::{
    P, field_name, field_sep, ident, newlines, plain_string, sep, spanned_ident, string_lit,
    struct_destruct_pat,
};
use super::types::spanned_type_expr;

// ---------------------------------------------------------------------------
// Type declaration
// ---------------------------------------------------------------------------

pub(super) fn type_decl() -> P<Decl> {
    let field_def = field_name()
        .then_ignore(just(Token::Colon))
        .then(spanned_type_expr())
        .map(|(name, ty)| Field { name, ty });

    let rich_variant = ident()
        .then(
            just(Token::LBrace)
                .ignore_then(newlines())
                .ignore_then(
                    field_def
                        .clone()
                        .separated_by(field_sep())
                        .allow_trailing()
                        .collect::<Vec<_>>(),
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
                .ignore_then(
                    ident()
                        .then(
                            just(Token::LBrace)
                                .ignore_then(newlines())
                                .ignore_then(
                                    field_name()
                                        .then_ignore(just(Token::Colon))
                                        .then(spanned_type_expr())
                                        .map(|(n, t)| Field { name: n, ty: t })
                                        .separated_by(field_sep())
                                        .allow_trailing()
                                        .collect::<Vec<_>>(),
                                )
                                .then_ignore(newlines())
                                .then_ignore(just(Token::RBrace))
                                .or_not(),
                        )
                        .map(|(name, fields)| EnumVariant { name, fields }),
                )
                .repeated()
                .collect::<Vec<_>>(),
        )
        .map(|(first, rest)| {
            let mut variants = vec![first];
            variants.extend(rest);
            TypeDef::RichEnum(variants)
        });

    let simple_enum = ident()
        .then(
            just(Token::Bar)
                .ignore_then(ident())
                .repeated()
                .collect::<Vec<_>>(),
        )
        .map(|(first, rest)| {
            let mut names = vec![first];
            names.extend(rest);
            names
        })
        .try_map(|names, span| {
            if names.len() < 2 {
                Err(Rich::custom(span, "enum needs at least two variants"))
            } else {
                Ok(TypeDef::SimpleEnum(names))
            }
        });

    let struct_def = just(Token::LBrace)
        .ignore_then(newlines())
        .ignore_then(
            field_name()
                .then_ignore(just(Token::Colon))
                .then(spanned_type_expr())
                .map(|(n, t)| Field { name: n, ty: t })
                .separated_by(field_sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(TypeDef::Struct);

    let alias = spanned_type_expr().map(TypeDef::Alias);

    let after_eq = choice((rich_enum, simple_enum, alias));

    let type_params = just(Token::LBracket)
        .ignore_then(
            ident()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .or_not()
        .map(|p| p.unwrap_or_default());

    just(Token::Type)
        .ignore_then(spanned_ident())
        .then(type_params)
        .then(just(Token::Eq).ignore_then(after_eq).or(struct_def))
        .map(|(((name, name_span), type_params), def)| {
            Decl::Type(TypeDecl {
                name,
                name_span,
                type_params,
                def,
            })
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Interface declaration
// ---------------------------------------------------------------------------

pub(super) fn interface_decl() -> P<Decl> {
    let self_param = just(Token::SelfKw).map_with_span(|_, span: Span| Param {
        name: Binding::Ident("self".to_string()),
        name_span: span.clone(),
        ty: Node::new(TypeExpr::SelfType, span),
        default: None,
        variadic: false,
    });
    let typed_param = spanned_ident()
        .then_ignore(just(Token::Colon))
        .then(spanned_type_expr())
        .map(|((name, name_span), ty)| Param {
            name: Binding::Ident(name),
            name_span,
            ty,
            default: None,
            variadic: false,
        });
    let any_param = choice((self_param, typed_param)).boxed();

    let task_sig = just(Token::Task)
        .ignore_then(spanned_ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(
                    any_param
                        .separated_by(field_sep())
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(spanned_type_expr()).or_not())
        .map(|(((name, name_span), params), return_type)| TaskSig {
            name,
            name_span,
            params,
            return_type,
        });

    just(Token::Interface)
        .ignore_then(spanned_ident())
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(
            task_sig
                .separated_by(sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(|((name, name_span), methods)| {
            Decl::Interface(InterfaceDecl {
                name,
                name_span,
                methods,
            })
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Extern declaration
// ---------------------------------------------------------------------------

pub(super) fn extern_decl() -> P<Decl> {
    let param = spanned_ident()
        .then_ignore(just(Token::Colon))
        .then(spanned_type_expr())
        .map(|((name, name_span), ty)| Param {
            name: Binding::Ident(name),
            name_span,
            ty,
            default: None,
            variadic: false,
        });

    just(Token::Extern)
        .ignore_then(just(Token::Task))
        .ignore_then(spanned_ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(
                    param
                        .separated_by(field_sep())
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then_ignore(just(Token::Arrow))
        .then(spanned_type_expr())
        .then_ignore(just(Token::From))
        .then(plain_string())
        .map(|((((name, name_span), params), return_type), source)| {
            Decl::Extern(ExternDecl {
                name,
                name_span,
                params,
                return_type,
                source,
            })
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Use declaration
// ---------------------------------------------------------------------------

pub(super) fn use_decl() -> P<Decl> {
    let module_path = ident()
        .then(
            just(Token::Slash)
                .ignore_then(ident())
                .repeated()
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .map(|(first, rest)| {
            let mut segments = vec![first];
            segments.extend(rest);
            UseSource::Module(segments)
        });

    let source = choice((plain_string().map(UseSource::File), module_path));

    let alias = just(Token::As).ignore_then(ident());

    // `use A, B as C from "./path.keel"` / `from std/json`
    let import_item =
        spanned_ident()
            .then(alias.clone().or_not())
            .map(|((name, name_span), alias)| ImportItem {
                name,
                name_span,
                alias,
            });
    let symbols = import_item
        .separated_by(just(Token::Comma))
        .at_least(1)
        .collect::<Vec<_>>()
        .then_ignore(just(Token::From))
        .then(source.clone())
        .map(|(items, source)| UseKind::Symbols { items, source });

    // `use "./path.keel" [as alias]` / `use std/file [as alias]`
    let module = source
        .then(alias.or_not())
        .map(|(source, alias)| UseKind::Module { source, alias });

    just(Token::Use)
        .ignore_then(choice((symbols, module)))
        .map(|kind| Decl::Use(UseDecl { kind }))
        .boxed()
}

// ---------------------------------------------------------------------------
// Task declaration
// ---------------------------------------------------------------------------

pub(super) fn task_decl() -> P<TaskDecl> {
    // Capture the span of the full name/pattern for IDE features.
    let param_name_spanned = choice((
        struct_destruct_pat()
            .map(|fields| Binding::Destruct(DestructPat::Struct(fields)))
            .map_with_span(|b, span| (b, span)),
        spanned_ident().map(|(s, span)| (Binding::Ident(s), span)),
    ));
    // Ordinary param: `name: Type` with an optional `= default`.
    let regular_param = param_name_spanned
        .then_ignore(just(Token::Colon))
        .then(spanned_type_expr())
        .then(
            just(Token::Eq)
                .ignore_then(super::expr::expr_parser())
                .or_not(),
        )
        .map(|(((name, name_span), ty), default)| Param {
            name,
            name_span,
            ty,
            default,
            variadic: false,
        });
    // Variadic param: `...name: Type` — no default allowed (defaults to []).
    let variadic_param = just(Token::DotDotDot)
        .ignore_then(spanned_ident())
        .then_ignore(just(Token::Colon))
        .then(spanned_type_expr())
        .map(|((name, name_span), ty)| Param {
            name: Binding::Ident(name),
            name_span,
            ty,
            default: None,
            variadic: true,
        });
    // Each slot is either a variadic or a regular param.
    let any_param = choice((variadic_param, regular_param)).boxed();
    let param_list = just(Token::LParen)
        .ignore_then(newlines())
        .ignore_then(
            any_param
                .separated_by(field_sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
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
                Err(Rich::custom(
                    span,
                    format!("variadic parameter `...{name}` must be the last parameter"),
                ))
            } else {
                Ok(params)
            }
        });

    let type_params = just(Token::LBracket)
        .ignore_then(
            ident()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .or_not()
        .map(|p| p.unwrap_or_default());

    just(Token::Task)
        .ignore_then(spanned_ident())
        .then(type_params)
        .then(param_list)
        .then(just(Token::Arrow).ignore_then(spanned_type_expr()).or_not())
        .then(super::stmt::block_toplevel())
        .map(
            |(((((name, name_span), type_params), params), return_type), body)| TaskDecl {
                name,
                name_span,
                type_params,
                params,
                return_type,
                body,
            },
        )
        .boxed()
}

// ---------------------------------------------------------------------------
// Test declaration
// ---------------------------------------------------------------------------

pub(super) fn test_decl() -> P<Decl> {
    #[derive(Clone)]
    enum TestItem {
        Setup(Block),
        Stmt(Box<crate::ast::Node<crate::ast::Stmt>>),
    }

    let setup = just(Token::Ident("setup".to_string()))
        .ignore_then(super::stmt::block_toplevel())
        .map(TestItem::Setup)
        .boxed();

    let body_item = setup
        .or(super::stmt::stmt_parser().map(|stmt| TestItem::Stmt(Box::new(stmt))))
        .boxed();

    let param = just(Token::For)
        .ignore_then(spanned_ident())
        .then_ignore(just(Token::In))
        .then(super::expr::expr_parser())
        .map(|((name, name_span), cases)| TestParam {
            name,
            name_span,
            cases,
        })
        .or_not()
        .boxed();

    just(Token::Ident("test".to_string()))
        .ignore_then(
            string_lit()
                .map(|s| super::common::unescape_plain(&s))
                .map_with_span(|name, name_span| (name, name_span)),
        )
        .then(param)
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(body_item.separated_by(sep()).allow_trailing().collect::<Vec<_>>())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .try_map(|(((name, name_span), param), items), span| {
            let mut setup = Vec::new();
            let mut body = Vec::new();
            let mut seen_stmt = false;
            for item in items {
                match item {
                    TestItem::Setup(block) if !seen_stmt => setup.extend(block),
                    TestItem::Setup(_) => {
                        return Err(Rich::custom(
                            span,
                            "`setup` blocks must appear before assertions and other test statements",
                        ));
                    }
                    TestItem::Stmt(stmt) => {
                        seen_stmt = true;
                        body.push(*stmt);
                    }
                }
            }
            Ok(Decl::Test(TestDecl {
                name,
                name_span,
                param,
                setup,
                body,
            }))
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Agent declaration
// ---------------------------------------------------------------------------

fn agent_item() -> P<AgentItem> {
    // `@name ...` — block-body attributes get a block, others get an expr.
    let block_attr = just(Token::AtSign)
        .ignore_then(ident().try_map(|name, span| {
            if BLOCK_BODY_ATTRIBUTES.contains(&name.as_str()) {
                Ok(name)
            } else {
                Err(Rich::custom(
                    span,
                    format!("'{}' is not a block attribute", name),
                ))
            }
        }))
        .then(super::stmt::block_toplevel())
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
        .then(
            just(Token::If)
                .ignore_then(super::expr::expr_parser())
                .or_not(),
        )
        .map(|((namespace, method), condition)| ToolEntry {
            namespace,
            method,
            condition,
        });

    let tools_list = just(Token::LBracket)
        .ignore_then(newlines())
        .ignore_then(
            tool_entry
                .separated_by(field_sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBracket));

    // `@tools all` — the explicit unrestricted form. Lowered as a single
    // wildcard entry so the runtime and checker treat it uniformly.
    let tools_all = just(Token::Ident("all".to_string())).to(vec![ToolEntry {
        namespace: "all".to_string(),
        method: None,
        condition: None,
    }]);

    let tools_attr = just(Token::AtSign)
        .ignore_then(just(Token::Ident("tools".to_string())))
        .ignore_then(choice((tools_list, tools_all)))
        .map(|entries| {
            AgentItem::Attribute(AttributeDecl {
                name: "tools".to_string(),
                body: AttributeBody::Tools(entries),
            })
        })
        .boxed();

    let expr_attr = just(Token::AtSign)
        .ignore_then(ident())
        .then(super::expr::expr_parser())
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
                .map_with_span(|name, name_span| (name, name_span))
                .then_ignore(just(Token::Colon))
                .then(
                    just(Token::Ident("readonly".to_string()))
                        .or_not()
                        .map(|opt| opt.is_some()),
                )
                .then(spanned_type_expr())
                .then_ignore(just(Token::Eq))
                .then(super::expr::expr_parser())
                .map(
                    |((((name, name_span), readonly), ty), default)| StateField {
                        name,
                        name_span,
                        ty,
                        default,
                        readonly,
                    },
                )
                .separated_by(sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(AgentItem::State)
        .boxed();

    let task = task_decl().map(AgentItem::Task).boxed();

    let on_param_name_spanned = choice((
        struct_destruct_pat()
            .map(|fields| Binding::Destruct(DestructPat::Struct(fields)))
            .map_with_span(|b, span| (b, span)),
        spanned_ident().map(|(s, span)| (Binding::Ident(s), span)),
    ));
    let on_handler = just(Token::On)
        .ignore_then(ident())
        .then(
            just(Token::LParen)
                .ignore_then(
                    on_param_name_spanned
                        .then_ignore(just(Token::Colon))
                        .then(spanned_type_expr())
                        .map(|((name, name_span), ty)| Param {
                            name,
                            name_span,
                            ty,
                            default: None,
                            variadic: false,
                        }),
                )
                .then_ignore(just(Token::RParen))
                .or_not(),
        )
        .then(super::stmt::block_toplevel())
        .map(|((event, param), body)| AgentItem::On(OnHandler { event, param, body }))
        .boxed();

    choice((block_attr, tools_attr, expr_attr, state, task, on_handler)).boxed()
}

pub(super) fn agent_decl() -> P<Decl> {
    just(Token::Agent)
        .ignore_then(spanned_ident())
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(
            agent_item()
                .separated_by(sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .map(|((name, name_span), items)| {
            Decl::Agent(AgentDecl {
                name,
                name_span,
                items,
            })
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Impl declaration
// ---------------------------------------------------------------------------

pub(super) fn impl_decl() -> P<Decl> {
    // `self` as receiver param — type is filled in at registration time.
    // Span covers the `self` keyword token.
    let self_param = just(Token::SelfKw).map_with_span(|_, span: Span| Param {
        name: Binding::Ident("self".to_string()),
        name_span: span.clone(),
        ty: Node::new(TypeExpr::SelfType, span),
        default: None,
        variadic: false,
    });

    let typed_param = spanned_ident()
        .then_ignore(just(Token::Colon))
        .then(spanned_type_expr())
        .map(|((name, name_span), ty)| Param {
            name: Binding::Ident(name),
            name_span,
            ty,
            default: None,
            variadic: false,
        });

    let any_param = choice((self_param, typed_param)).boxed();

    let impl_task = just(Token::Task)
        .ignore_then(spanned_ident())
        .then(
            just(Token::LParen)
                .ignore_then(newlines())
                .ignore_then(
                    any_param
                        .separated_by(field_sep())
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then_ignore(newlines())
                .then_ignore(just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(spanned_type_expr()).or_not())
        .then(super::stmt::block_toplevel())
        .map(
            |((((name, name_span), params), return_type), body)| TaskDecl {
                name,
                name_span,
                type_params: vec![],
                params,
                return_type,
                body,
            },
        )
        .boxed();

    just(Token::Impl)
        .ignore_then(ident())
        .then_ignore(just(Token::For))
        .then(ident())
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(
            impl_task
                .separated_by(sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
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

pub(super) fn program_parser() -> P<Program> {
    // `stmt_parser()` produces `Node<Stmt>`; wrapping it in `Decl::Stmt` gives `Decl`.
    let stmt_decl = super::stmt::stmt_parser().map(Decl::Stmt);

    // Tokens that begin a top-level declaration. Used to find the next decl
    // boundary when recovering from a broken declaration.
    let decl_start = || {
        one_of([
            Token::Task,
            Token::Ident("test".to_string()),
            Token::Agent,
            Token::Interface,
            Token::Impl,
            Token::Type,
            Token::Extern,
            Token::Use,
        ])
    };

    let decl = choice((
        type_decl(),
        interface_decl(),
        impl_decl(),
        extern_decl(),
        task_decl().map(Decl::Task),
        test_decl(),
        agent_decl(),
        use_decl(),
        stmt_decl,
    ))
    .map_with_span(Node::new)
    .map(Some)
    // On failure, consume the broken declaration's tokens up to (but not
    // including) the `newline + keyword` boundary that starts the next
    // declaration, emit the error, and yield `None` so the list resumes.
    .recover_with(via_parser(
        any()
            .and_is(sep().ignore_then(decl_start()).not())
            .repeated()
            .at_least(1)
            .ignored()
            .map(|_| None),
    ))
    .boxed();

    newlines()
        .ignore_then(
            decl.separated_by(sep())
                .allow_trailing()
                .collect::<Vec<Option<Node<Decl>>>>(),
        )
        .then_ignore(newlines())
        .then_ignore(end())
        .map(|declarations| Program {
            declarations: declarations.into_iter().flatten().collect(),
        })
        .boxed()
}
