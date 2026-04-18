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
    parse(tokens, source.len(), &named)
        .unwrap_err()
        .to_string()
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
  @model "claude-haiku"
}
"#;
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(a) => {
            assert_eq!(a.name, "Hello");
            assert_eq!(a.items.len(), 2);
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
fn parse_agent_with_list_attribute() {
    let src = r#"
agent Bot {
  @role "..."
  @tools [Email, Calendar]
}
"#;
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(a) => {
            let tools = a.items.iter().find_map(|it| match it {
                AgentItem::Attribute(attr) if attr.name == "tools" => Some(attr),
                _ => None,
            }).expect("expected @tools");
            match &tools.body {
                AttributeBody::Expr(Expr::ListLit(items)) => {
                    assert_eq!(items.len(), 2);
                }
                other => panic!("expected ListLit, got {:?}", other),
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
            let limits = a.items.iter().find_map(|it| match it {
                AgentItem::Attribute(attr) if attr.name == "limits" => Some(attr),
                _ => None,
            }).expect("expected @limits");
            match &limits.body {
                AttributeBody::Expr(Expr::StructLit(fields)) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].0, "max_cost_per_request");
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
            let on_start = a.items.iter().find_map(|it| match it {
                AgentItem::Attribute(attr) if attr.name == "on_start" => Some(attr),
                _ => None,
            }).expect("expected @on_start");
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
            let state = a.items.iter().find_map(|it| match it {
                AgentItem::State(fields) => Some(fields),
                _ => None,
            }).expect("expected state");
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
            let handler = a.items.iter().find_map(|it| match it {
                AgentItem::On(h) => Some(h),
                _ => None,
            }).expect("expected on handler");
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
                Stmt::Expr(Expr::MethodCall { object, method, args }) => {
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
        Decl::Task(t) => {
            match &t.body[0].0 {
                Stmt::Let { value, .. } => {
                    assert!(matches!(value, Expr::Cast { .. }));
                }
                other => panic!("expected Let, got {:?}", other),
            }
        }
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

// ─── Error cases ────────────────────────────────────────────────────────────

#[test]
fn parse_error_on_unexpected_token() {
    let err = parse_err("type = invalid");
    assert!(!err.is_empty());
}
