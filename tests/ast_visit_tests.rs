use keel_lang::ast::visit::{self, Visitor};
use keel_lang::ast::*;
use keel_lang::lexer::{self, Span};
use keel_lang::parser;
use miette::NamedSource;

fn parse_ok(source: &str) -> Program {
    let named = NamedSource::new("ast_visit.keel", source.to_string());
    let tokens = lexer::lex(source, &named).expect("lexer failed");
    parser::parse(tokens, source.len(), &named).expect("parser failed")
}

#[derive(Default)]
struct Counts {
    decls: usize,
    agent_items: usize,
    attribute_bodies: usize,
    blocks: usize,
    stmts: usize,
    exprs: usize,
    patterns: usize,
    type_exprs: usize,
    saw_tool_condition: bool,
    saw_string_interpolation: bool,
    saw_self_ref: bool,
    saw_self_access: bool,
    saw_null_field_access: bool,
    saw_null_assert: bool,
    saw_struct_lit: bool,
    saw_set_lit: bool,
    saw_tuple_lit: bool,
    saw_pipeline: bool,
    saw_range: bool,
    saw_cast: bool,
    saw_if_expr: bool,
    saw_lambda_expr: bool,
    saw_duration: bool,
    saw_enum_variant: bool,
}

impl Visitor for Counts {
    fn visit_decl(&mut self, decl: &Decl, span: &Span) {
        self.decls += 1;
        visit::walk_decl(self, decl, span);
    }

    fn visit_agent_item(&mut self, item: &AgentItem) {
        self.agent_items += 1;
        visit::walk_agent_item(self, item);
    }

    fn visit_attribute_body(&mut self, body: &AttributeBody) {
        self.attribute_bodies += 1;
        if let AttributeBody::Tools(entries) = body {
            self.saw_tool_condition = entries.iter().any(|entry| entry.condition.is_some());
        }
        visit::walk_attribute_body(self, body);
    }

    fn visit_block(&mut self, block: &Block) {
        self.blocks += 1;
        visit::walk_block(self, block);
    }

