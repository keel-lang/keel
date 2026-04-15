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
                    assert_eq!(variants, &["low", "medium", "high", "critical"]);
                }
                _ => panic!("Expected SimpleEnum"),
            }
        }
        _ => panic!("Expected Type declaration"),
    }
}

#[test]
fn parse_struct_type() {
    let prog = parse_ok("type EmailInfo {\n  sender: str,\n  subject: str\n}");
    match first_decl(&prog) {
        Decl::Type(td) => {
            assert_eq!(td.name, "EmailInfo");
            match &td.def {
                TypeDef::Struct(fields) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "sender");
                    assert_eq!(fields[1].name, "subject");
                }
                _ => panic!("Expected Struct"),
            }
        }
        _ => panic!("Expected Type declaration"),
    }
}

#[test]
fn parse_struct_newline_separated() {
    let prog = parse_ok("type Result {\n  urgency: str\n  category: str\n}");
    match first_decl(&prog) {
        Decl::Type(td) => match &td.def {
            TypeDef::Struct(fields) => assert_eq!(fields.len(), 2),
            _ => panic!("Expected Struct"),
        },
        _ => panic!("Expected Type"),
    }
}

// ─── Connect declarations ────────────────────────────────────────────────────

#[test]
fn parse_connect_with_config() {
    let src = "connect email via imap {\n  host: env.IMAP_HOST,\n  user: env.EMAIL_USER\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Connect(cd) => {
            assert_eq!(cd.name, "email");
            assert_eq!(cd.protocol, "imap");
            assert_eq!(cd.config.len(), 2);
            assert_eq!(cd.config[0].0, "host");
            assert_eq!(cd.config[1].0, "user");
        }
        _ => panic!("Expected Connect"),
    }
}

#[test]
fn parse_connect_no_config() {
    let prog = parse_ok("connect slack via webhook");
    match first_decl(&prog) {
        Decl::Connect(cd) => {
            assert_eq!(cd.name, "slack");
            assert_eq!(cd.protocol, "webhook");
            assert!(cd.config.is_empty());
        }
        _ => panic!("Expected Connect"),
    }
}

// ─── Task declarations ───────────────────────────────────────────────────────

#[test]
fn parse_simple_task() {
    let src = "task greet(name: str) -> str {\n  \"Hello!\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => {
            assert_eq!(td.name, "greet");
            assert_eq!(td.params.len(), 1);
            assert_eq!(td.params[0].name, "name");
            assert!(td.return_type.is_some());
        }
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_task_multiple_params() {
    let src = "task compose(email: str, tone: str) -> str {\n  \"ok\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => {
            assert_eq!(td.params.len(), 2);
            assert_eq!(td.params[0].name, "email");
            assert_eq!(td.params[1].name, "tone");
        }
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_task_with_default_param() {
    let src = "task compose(tone: str = \"friendly\") -> str {\n  \"ok\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => {
            assert!(td.params[0].default.is_some());
        }
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_task_struct_param() {
    let src = "task triage(email: {body: str, from: str}) -> str {\n  \"ok\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => {
            match &td.params[0].ty {
                TypeExpr::Struct(fields) => {
                    assert_eq!(fields.len(), 2);
                    assert_eq!(fields[0].name, "body");
                    assert_eq!(fields[1].name, "from");
                }
                _ => panic!("Expected struct param type"),
            }
        }
        _ => panic!("Expected Task"),
    }
}

// ─── Agent declarations ──────────────────────────────────────────────────────

