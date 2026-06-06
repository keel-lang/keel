//! Parser for the Keel language.
//!
//! Built on [`chumsky`] 0.9. All sub-parsers return [`BoxedParser`] to avoid
//! the macOS linker crash caused by deeply nested chumsky type parameters.
//! Newlines serve as statement separators — the grammar is newline-sensitive
//! rather than semicolon-delimited.
#![allow(clippy::result_large_err)]

mod common;
mod decl;
mod error;
mod expr;
mod stmt;
mod strings;
mod types;

use chumsky::Stream;
use chumsky::prelude::*;
use miette::NamedSource;

use crate::ast::*;
use crate::lexer::{Span, Token};

use common::*;

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

    decl::program_parser()
        .parse(stream)
        .map_err(|errors| error::into_miette(errors, named_src))
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
) -> miette::Result<Vec<Node<Stmt>>> {
    let eoi = source_len..source_len + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    let parser = newlines()
        .ignore_then(stmt::stmt_parser().separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(end());

    parser
        .parse(stream)
        .map_err(|errors| error::into_miette(errors, named_src))
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::ast::*;
    use crate::lexer::lex;
    use miette::NamedSource;

    fn parse_ok(source: &str) -> Program {
        let named = NamedSource::new("test.keel", source.to_string());
        let tokens = lex(source, &named).expect("lexer failed");
        parse(tokens, source.len(), &named).expect("parser failed")
    }

    fn parse_err(source: &str) -> String {
        let named = NamedSource::new("test.keel", source.to_string());
        let tokens = lex(source, &named).expect("lexer failed");
        parse(tokens, source.len(), &named).unwrap_err().to_string()
    }

    fn first_decl(program: &Program) -> &Decl {
        &program.declarations[0].kind
    }

    // ─── Type declarations ───────────────────────────────────────────────────────

    #[test]
    fn parse_simple_enum() {
        let prog = parse_ok("type Urgency = low | medium | high | critical");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.name, "Urgency");
                match &td.def {
                    TypeDef::SimpleEnum(variants) => {
                        assert_eq!(variants, &vec!["low", "medium", "high", "critical"]);
                    }
                    other => panic!("expected SimpleEnum, got {:?}", other),
                }
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_struct_type() {
        let prog = parse_ok("type EmailInfo { sender: str, subject: str, unread: bool }");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.name, "EmailInfo");
                match &td.def {
                    TypeDef::Struct(fields) => {
                        assert_eq!(fields.len(), 3);
                        assert_eq!(fields[0].name, "sender");
                    }
                    other => panic!("expected Struct, got {:?}", other),
                }
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_type_alias() {
        let prog = parse_ok("type Timestamp = datetime");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.name, "Timestamp");
                match &td.def {
                    TypeDef::Alias(ty_node) => {
                        let TypeExpr::Named(n) = &ty_node.kind else {
                            panic!("expected Named alias, got {:?}", ty_node.kind);
                        };
                        assert_eq!(n, "datetime");
                    }
                    other => panic!("expected Alias, got {:?}", other),
                }
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    // ─── Interface declarations ──────────────────────────────────────────────────

    #[test]
    fn parse_interface() {
        let src = r#"
interface LlmProvider {
  task complete(messages: list[Message]) -> LlmResponse
  task embed(text: str) -> list[float]
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Interface(iface) => {
                assert_eq!(iface.name, "LlmProvider");
                assert_eq!(iface.methods.len(), 2);
                assert_eq!(iface.methods[0].name, "complete");
                assert_eq!(iface.methods[1].name, "embed");
            }
            other => panic!("expected Interface, got {:?}", other),
        }
    }

    // ─── Extern declarations ─────────────────────────────────────────────────────

    #[test]
    fn parse_extern_task() {
        let prog = parse_ok(r#"extern task tokenize(text: str) -> list[str] from "nlp_utils""#);
        match first_decl(&prog) {
            Decl::Extern(ex) => {
                assert_eq!(ex.name, "tokenize");
                assert_eq!(ex.source, "nlp_utils");
                assert_eq!(ex.params.len(), 1);
            }
            other => panic!("expected Extern, got {:?}", other),
        }
    }

    // ─── Use declarations ────────────────────────────────────────────────────────

    #[test]
    fn parse_use_file() {
        let prog = parse_ok(r#"use "./email_utils.keel""#);
        match first_decl(&prog) {
            Decl::Use(u) => match &u.kind {
                UseKind::File(path) => assert_eq!(path, "./email_utils.keel"),
                other => panic!("expected File, got {:?}", other),
            },
            other => panic!("expected Use, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_symbol() {
        let prog = parse_ok(r#"use Classifier from "./classifiers.keel""#);
        match first_decl(&prog) {
            Decl::Use(u) => match &u.kind {
                UseKind::Symbol { name, source } => {
                    assert_eq!(name, "Classifier");
                    assert_eq!(source, "./classifiers.keel");
                }
                other => panic!("expected Symbol, got {:?}", other),
            },
            other => panic!("expected Use, got {:?}", other),
        }
    }

    #[test]
    fn parse_use_package() {
        let prog = parse_ok("use keel/slack");
        match first_decl(&prog) {
            Decl::Use(u) => match &u.kind {
                UseKind::Package(parts) => assert_eq!(parts, &vec!["keel", "slack"]),
                other => panic!("expected Package, got {:?}", other),
            },
            other => panic!("expected Use, got {:?}", other),
        }
    }

    // ─── Tasks ───────────────────────────────────────────────────────────────────

    #[test]
    fn parse_task_with_return_type() {
        let prog = parse_ok(r#"task greet(name: str) -> str { "Hello, {name}!" }"#);
        match first_decl(&prog) {
            Decl::Task(t) => {
                assert_eq!(t.name, "greet");
                assert_eq!(t.params.len(), 1);
                assert!(t.return_type.is_some());
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    // ─── Agents: attributes ──────────────────────────────────────────────────────

    #[test]
    fn parse_agent_with_string_attributes() {
        let src = r#"
agent Hello {
  @role "A greeter"
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Agent(a) => {
                assert_eq!(a.name, "Hello");
                assert_eq!(a.items.len(), 1);
                match &a.items[0] {
                    AgentItem::Attribute(attr) => {
                        assert_eq!(attr.name, "role");
                        matches!(attr.body, AttributeBody::Expr(_));
                    }
                    other => panic!("expected Attribute, got {:?}", other),
                }
            }
            other => panic!("expected Agent, got {:?}", other),
        }
    }

    #[test]
    fn parse_agent_tools_plain() {
        let src = r#"
agent Bot {
  @tools [Email, Calendar]
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Agent(a) => {
                let tools = a
                    .items
                    .iter()
                    .find_map(|it| match it {
                        AgentItem::Attribute(attr) if attr.name == "tools" => Some(attr),
                        _ => None,
                    })
                    .expect("expected @tools");
                match &tools.body {
                    AttributeBody::Tools(entries) => {
                        assert_eq!(entries.len(), 2);
                        assert_eq!(entries[0].namespace, "Email");
                        assert!(entries[0].method.is_none());
                        assert!(entries[0].condition.is_none());
                    }
                    other => panic!("expected Tools, got {:?}", other),
                }
            }
            other => panic!("expected Agent, got {:?}", other),
        }
    }

    #[test]
    fn parse_agent_tools_with_method_and_guard() {
        let src = r#"
agent Bot {
  state { confirmed: bool = false }
  @tools [Email.fetch, Email.send if self.confirmed, Http]
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Agent(a) => {
                let tools = a
                    .items
                    .iter()
                    .find_map(|it| match it {
                        AgentItem::Attribute(attr) if attr.name == "tools" => Some(attr),
                        _ => None,
                    })
                    .expect("expected @tools");
                match &tools.body {
                    AttributeBody::Tools(entries) => {
                        assert_eq!(entries.len(), 3);
                        assert_eq!(entries[0].namespace, "Email");
                        assert_eq!(entries[0].method.as_deref(), Some("fetch"));
                        assert!(entries[0].condition.is_none());
                        assert_eq!(entries[1].namespace, "Email");
                        assert_eq!(entries[1].method.as_deref(), Some("send"));
                        assert!(entries[1].condition.is_some());
                        assert_eq!(entries[2].namespace, "Http");
                        assert!(entries[2].method.is_none());
                        assert!(entries[2].condition.is_none());
                    }
                    other => panic!("expected Tools, got {:?}", other),
                }
            }
            other => panic!("expected Agent, got {:?}", other),
        }
    }

    #[test]
    fn parse_agent_with_struct_attribute() {
        let src = r#"
agent Bot {
  @role "..."
  @limits { max_cost_per_request: 0.50, timeout: 30.seconds }
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Agent(a) => {
                let limits = a
                    .items
                    .iter()
                    .find_map(|it| match it {
                        AgentItem::Attribute(attr) if attr.name == "limits" => Some(attr),
                        _ => None,
                    })
                    .expect("expected @limits");
                match &limits.body {
                    AttributeBody::Expr(expr_node) => {
                        let Expr::StructLit(fields) = &expr_node.kind else {
                            panic!("expected StructLit, got {:?}", expr_node.kind);
                        };
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].0.as_str(), Some("max_cost_per_request"));
                    }
                    other => panic!("expected StructLit, got {:?}", other),
                }
            }
            other => panic!("expected Agent, got {:?}", other),
        }
    }

    #[test]
    fn parse_agent_with_on_start_block() {
        let src = r#"
agent Bot {
  @role "..."
  @on_start {
    Schedule.every(5.minutes, () => {
      Io.notify("tick")
    })
  }
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Agent(a) => {
                let on_start = a
                    .items
                    .iter()
                    .find_map(|it| match it {
                        AgentItem::Attribute(attr) if attr.name == "on_start" => Some(attr),
                        _ => None,
                    })
                    .expect("expected @on_start");
                match &on_start.body {
                    AttributeBody::Block(body) => {
                        assert!(!body.is_empty(), "on_start body should contain statements");
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Agent, got {:?}", other),
        }
    }

    #[test]
    fn parse_agent_with_state() {
        let src = r#"
agent Counter {
  @role "..."
  state {
    count: int = 0
    last: datetime? = none
  }
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Agent(a) => {
                let state = a
                    .items
                    .iter()
                    .find_map(|it| match it {
                        AgentItem::State(fields) => Some(fields),
                        _ => None,
                    })
                    .expect("expected state");
                assert_eq!(state.len(), 2);
                assert_eq!(state[0].name, "count");
                assert_eq!(state[1].name, "last");
            }
            other => panic!("expected Agent, got {:?}", other),
        }
    }

    #[test]
    fn parse_agent_with_on_handler() {
        let src = r#"
agent Bot {
  @role "..."
  on message(msg: Message) {
    Io.notify(msg.body)
  }
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Agent(a) => {
                let handler = a
                    .items
                    .iter()
                    .find_map(|it| match it {
                        AgentItem::On(h) => Some(h),
                        _ => None,
                    })
                    .expect("expected on handler");
                assert_eq!(handler.event, "message");
                assert!(handler.param.is_some());
            }
            other => panic!("expected Agent, got {:?}", other),
        }
    }

    // ─── Call syntax ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_namespace_method_call() {
        let prog = parse_ok(r#"task t() { Ai.classify(body, as: Urgency) }"#);
        match first_decl(&prog) {
            Decl::Task(t) => {
                assert_eq!(t.body.len(), 1);
                match &t.body[0].kind {
                    Stmt::Expr(expr_node) => match &expr_node.kind {
                        Expr::MethodCall {
                            object,
                            method,
                            args,
                        } => {
                            assert!(matches!(&object.as_ref().kind, Expr::Ident(n) if n == "Ai"));
                            assert_eq!(method, "classify");
                            assert_eq!(args.len(), 2);
                            assert!(args[0].name.is_none());
                            assert_eq!(args[1].name.as_deref(), Some("as"));
                        }
                        other => panic!("expected MethodCall, got {:?}", other),
                    },
                    other => panic!("expected Stmt::Expr, got {:?}", other),
                }
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn parse_explicit_lambda_arg() {
        let prog =
            parse_ok(r#"task t() { Schedule.every(5.minutes, () => { Io.notify("tick") }) }"#);
        match first_decl(&prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::Expr(expr_node) => match &expr_node.kind {
                    Expr::MethodCall { args, .. } => {
                        assert_eq!(args.len(), 2);
                        assert!(matches!(&args[1].value.kind, Expr::Lambda { .. }));
                    }
                    other => panic!("expected MethodCall, got {:?}", other),
                },
                other => panic!("expected Stmt::Expr, got {:?}", other),
            },
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn parse_as_cast() {
        let prog = parse_ok(r#"task t() { x = Ai.prompt(system: "hi") as MyType }"#);
        match first_decl(&prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::Let { value, .. } => {
                    assert!(matches!(&value.kind, Expr::Cast { .. }));
                }
                other => panic!("expected Let, got {:?}", other),
            },
            other => panic!("expected Task, got {:?}", other),
        }
    }

    // ─── Top-level statements ────────────────────────────────────────────────────

    #[test]
    fn parse_top_level_run() {
        let prog = parse_ok("run(MyAgent)");
        match first_decl(&prog) {
            Decl::Stmt(stmt_node) => {
                let Stmt::Expr(expr_node) = &stmt_node.kind else {
                    panic!("expected Stmt::Expr, got {:?}", stmt_node.kind);
                };
                let Expr::Call { callee, args } = &expr_node.kind else {
                    panic!("expected Expr::Call, got {:?}", expr_node.kind);
                };
                assert!(matches!(&callee.as_ref().kind, Expr::Ident(n) if n == "run"));
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected Stmt::Expr(Call), got {:?}", other),
        }
    }

    // ─── Control flow ────────────────────────────────────────────────────────────

    #[test]
    fn parse_when_with_variants() {
        let src = r#"
task t() {
  when urgency {
    low, medium => Io.notify("easy")
    high, critical => Io.notify("escalate")
  }
}
"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::When { arms, .. } => {
                    assert_eq!(arms.len(), 2);
                    assert_eq!(arms[0].patterns.len(), 2);
                }
                other => panic!("expected When, got {:?}", other),
            },
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn parse_if_expression() {
        let src = r#"task t() -> str { if x { "yes" } else { "no" } }"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Task(t) => {
                assert_eq!(t.body.len(), 1);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parse_null_coalesce_and_pipeline() {
        let prog = parse_ok(r#"task t() { x = a |> b ?? "default" }"#);
        match first_decl(&prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::Let { value, .. } => {
                    assert!(matches!(&value.kind, Expr::NullCoalesce(_, _)));
                }
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    // ─── Removed keywords are identifiers ────────────────────────────────────────

    #[test]
    fn former_keyword_classify_is_ident() {
        let prog = parse_ok(r#"task t() { Ai.classify(x, as: T) }"#);
        match first_decl(&prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::Expr(expr_node) => match &expr_node.kind {
                    Expr::MethodCall { method, .. } => {
                        assert_eq!(method, "classify");
                    }
                    _ => panic!(),
                },
                _ => panic!(),
            },
            _ => panic!(),
        }
    }

    // ─── Generic type declarations ───────────────────────────────────────────────

    #[test]
    fn parse_generic_struct_single_param() {
        let prog = parse_ok("type Paginated[T] { items: list[T]\npage: int\nhas_more: bool }");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.name, "Paginated");
                assert_eq!(td.type_params, vec!["T"]);
                assert!(matches!(&td.def, TypeDef::Struct(_)));
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_struct_multi_param() {
        let prog = parse_ok("type Pair[A, B] { first: A\nsecond: B }");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.name, "Pair");
                assert_eq!(td.type_params, vec!["A", "B"]);
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_alias() {
        let prog = parse_ok("type Bag[T] = list[T]");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.name, "Bag");
                assert_eq!(td.type_params, vec!["T"]);
                assert!(
                    matches!(&td.def, TypeDef::Alias(n) if matches!(&n.kind, TypeExpr::List(_)))
                );
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_rich_enum() {
        let prog = parse_ok("type Pair[A, B] =\n  | both { first: A, second: B }\n  | neither");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.name, "Pair");
                assert_eq!(td.type_params, vec!["A", "B"]);
                assert!(matches!(&td.def, TypeDef::RichEnum(_)));
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_non_generic_type_has_empty_params() {
        let prog = parse_ok("type Urgency = low | medium | high");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert!(td.type_params.is_empty());
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    // ─── Function type syntax ────────────────────────────────────────────────────

    #[test]
    fn parse_func_type_single_param() {
        let prog = parse_ok("type Handler = (str) -> bool");
        match first_decl(&prog) {
            Decl::Type(td) => match &td.def {
                TypeDef::Alias(ty_node) => {
                    let TypeExpr::Func(params, ret) = &ty_node.kind else {
                        panic!("expected Func alias, got {:?}", ty_node.kind);
                    };
                    assert_eq!(params.len(), 1);
                    assert!(matches!(&params[0], TypeExpr::Named(n) if n == "str"));
                    assert!(matches!(ret.as_ref(), TypeExpr::Named(n) if n == "bool"));
                }
                other => panic!("expected Func alias, got {:?}", other),
            },
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_func_type_multi_param() {
        let prog = parse_ok("type Reducer = (str, int) -> str");
        match first_decl(&prog) {
            Decl::Type(td) => match &td.def {
                TypeDef::Alias(ty_node) => {
                    let TypeExpr::Func(params, ret) = &ty_node.kind else {
                        panic!("expected Func alias, got {:?}", ty_node.kind);
                    };
                    assert_eq!(params.len(), 2);
                    assert!(matches!(ret.as_ref(), TypeExpr::Named(n) if n == "str"));
                }
                other => panic!("expected Func alias, got {:?}", other),
            },
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_func_type_no_params() {
        let prog = parse_ok("type Thunk = () -> str");
        match first_decl(&prog) {
            Decl::Type(td) => match &td.def {
                TypeDef::Alias(ty_node) => {
                    let TypeExpr::Func(params, _) = &ty_node.kind else {
                        panic!("expected Func alias, got {:?}", ty_node.kind);
                    };
                    assert!(params.is_empty());
                }
                other => panic!("expected Func alias, got {:?}", other),
            },
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_tuple_type_still_works() {
        let prog = parse_ok("type Coord = (str, int)");
        match first_decl(&prog) {
            Decl::Type(td) => match &td.def {
                TypeDef::Alias(ty_node) => {
                    let TypeExpr::Tuple(elems) = &ty_node.kind else {
                        panic!("expected Tuple alias, got {:?}", ty_node.kind);
                    };
                    assert_eq!(elems.len(), 2);
                }
                other => panic!("expected Tuple alias, got {:?}", other),
            },
            other => panic!("expected Type, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_func_type_alias() {
        let prog = parse_ok("type Predicate[T] = (T) -> bool");
        match first_decl(&prog) {
            Decl::Type(td) => {
                assert_eq!(td.type_params, vec!["T"]);
                assert!(
                    matches!(&td.def, TypeDef::Alias(n) if matches!(&n.kind, TypeExpr::Func(_, _)))
                );
            }
            other => panic!("expected Type, got {:?}", other),
        }
    }

    // ─── Generic tasks ───────────────────────────────────────────────────────────

    #[test]
    fn parse_generic_task_single_param() {
        let prog = parse_ok("task identity[T](x: T) -> T { x }");
        match first_decl(&prog) {
            Decl::Task(t) => {
                assert_eq!(t.name, "identity");
                assert_eq!(t.type_params, vec!["T"]);
                assert_eq!(t.params.len(), 1);
                assert!(t.return_type.is_some());
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_task_multi_param() {
        let prog = parse_ok("task swap[A, B](a: A, b: B) -> B { b }");
        match first_decl(&prog) {
            Decl::Task(t) => {
                assert_eq!(t.name, "swap");
                assert_eq!(t.type_params, vec!["A", "B"]);
                assert_eq!(t.params.len(), 2);
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn parse_non_generic_task_has_empty_type_params() {
        let prog = parse_ok("task greet(name: str) -> str { name }");
        match first_decl(&prog) {
            Decl::Task(t) => {
                assert!(t.type_params.is_empty());
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    // ─── Error cases ─────────────────────────────────────────────────────────────

    #[test]
    fn parse_error_on_unexpected_token() {
        let err = parse_err("type = invalid");
        assert!(!err.is_empty());
    }

    #[test]
    fn parse_errors_collected_from_multiple_declarations() {
        let src = "task a() {\n  x =\n}\ntask b() {\n  y =\n}\n";
        let named = miette::NamedSource::new("test.keel", src.to_string());
        let tokens = lex(src, &named).expect("lex ok");
        let report = super::parse(tokens, src.len(), &named).unwrap_err();
        let label_count = report.labels().map(|ls| ls.count()).unwrap_or(0);
        assert!(
            label_count >= 2,
            "expected ≥2 error labels for two broken tasks, got {label_count}"
        );
    }

    #[test]
    fn parse_when_expr_produces_expr_when_node() {
        let prog = parse_ok(
            r#"
task t(x: str) {
  result = when x {
    "a" => "alpha"
    _   => "other"
  }
}
"#,
        );
        match first_decl(&prog) {
            Decl::Task(t) => {
                let has_when_expr = t.body.iter().any(|s_node| {
                    matches!(
                        &s_node.kind,
                        Stmt::Let { value, .. } if matches!(&value.kind, Expr::WhenExpr { .. })
                    )
                });
                assert!(has_when_expr, "expected a let binding with a WhenExpr RHS");
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    // ─── Variadic parameters ─────────────────────────────────────────────────────

    #[test]
    fn variadic_trailing_param_is_ok() {
        parse_ok("task f(a: int, ...rest: str) -> str { \"ok\" }");
    }

    #[test]
    fn variadic_only_param_is_ok() {
        parse_ok("task f(...items: int) -> int { 0 }");
    }

    #[test]
    fn variadic_non_trailing_is_parse_error() {
        parse_err("task f(...a: int, b: int) -> int { 0 }");
    }

    // ─── while loop ──────────────────────────────────────────────────────────────

    #[test]
    fn parse_while_basic() {
        let prog = parse_ok("task t() { n = 0\nwhile n < 3 {\nn += 1\n}\n}");
        match first_decl(&prog) {
            Decl::Task(td) => {
                let while_stmt = td
                    .body
                    .iter()
                    .find(|s_node| matches!(&s_node.kind, Stmt::While { .. }));
                assert!(while_stmt.is_some(), "expected Stmt::While in task body");
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn parse_while_true_with_break() {
        parse_ok("task t() { while true {\nbreak\n}\n}");
    }

    #[test]
    fn parse_while_nested_in_for() {
        parse_ok(
            r#"task t() {
    for i in 1..3 {
        j = 0
        while j < i {
            j += 1
        }
    }
}"#,
        );
    }

    // ─── Impl declarations ────────────────────────────────────────────────────────

    #[test]
    fn parse_impl_stringable() {
        let src = r#"impl Stringable for Point {
  task to_str(self) -> str {
    "hello"
  }
}"#;
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Impl(id) => {
                assert_eq!(id.interface_name, "Stringable");
                assert_eq!(id.type_name, "Point");
                assert_eq!(id.methods.len(), 1);
                assert_eq!(id.methods[0].name, "to_str");
            }
            other => panic!("expected Decl::Impl, got {other:?}"),
        }
    }

    // ─── Declaration spans ────────────────────────────────────────────────────────

    fn first_span(prog: &Program) -> std::ops::Range<usize> {
        prog.declarations[0].span.clone()
    }

    #[test]
    fn decl_span_task() {
        let src = "task greet(name: str) -> str { name }";
        let prog = parse_ok(src);
        let span = first_span(&prog);
        assert_ne!(
            span,
            0..0,
            "task decl span must not be the 0..0 placeholder"
        );
        assert_eq!(span.start, 0);
        assert!(
            src[span].starts_with("task"),
            "span should cover the task keyword"
        );
    }

    #[test]
    fn decl_span_agent() {
        let src = "agent Greeter { @role \"assistant\" }";
        let prog = parse_ok(src);
        let span = first_span(&prog);
        assert_ne!(
            span,
            0..0,
            "agent decl span must not be the 0..0 placeholder"
        );
        assert_eq!(span.start, 0);
        assert!(
            src[span].starts_with("agent"),
            "span should cover the agent keyword"
        );
    }

    #[test]
    fn decl_span_type() {
        let src = "type Color = red | green | blue";
        let prog = parse_ok(src);
        let span = first_span(&prog);
        assert_ne!(
            span,
            0..0,
            "type decl span must not be the 0..0 placeholder"
        );
        assert_eq!(span.start, 0);
        assert!(
            src[span].starts_with("type"),
            "span should cover the type keyword"
        );
    }

    #[test]
    fn decl_span_interface() {
        let src = "interface Printable { task print(self) -> str }";
        let prog = parse_ok(src);
        let span = first_span(&prog);
        assert_ne!(
            span,
            0..0,
            "interface decl span must not be the 0..0 placeholder"
        );
        assert_eq!(span.start, 0);
        assert!(
            src[span].starts_with("interface"),
            "span should cover the interface keyword"
        );
    }

    #[test]
    fn decl_span_impl() {
        let src = "impl Stringable for Point { task to_str(self) -> str { \"p\" } }";
        let prog = parse_ok(src);
        let span = first_span(&prog);
        assert_ne!(
            span,
            0..0,
            "impl decl span must not be the 0..0 placeholder"
        );
        assert_eq!(span.start, 0);
        assert!(
            src[span].starts_with("impl"),
            "span should cover the impl keyword"
        );
    }

    #[test]
    fn decl_span_multiple_are_distinct() {
        let src = "type A = x | y\ntask do_thing() -> str { \"ok\" }";
        let prog = parse_ok(src);
        assert_eq!(prog.declarations.len(), 2, "expected two declarations");
        let (span_a, span_b) = (
            prog.declarations[0].span.clone(),
            prog.declarations[1].span.clone(),
        );
        assert_ne!(span_a, 0..0, "first decl span must not be 0..0 placeholder");
        assert_ne!(
            span_b,
            0..0,
            "second decl span must not be 0..0 placeholder"
        );
        assert_ne!(span_a, span_b, "declarations must have distinct spans");
        assert!(
            src[span_a.clone()].starts_with("type"),
            "first span covers 'type'"
        );
        assert!(
            src[span_b.clone()].starts_with("task"),
            "second span covers 'task'"
        );
        assert!(
            span_a.end <= span_b.start,
            "spans must not overlap: {span_a:?} overlaps {span_b:?}"
        );
    }

    // ─── Declaration name spans ───────────────────────────────────────────────────

    #[test]
    fn name_span_task() {
        let src = "task greet(name: str) -> str { name }";
        let prog = parse_ok(src);
        let Decl::Task(t) = first_decl(&prog) else {
            panic!("expected Decl::Task")
        };
        assert_eq!(&src[t.name_span.clone()], "greet");
        assert_eq!(t.name_span, 5..10);
    }

    #[test]
    fn name_span_agent() {
        let src = "agent Greeter { @role \"assistant\" }";
        let prog = parse_ok(src);
        let Decl::Agent(a) = first_decl(&prog) else {
            panic!("expected Decl::Agent")
        };
        assert_eq!(&src[a.name_span.clone()], "Greeter");
        assert_eq!(a.name_span, 6..13);
    }

    #[test]
    fn name_span_type() {
        let src = "type Color = red | green | blue";
        let prog = parse_ok(src);
        let Decl::Type(t) = first_decl(&prog) else {
            panic!("expected Decl::Type")
        };
        assert_eq!(&src[t.name_span.clone()], "Color");
        assert_eq!(t.name_span, 5..10);
    }

    #[test]
    fn name_span_interface() {
        let src = "interface Printable { task print(self) -> str }";
        let prog = parse_ok(src);
        let Decl::Interface(i) = first_decl(&prog) else {
            panic!("expected Decl::Interface")
        };
        assert_eq!(&src[i.name_span.clone()], "Printable");
        assert_eq!(i.name_span, 10..19);
    }

    #[test]
    fn name_span_task_param() {
        let src = "task greet(name: str) -> str { name }";
        let prog = parse_ok(src);
        let Decl::Task(t) = first_decl(&prog) else {
            panic!("expected Decl::Task")
        };
        let param = &t.params[0];
        assert_eq!(&src[param.name_span.clone()], "name");
        assert_eq!(param.name_span, 11..15);
    }

    #[test]
    fn name_span_task_variadic_param() {
        let src = "task log(...msgs: str) { }";
        let prog = parse_ok(src);
        let Decl::Task(t) = first_decl(&prog) else {
            panic!("expected Decl::Task")
        };
        let param = &t.params[0];
        assert_eq!(&src[param.name_span.clone()], "msgs");
        assert_eq!(param.name_span, 12..16);
    }

    #[test]
    fn name_span_interface_method() {
        let src = "interface Printable { task print(self) -> str }";
        let prog = parse_ok(src);
        let Decl::Interface(i) = first_decl(&prog) else {
            panic!("expected Decl::Interface")
        };
        let sig = &i.methods[0];
        assert_eq!(&src[sig.name_span.clone()], "print");
        assert_eq!(sig.name_span, 27..32);
    }

    #[test]
    fn name_span_agent_task() {
        let src = "agent Bot {\ntask tick() { }\n}";
        let prog = parse_ok(src);
        let Decl::Agent(a) = first_decl(&prog) else {
            panic!("expected Decl::Agent")
        };
        assert_eq!(&src[a.name_span.clone()], "Bot");
        let AgentItem::Task(t) = &a.items[0] else {
            panic!("expected AgentItem::Task")
        };
        assert_eq!(&src[t.name_span.clone()], "tick");
    }

    // ─── Type-expression span tests ───────────────────────────────────────────────

    #[test]
    fn param_ty_span() {
        let src = "task f(x: str) { }";
        let prog = parse_ok(src);
        let Decl::Task(t) = first_decl(&prog) else {
            panic!("expected Decl::Task")
        };
        let span = &t.params[0].ty.span;
        assert_eq!(&src[span.clone()], "str");
    }

    #[test]
    fn return_type_span() {
        let src = "task greet() -> bool { true }";
        let prog = parse_ok(src);
        let Decl::Task(t) = first_decl(&prog) else {
            panic!("expected Decl::Task")
        };
        let ret_node = t.return_type.as_ref().expect("expected return type");
        let span = &ret_node.span;
        assert_eq!(&src[span.clone()], "bool");
    }

    #[test]
    fn field_ty_span() {
        let src = "type Point { x: int, y: float }";
        let prog = parse_ok(src);
        let Decl::Type(td) = first_decl(&prog) else {
            panic!("expected Decl::Type")
        };
        let TypeDef::Struct(fields) = &td.def else {
            panic!("expected Struct")
        };
        let x_span = &fields[0].ty.span;
        let y_span = &fields[1].ty.span;
        assert_eq!(&src[x_span.clone()], "int");
        assert_eq!(&src[y_span.clone()], "float");
    }

    #[test]
    fn alias_ty_span() {
        let src = "type Ts = datetime";
        let prog = parse_ok(src);
        let Decl::Type(td) = first_decl(&prog) else {
            panic!("expected Decl::Type")
        };
        let TypeDef::Alias(ty_node) = &td.def else {
            panic!("expected Alias")
        };
        let span = &ty_node.span;
        assert_eq!(&src[span.clone()], "datetime");
    }

    #[test]
    fn let_ty_span() {
        let src = "task f() {\nx: int = 1\n}";
        let prog = parse_ok(src);
        let Decl::Task(t) = first_decl(&prog) else {
            panic!("expected Decl::Task")
        };
        let Stmt::Let { ty, .. } = &t.body[0].kind else {
            panic!("expected Stmt::Let")
        };
        let ty_node = ty.as_ref().expect("expected type annotation");
        let span = &ty_node.span;
        assert_eq!(&src[span.clone()], "int");
    }

    #[test]
    fn cast_ty_span() {
        let src = "task f() {\n1 as float\n}";
        let prog = parse_ok(src);
        let Decl::Task(t) = first_decl(&prog) else {
            panic!("expected Decl::Task")
        };
        let Stmt::Expr(expr_node) = &t.body[0].kind else {
            panic!("expected Stmt::Expr")
        };
        let Expr::Cast { ty, .. } = &expr_node.kind else {
            panic!("expected Expr::Cast")
        };
        let span = &ty.span;
        assert_eq!(&src[span.clone()], "float");
    }

    fn first_interpolation(parts: &[StringPart]) -> &SpannedExpr {
        parts
            .iter()
            .find_map(|part| match part {
                StringPart::Interpolation(expr, _) => Some(expr.as_ref()),
                _ => None,
            })
            .expect("expected interpolation")
    }

    fn task_body_string_parts(program: &Program) -> &[StringPart] {
        let Decl::Task(task) = first_decl(program) else {
            panic!("expected Decl::Task")
        };
        let Stmt::Expr(expr) = &task.body[0].kind else {
            panic!("expected Stmt::Expr")
        };
        let Expr::StringLit(parts) = &expr.kind else {
            panic!("expected Expr::StringLit")
        };
        parts
    }

    #[test]
    fn interpolation_slot_span_is_file_relative() {
        for src in [
            r#"task greet(name: str) { "héllo { name }" }"#,
            "task greet(name: str) { \"\"\"héllo { name }\"\"\" }",
            r#"task greet(name: str) { "héllo { name:>10}" }"#,
        ] {
            let program = parse_ok(src);
            let expr = first_interpolation(task_body_string_parts(&program));
            let name_start = src.rfind("name").expect("expected interpolated name");
            assert_eq!(expr.span, name_start..name_start + "name".len());
            assert_eq!(&src[expr.span.clone()], "name");
        }
    }

    #[test]
    fn nested_interpolation_slot_span_is_file_relative() {
        let src = r#"task greet(name: str) { "outer {"inner { name }"}" }"#;
        let program = parse_ok(src);
        let nested_string = first_interpolation(task_body_string_parts(&program));
        let Expr::StringLit(parts) = &nested_string.kind else {
            panic!("expected nested Expr::StringLit");
        };
        let expr = first_interpolation(parts);
        let name_start = src.rfind("name").expect("expected interpolated name");
        assert_eq!(expr.span, name_start..name_start + "name".len());
        assert_eq!(&src[expr.span.clone()], "name");
    }

    // ─── Deduplicated when/if grammar (#13) ──────────────────────────────────────

    #[test]
    fn when_stmt_span_covers_keyword() {
        let src =
            "task t(x: str) -> str {\n    when x {\n        _ => \"y\"\n    }\n    \"done\"\n}";
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Task(t) => {
                let when_node = &t.body[0];
                assert!(
                    matches!(when_node.kind, Stmt::When { .. }),
                    "expected Stmt::When"
                );
                assert!(
                    src[when_node.span.clone()].starts_with("when"),
                    "when stmt span must start at 'when' keyword, got: {:?}",
                    &src[when_node.span.clone()]
                );
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn when_expr_span_covers_keyword() {
        let src = "task t(x: str) -> str {\n    result = when x {\n        _ => \"y\"\n    }\n    result\n}";
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::Let { value, .. } => {
                    assert!(
                        matches!(value.kind, Expr::WhenExpr { .. }),
                        "expected Expr::WhenExpr on RHS"
                    );
                    assert!(
                        src[value.span.clone()].starts_with("when"),
                        "when expr span must start at 'when' keyword, got: {:?}",
                        &src[value.span.clone()]
                    );
                }
                other => panic!("expected Stmt::Let, got {:?}", other),
            },
            other => panic!("expected Task, got {:?}", other),
        }
    }

    #[test]
    fn when_arm_span_identical_in_stmt_and_expr_context() {
        // The arm pattern "ok" appears at the same position in both forms.
        // Both should parse and produce identically-structured arms.
        let stmt_src = "task t(x: str) -> str {\n    when x {\n        \"ok\" => \"yes\"\n        _ => \"no\"\n    }\n    \"done\"\n}";
        let expr_src = "task t(x: str) -> str {\n    r = when x {\n        \"ok\" => \"yes\"\n        _ => \"no\"\n    }\n    r\n}";

        let stmt_prog = parse_ok(stmt_src);
        let expr_prog = parse_ok(expr_src);

        let stmt_arms = match &first_decl(&stmt_prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::When { arms, .. } => arms.clone(),
                other => panic!("expected Stmt::When, got {:?}", other),
            },
            other => panic!("expected Task, got {:?}", other),
        };

        let expr_arms = match &first_decl(&expr_prog) {
            Decl::Task(t) => match &t.body[0].kind {
                Stmt::Let { value, .. } => match &value.kind {
                    Expr::WhenExpr { arms, .. } => arms.clone(),
                    other => panic!("expected Expr::WhenExpr, got {:?}", other),
                },
                other => panic!("expected Stmt::Let, got {:?}", other),
            },
            other => panic!("expected Task, got {:?}", other),
        };

        assert_eq!(
            stmt_arms.len(),
            expr_arms.len(),
            "arm count must match between stmt and expr when"
        );
        for (i, (sa, ea)) in stmt_arms.iter().zip(expr_arms.iter()).enumerate() {
            assert_eq!(
                sa.patterns.len(),
                ea.patterns.len(),
                "arm {i} pattern count must match"
            );
            assert_eq!(
                sa.body.len(),
                ea.body.len(),
                "arm {i} body statement count must match"
            );
            // Verify pattern *variant* matches — catches the case where one context
            // parses a literal arm as Wildcard and the other as Literal.
            for (j, (sp, ep)) in sa.patterns.iter().zip(ea.patterns.iter()).enumerate() {
                let same_variant = matches!(
                    (sp, ep),
                    (Pattern::Wildcard, Pattern::Wildcard)
                        | (Pattern::Ident(_), Pattern::Ident(_))
                        | (Pattern::Literal(_), Pattern::Literal(_))
                        | (Pattern::Variant { .. }, Pattern::Variant { .. })
                );
                assert!(same_variant, "arm {i} pattern {j}: stmt={sp:?} expr={ep:?}");
            }
        }
    }

    #[test]
    fn if_else_if_chain_at_stmt_position_parses() {
        // Previously produced a parse error ("found 'if' but expected '{'").
        let src =
            "task t() -> str {\n    if true { \"a\" } else if false { \"b\" } else { \"c\" }\n}";
        let prog = parse_ok(src);
        match first_decl(&prog) {
            Decl::Task(t) => {
                assert_eq!(t.body.len(), 1, "expected one statement");
                match &t.body[0].kind {
                    Stmt::If {
                        else_body: Some(else_block),
                        ..
                    } => {
                        assert_eq!(
                            else_block.len(),
                            1,
                            "else block must contain the else-if stmt"
                        );
                        assert!(
                            matches!(else_block[0].kind, Stmt::If { .. }),
                            "else block must contain Stmt::If (the else-if branch)"
                        );
                    }
                    other => panic!("expected Stmt::If with Some(else_body), got {:?}", other),
                }
            }
            other => panic!("expected Task, got {:?}", other),
        }
    }
}