    fn visit_stmt(&mut self, stmt: &Stmt, span: &Span) {
        self.stmts += 1;
        visit::walk_stmt(self, stmt, span);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        self.exprs += 1;
        match expr {
            Expr::StringLit(parts) => {
                self.saw_string_interpolation |= parts
                    .iter()
                    .any(|part| matches!(part, StringPart::Interpolation(..)));
            }
            Expr::NullFieldAccess(_, _) => self.saw_null_field_access = true,
            Expr::NullAssert(_) => self.saw_null_assert = true,
            Expr::SelfAccess(_) => self.saw_self_access = true,
            Expr::SelfRef => self.saw_self_ref = true,
            Expr::StructLit(_) => self.saw_struct_lit = true,
            Expr::SetLit(_) => self.saw_set_lit = true,
            Expr::TupleLit(_) => self.saw_tuple_lit = true,
            Expr::Pipeline(_, _) => self.saw_pipeline = true,
            Expr::Range(_, _) => self.saw_range = true,
            Expr::Cast { .. } => self.saw_cast = true,
            Expr::IfExpr { .. } => self.saw_if_expr = true,
            Expr::Lambda { .. } => self.saw_lambda_expr = true,
            Expr::Duration { .. } => self.saw_duration = true,
            Expr::EnumVariant { .. } => self.saw_enum_variant = true,
            Expr::Integer(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::None_
            | Expr::Ident(_)
            | Expr::FieldAccess(_, _)
            | Expr::ListLit(_)
            | Expr::BinaryOp { .. }
            | Expr::UnaryOp { .. }
            | Expr::NullCoalesce(_, _)
            | Expr::Call { .. }
            | Expr::MethodCall { .. }
            | Expr::WhenExpr { .. }
            | Expr::Index { .. }
            | Expr::StructSpreadUpdate { .. } => {}
        }
        visit::walk_expr(self, expr);
    }

    fn visit_pattern(&mut self, pattern: &Pattern) {
        self.patterns += 1;
        visit::walk_pattern(self, pattern);
    }

    fn visit_type_expr(&mut self, ty: &TypeExpr) {
        self.type_exprs += 1;
        visit::walk_type_expr(self, ty);
    }
}

#[test]
fn visitor_reaches_major_ast_shapes_from_parsed_program() {
    let source = r#"
use "./shared.keel"
use Helper from "./helper.keel"
use keel/slack

type Severity = low | medium | high | critical
type Action =
  | reply { to: str, tone: str }
  | archive
type Ticket { subject: str, count: int }
type MaybeText = str?
type Pair = (str, int)
type Bag = dynamic

interface Handler {
  task handle(ticket: Ticket) -> Action
}

extern task risky(input: str) -> str from "native"

task helper() -> str {
  return "helper"
}

task inspect(ticket: Ticket, guidance: str? = none) -> str {
  maybe = ticket?.subject ?? "none"
  forced = maybe!
  pair = (1, 2)
  values = [1, 2, 3]
  known = set["a", "b"]
  shaped = { name: "Ada", score: 42 }
  math = -1 + 2 * 3
  flag = not false
  piped = values |> helper
  rng = 1..3
  casted = Json.parse("{}") as dynamic
  later = 5.minutes
  action = Action.reply { to: "ops", tone: "direct" }
  f1 = x => x + 1
  f2 = () => { return "ok" }
  choice = if flag { "yes" } else { "no" }
  label = "done"
  for x in 1..3 if x > 1 {
    Io.show("{x}")
  }
  if maybe != none {
    Io.show(ticket.subject)
  } else {
    Io.show("missing")
  }
  when action {
    reply { to, tone } where tone == "direct" => {
      Io.show(to)
    }
    archive => {
      Io.show("archive")
    }
  }
  try {
    risky(maybe)
  } catch err: Error {
    Io.show(err.message)
  }
  return label
}

agent Bot {
  @role "bot"
  @limits { max_cost_per_request: 0.50, timeout: 30.seconds }
  state {
    ready: bool = false
    last: str? = none
  }
  @tools [Email.fetch, Email.send if self.ready, Io]
  @on_start {
    self.ready = true
    Agent.stop(self)
  }
  on message(msg: Ticket) {
    self.last = msg.subject
    Io.show(self.last)
  }
  task run_one(ticket: Ticket) -> str {
    return inspect(ticket)
  }
}

run(Bot)
"#;

    let program = parse_ok(source);
    let mut visitor = Counts::default();
    visitor.visit_program(&program);

    assert!(visitor.decls >= 13, "decls: {}", visitor.decls);
    assert!(
        visitor.agent_items >= 7,
        "agent_items: {}",
        visitor.agent_items
    );
    assert!(
        visitor.attribute_bodies >= 4,
        "attribute_bodies: {}",
        visitor.attribute_bodies
    );
    assert!(visitor.blocks >= 10, "blocks: {}", visitor.blocks);
    assert!(visitor.stmts >= 35, "stmts: {}", visitor.stmts);
    assert!(visitor.exprs >= 90, "exprs: {}", visitor.exprs);
    assert!(visitor.patterns >= 2, "patterns: {}", visitor.patterns);
    assert!(
        visitor.type_exprs >= 25,
        "type_exprs: {}",
        visitor.type_exprs
    );
    assert!(visitor.saw_tool_condition);
    assert!(visitor.saw_string_interpolation);
    assert!(visitor.saw_self_ref);
    assert!(visitor.saw_self_access);
    assert!(visitor.saw_null_field_access);
    assert!(visitor.saw_null_assert);
    assert!(visitor.saw_struct_lit);
    assert!(visitor.saw_set_lit);
    assert!(visitor.saw_tuple_lit);
    assert!(visitor.saw_pipeline);
    assert!(visitor.saw_range);
    assert!(visitor.saw_cast);
    assert!(visitor.saw_if_expr);
    assert!(visitor.saw_lambda_expr);
    assert!(visitor.saw_duration);
    assert!(visitor.saw_enum_variant);
}

#[test]
fn visitor_walks_function_and_generic_type_expressions() {
    let ty = TypeExpr::Func(
        vec![
            TypeExpr::Generic(
                "Result".to_string(),
                vec![TypeExpr::Named("str".to_string()), TypeExpr::Dynamic],
            ),
            TypeExpr::Map(
                Box::new(TypeExpr::Named("str".to_string())),
                Box::new(TypeExpr::Set(Box::new(TypeExpr::Named("int".to_string())))),
            ),
        ],
        Box::new(TypeExpr::List(Box::new(TypeExpr::Named(
            "bool".to_string(),
        )))),
    );

    let mut visitor = Counts::default();
    visitor.visit_type_expr(&ty);

    assert_eq!(visitor.type_exprs, 10);
}