#[test]
fn parse_minimal_agent() {
    let src = "agent Hello {\n  role \"greeter\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(ad) => {
            assert_eq!(ad.name, "Hello");
            assert!(ad.items.iter().any(|i| matches!(i, AgentItem::Role(_))));
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn parse_agent_with_model_and_tools() {
    let src = "agent Bot {\n  role \"helper\"\n  model \"claude-haiku\"\n  tools [email, web]\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(ad) => {
            assert!(ad.items.iter().any(|i| matches!(i, AgentItem::Model(_))));
            assert!(ad.items.iter().any(|i| {
                if let AgentItem::Tools(t) = i {
                    t.len() == 2
                } else {
                    false
                }
            }));
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn parse_agent_with_state() {
    let src = "agent Counter {\n  role \"counter\"\n  state {\n    count: int = 0\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(ad) => {
            let has_state = ad.items.iter().any(|i| {
                if let AgentItem::State(fields) = i {
                    fields.len() == 1 && fields[0].name == "count"
                } else {
                    false
                }
            });
            assert!(has_state);
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn parse_agent_with_every() {
    let src = "agent Poller {\n  role \"poller\"\n  every 5.minutes {\n    notify user \"tick\"\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(ad) => {
            assert!(ad.items.iter().any(|i| matches!(i, AgentItem::Every(_))));
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn parse_agent_memory() {
    let src = "agent Bot {\n  role \"bot\"\n  memory persistent\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(ad) => {
            assert!(ad.items.iter().any(|i| matches!(i, AgentItem::Memory(MemoryMode::Persistent))));
        }
        _ => panic!("Expected Agent"),
    }
}

// ─── Run statement ───────────────────────────────────────────────────────────

#[test]
fn parse_run() {
    let prog = parse_ok("agent A {\n  role \"a\"\n}\nrun A");
    let last = &prog.declarations.last().unwrap().0;
    match last {
        Decl::Run(rs) => {
            assert_eq!(rs.agent, "A");
            assert!(!rs.background);
        }
        _ => panic!("Expected Run"),
    }
}

// ─── Expressions ─────────────────────────────────────────────────────────────

#[test]
fn parse_classify_expr() {
    let src = "task t(x: str) -> str {\n  classify x as Urgency fallback medium\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => {
            let stmt = &td.body[0].0;
            match stmt {
                Stmt::Expr(Expr::Classify { target, fallback, .. }) => {
                    assert!(matches!(target, ClassifyTarget::Named(n) if n == "Urgency"));
                    assert!(fallback.is_some());
                }
                _ => panic!("Expected Classify expression, got {:?}", stmt),
            }
        }
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_draft_expr() {
    let src = "task t() -> str {\n  draft \"hello\" {\n    tone: \"friendly\"\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => {
            let stmt = &td.body[0].0;
            match stmt {
                Stmt::Expr(Expr::Draft { options, .. }) => {
                    assert_eq!(options.len(), 1);
                    assert_eq!(options[0].0, "tone");
                }
                _ => panic!("Expected Draft, got {:?}", stmt),
            }
        }
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_summarize_expr() {
    let src = "task t(x: str) -> str {\n  summarize x in 1 sentence fallback \"none\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::Expr(Expr::Summarize { length, fallback, .. }) => {
                assert!(length.is_some());
                assert!(fallback.is_some());
            }
            other => panic!("Expected Summarize, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_ask_expr() {
    let src = "task t() -> str {\n  ask user \"What?\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::Expr(Expr::Ask { .. }) => {}
            other => panic!("Expected Ask, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

// ─── Control flow ────────────────────────────────────────────────────────────

#[test]
fn parse_if_else() {
    let src = "task t(x: bool) {\n  if x {\n    notify user \"yes\"\n  } else {\n    notify user \"no\"\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::If { else_body, .. } => assert!(else_body.is_some()),
            other => panic!("Expected If, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_if_no_else() {
    let src = "task t(x: bool) {\n  if x {\n    notify user \"yes\"\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::If { else_body, .. } => assert!(else_body.is_none()),
            other => panic!("Expected If, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_when() {
    let src = "task t(x: str) {\n  when x {\n    low => notify user \"low\"\n    high => notify user \"high\"\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::When { arms, .. } => assert_eq!(arms.len(), 2),
            other => panic!("Expected When, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_when_multi_pattern() {
    let src = "task t(x: str) {\n  when x {\n    low, medium => notify user \"ok\"\n    _ => notify user \"other\"\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::When { arms, .. } => {
                assert_eq!(arms[0].patterns.len(), 2);
            }
            other => panic!("Expected When, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_for_loop() {
    let src = "task t(items: list[str]) {\n  for item in items {\n    notify user item\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::For { binding, .. } => assert_eq!(binding, "item"),
            other => panic!("Expected For, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

// ─── Statements ──────────────────────────────────────────────────────────────

#[test]
fn parse_assignment() {
    let src = "task t() {\n  x = 42\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::Let { name, .. } => assert_eq!(name, "x"),
            other => panic!("Expected Let, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_typed_assignment() {
    let src = "task t() {\n  x: int = 42\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::Let { name, ty, .. } => {
                assert_eq!(name, "x");
                assert!(ty.is_some());
            }
            other => panic!("Expected Let, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_self_assign() {
    let src = "agent A {\n  role \"a\"\n  state { count: int = 0 }\n  task inc() {\n    self.count = self.count + 1\n  }\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Agent(ad) => {
            let task = ad.items.iter().find_map(|i| {
                if let AgentItem::Task(t) = i { Some(t) } else { None }
            }).unwrap();
            match &task.body[0].0 {
                Stmt::SelfAssign { field, .. } => assert_eq!(field, "count"),
                other => panic!("Expected SelfAssign, got {:?}", other),
            }
        }
        _ => panic!("Expected Agent"),
    }
}

#[test]
fn parse_notify() {
    let src = "task t() {\n  notify user \"hello\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::Notify { .. } => {}
            other => panic!("Expected Notify, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_send() {
    let src = "task t(x: str) {\n  send x to email\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::Send { .. } => {}
            other => panic!("Expected Send, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

#[test]
fn parse_return() {
    let src = "task t() -> str {\n  return \"done\"\n}";
    let prog = parse_ok(src);
    match first_decl(&prog) {
        Decl::Task(td) => match &td.body[0].0 {
            Stmt::Return(Some(_)) => {}
            other => panic!("Expected Return, got {:?}", other),
        },
        _ => panic!("Expected Task"),
    }
}

// ─── Full program ────────────────────────────────────────────────────────────

#[test]
fn parse_multi_declaration_program() {
    let src = "type Status = active | inactive\n\ntask greet(name: str) -> str {\n  \"hello\"\n}\n\nagent Bot {\n  role \"bot\"\n}\n\nrun Bot";
    let prog = parse_ok(src);
    assert_eq!(prog.declarations.len(), 4);
    assert!(matches!(prog.declarations[0].0, Decl::Type(_)));
    assert!(matches!(prog.declarations[1].0, Decl::Task(_)));
    assert!(matches!(prog.declarations[2].0, Decl::Agent(_)));
    assert!(matches!(prog.declarations[3].0, Decl::Run(_)));
}

// ─── Error reporting ─────────────────────────────────────────────────────────

#[test]
fn parse_error_unclosed_brace() {
    let msg = parse_err("agent A {");
    assert!(msg.contains("Parse error"));
}

#[test]
fn parse_error_missing_body() {
    let msg = parse_err("task t()");
    assert!(msg.contains("Parse error"));
}
