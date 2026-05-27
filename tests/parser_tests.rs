use keel_lang::ast::*;
use keel_lang::lexer::lex;
use keel_lang::parser::parse;
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
    &program.declarations[0].0
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
                TypeDef::Alias(TypeExpr::Named(n)) => assert_eq!(n, "datetime"),
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
                    // Email.fetch — method, no guard
                    assert_eq!(entries[0].namespace, "Email");
                    assert_eq!(entries[0].method.as_deref(), Some("fetch"));
                    assert!(entries[0].condition.is_none());
                    // Email.send if self.confirmed — method + guard
                    assert_eq!(entries[1].namespace, "Email");
                    assert_eq!(entries[1].method.as_deref(), Some("send"));
                    assert!(entries[1].condition.is_some());
                    // Http — whole namespace, no guard
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
                AttributeBody::Expr(Expr::StructLit(fields)) => {
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

// ─── Call syntax ────────────────────────────────────────────────────────────

#[test]
fn parse_namespace_method_call() {
    // Inside a task body so we get an Expr statement
    let prog = parse_ok(r#"task t() { Ai.classify(body, as: Urgency) }"#);
    match first_decl(&prog) {
        Decl::Task(t) => {
            assert_eq!(t.body.len(), 1);
            match &t.body[0].0 {
                Stmt::Expr(Expr::MethodCall {
                    object,
                    method,
                    args,
                }) => {
                    assert!(matches!(object.as_ref(), Expr::Ident(n) if n == "Ai"));
                    assert_eq!(method, "classify");
                    assert_eq!(args.len(), 2);
                    assert!(args[0].name.is_none());
                    assert_eq!(args[1].name.as_deref(), Some("as"));
                }
                other => panic!("expected MethodCall, got {:?}", other),
            }
        }
        other => panic!("expected Task, got {:?}", other),
    }
}

#[test]
fn parse_explicit_lambda_arg() {
    let prog = parse_ok(r#"task t() { Schedule.every(5.minutes, () => { Io.notify("tick") }) }"#);
    match first_decl(&prog) {
        Decl::Task(t) => match &t.body[0].0 {
            Stmt::Expr(Expr::MethodCall { args, .. }) => {
                assert_eq!(args.len(), 2);
                assert!(matches!(&args[1].value, Expr::Lambda { .. }));
            }
            other => panic!("expected MethodCall, got {:?}", other),
        },
        other => panic!("expected Task, got {:?}", other),
    }
}

#[test]
fn parse_as_cast() {
    let prog = parse_ok(r#"task t() { x = Ai.prompt(system: "hi") as MyType }"#);
    match first_decl(&prog) {
        Decl::Task(t) => match &t.body[0].0 {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::Cast { .. }));
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
        Decl::Stmt((Stmt::Expr(Expr::Call { callee, args }), _)) => {
            assert!(matches!(callee.as_ref(), Expr::Ident(n) if n == "run"));
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
        Decl::Task(t) => match &t.body[0].0 {
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
            // The body is a single `if` statement (last-expr rule handled by checker later).
            assert_eq!(t.body.len(), 1);
        }
        _ => panic!(),
    }
}

#[test]
fn parse_null_coalesce_and_pipeline() {
    let prog = parse_ok(r#"task t() { x = a |> b ?? "default" }"#);
    match first_decl(&prog) {
        Decl::Task(t) => match &t.body[0].0 {
            Stmt::Let { value, .. } => {
                assert!(matches!(value, Expr::NullCoalesce(_, _)));
            }
            _ => panic!(),
        },
        _ => panic!(),
    }
}

// ─── Removed keywords are identifiers ────────────────────────────────────────

#[test]
fn former_keyword_classify_is_ident() {
    // `classify` is no longer a keyword — it can be a variable name, a
    // function name, or a method name. Here we use it as a method call.
    let prog = parse_ok(r#"task t() { Ai.classify(x, as: T) }"#);
    match first_decl(&prog) {
        Decl::Task(t) => match &t.body[0].0 {
            Stmt::Expr(Expr::MethodCall { method, .. }) => {
                assert_eq!(method, "classify");
            }
            _ => panic!(),
        },
        _ => panic!(),
    }
}

// ─── Generic type declarations ──────────────────────────────────────────────

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
            assert!(matches!(&td.def, TypeDef::Alias(TypeExpr::List(_))));
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
            TypeDef::Alias(TypeExpr::Func(params, ret)) => {
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
            TypeDef::Alias(TypeExpr::Func(params, ret)) => {
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
            TypeDef::Alias(TypeExpr::Func(params, _)) => {
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
            TypeDef::Alias(TypeExpr::Tuple(elems)) => {
                assert_eq!(elems.len(), 2);
            }
            other => panic!("expected Tuple alias, got {:?}", other),
        },
        other => panic!("expected Type, got {:?}", other),
    }
}

#[test]
fn parse_generic_func_type_alias() {
    // type Predicate[T] = (T) -> bool  — from SPEC §2.6
    let prog = parse_ok("type Predicate[T] = (T) -> bool");
    match first_decl(&prog) {
        Decl::Type(td) => {
            assert_eq!(td.type_params, vec!["T"]);
            assert!(matches!(&td.def, TypeDef::Alias(TypeExpr::Func(_, _))));
        }
        other => panic!("expected Type, got {:?}", other),
    }
}

// ─── Generic tasks ──────────────────────────────────────────────────────────

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

// ─── Error cases ────────────────────────────────────────────────────────────

#[test]
fn parse_error_on_unexpected_token() {
    let err = parse_err("type = invalid");
    assert!(!err.is_empty());
}

#[test]
fn parse_when_expr_produces_expr_when_node() {
    use keel_lang::ast::{Expr, Stmt};
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
    // The task body should contain a let/assign whose RHS is a WhenExpr.
    match first_decl(&prog) {
        Decl::Task(t) => {
            // find the statement with a WhenExpr on the rhs
            let has_when_expr = t.body.iter().any(|(stmt, _)| {
                matches!(
                    stmt,
                    Stmt::Let {
                        value: Expr::WhenExpr { .. },
                        ..
                    }
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
    // parse_err panics if the source parses successfully.
    // Reaching here means the parser correctly rejected the non-trailing variadic.
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
                .find(|(s, _)| matches!(s, Stmt::While { .. }));
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
//
// Regression tests for the `0..0` placeholder that was written for every
// declaration span in `program_parser()`.  Each test asserts that the stored
// span is non-empty and that the source slice it refers to begins with the
// expected keyword, proving the span actually covers the declaration's text.

fn first_span(prog: &Program) -> std::ops::Range<usize> {
    prog.declarations[0].1.clone()
}

#[test]
fn decl_span_task() {
    let src = "task greet(name: str) -> str { name }";
    let prog = parse_ok(src);
    let span = first_span(&prog);
    assert_ne!(span, 0..0, "task decl span must not be the 0..0 placeholder");
    assert_eq!(span.start, 0);
    assert!(src[span].starts_with("task"), "span should cover the task keyword");
}

#[test]
fn decl_span_agent() {
    let src = "agent Greeter { @role \"assistant\" }";
    let prog = parse_ok(src);
    let span = first_span(&prog);
    assert_ne!(span, 0..0, "agent decl span must not be the 0..0 placeholder");
    assert_eq!(span.start, 0);
    assert!(src[span].starts_with("agent"), "span should cover the agent keyword");
}

#[test]
fn decl_span_type() {
    let src = "type Color = red | green | blue";
    let prog = parse_ok(src);
    let span = first_span(&prog);
    assert_ne!(span, 0..0, "type decl span must not be the 0..0 placeholder");
    assert_eq!(span.start, 0);
    assert!(src[span].starts_with("type"), "span should cover the type keyword");
}

#[test]
fn decl_span_interface() {
    let src = "interface Printable { task print(self) -> str }";
    let prog = parse_ok(src);
    let span = first_span(&prog);
    assert_ne!(span, 0..0, "interface decl span must not be the 0..0 placeholder");
    assert_eq!(span.start, 0);
    assert!(src[span].starts_with("interface"), "span should cover the interface keyword");
}

#[test]
fn decl_span_impl() {
    let src = "impl Stringable for Point { task to_str(self) -> str { \"p\" } }";
    let prog = parse_ok(src);
    let span = first_span(&prog);
    assert_ne!(span, 0..0, "impl decl span must not be the 0..0 placeholder");
    assert_eq!(span.start, 0);
    assert!(src[span].starts_with("impl"), "span should cover the impl keyword");
}

#[test]
fn decl_span_multiple_are_distinct() {
    // Two declarations in the same file must each carry their own non-overlapping span.
    let src = "type A = x | y\ntask do_thing() -> str { \"ok\" }";
    let prog = parse_ok(src);
    assert_eq!(prog.declarations.len(), 2, "expected two declarations");
    let (span_a, span_b) = (
        prog.declarations[0].1.clone(),
        prog.declarations[1].1.clone(),
    );
    assert_ne!(span_a, 0..0, "first decl span must not be 0..0 placeholder");
    assert_ne!(span_b, 0..0, "second decl span must not be 0..0 placeholder");
    assert_ne!(span_a, span_b, "declarations must have distinct spans");
    assert!(src[span_a.clone()].starts_with("type"), "first span covers 'type'");
    assert!(src[span_b.clone()].starts_with("task"), "second span covers 'task'");
    // Spans must not overlap: first ends before second starts.
    assert!(
        span_a.end <= span_b.start,
        "spans must not overlap: {span_a:?} overlaps {span_b:?}"
    );
}
