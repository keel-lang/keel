//! Linter for Keel programs.
//!
//! Emits `LintWarning`s for style and best-practice issues beyond what the
//! type checker enforces. Lint warnings do not block compilation or execution.
//!
//! Rules implemented:
//!   1. Unused `let` bindings — variable bound but never read.
//!   2. Declared tasks never called — top-level or agent tasks with no invocation site.
//!   3. `Ai.*` calls outside an agent — no `@role`/`@model` context available.
//!   4. Agent state fields written but never read across any handler or task.
//!
//! Known limitations:
//!   - Rule 1 does not inspect lambda bodies (only explicit if/for/when/try blocks).
//!   - Rule 2 does not detect tasks called only via `Agent.delegate` string args.
//!   - Rule 4 only checks write-never-read; read-before-write requires data-flow analysis.

use std::collections::HashSet;

use crate::ast::visit::{self, Visitor};
use crate::ast::*;
use crate::diagnostics::LintWarning;
use crate::lexer::Span;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[must_use]
pub fn lint(program: &Program) -> Vec<LintWarning> {
    let mut l = Linter::new();
    l.run(program);
    l.warnings
}

// ---------------------------------------------------------------------------
// Linter state
// ---------------------------------------------------------------------------

struct Linter {
    warnings: Vec<LintWarning>,
}

impl Linter {
    fn new() -> Self {
        Linter {
            warnings: Vec::new(),
        }
    }

    fn warn(
        &mut self,
        msg: impl Into<String>,
        span: Option<Span>,
        hint: Option<String>,
        fixable: bool,
    ) {
        self.warnings.push(LintWarning {
            message: msg.into(),
            span,
            hint,
            fixable,
        });
    }

    fn run(&mut self, program: &Program) {
        // Rule 2: tasks declared but never called
        let declared = declared_tasks(program);
        let used = all_ident_reads(program);
        for (name, span) in &declared {
            if !used.contains(name.as_str()) {
                self.warn(
                    format!("task `{name}` is declared but never called"),
                    Some(span.clone()),
                    Some("remove this task or call it somewhere in the program".into()),
                    false,
                );
            }
        }

        // Rules 1, 3, 4 — walk declarations
        for node in &program.declarations {
            match &node.kind {
                Decl::Task(t) => {
                    self.check_block_unused(&t.body);
                    self.check_block_ai_outside_agent(&t.body);
                }
                Decl::Agent(a) => {
                    // Rule 4
                    self.check_agent_state(a);
                    for item in &a.items {
                        match item {
                            // Rule 1 inside agent tasks and handlers
                            AgentItem::Task(t) => self.check_block_unused(&t.body),
                            AgentItem::On(h) => self.check_block_unused(&h.body),
                            AgentItem::Attribute(attr) => {
                                if let AttributeBody::Block(block) = &attr.body {
                                    self.check_block_unused(block);
                                }
                            }
                            AgentItem::State(_) => {}
                        }
                    }
                }
                Decl::Stmt(stmt_node) => {
                    // Rule 3: top-level Ai.* calls
                    for method in ai_methods_in_stmt(&stmt_node.kind) {
                        self.warn(
                            format!("`Ai.{method}` called outside an agent — no `@role` or `@model` context"),
                            Some(stmt_node.span.clone()),
                            Some("wrap in an agent body with `@role` and `@model` attributes".into()),
                            false,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Rule 1: unused let bindings
    // -----------------------------------------------------------------------

    fn check_block_unused(&mut self, block: &Block) {
        // Collect let-bindings defined at this block level only (not in nested blocks).
        // Each entry is (name, span, fixable). Destructure bindings are not auto-fixable.
        let mut defined: Vec<(String, Span, bool)> = Vec::new();
        for s_node in block {
            if let Stmt::Let { binding, .. } = &s_node.kind {
                match binding {
                    Binding::Ident(name) => {
                        defined.push((name.clone(), s_node.span.clone(), true));
                    }
                    Binding::Destruct(DestructPat::Struct(fields)) => {
                        for (_, local) in fields {
                            defined.push((local.clone(), s_node.span.clone(), false));
                        }
                    }
                    Binding::Destruct(DestructPat::Tuple(names)) => {
                        for name in names {
                            defined.push((name.clone(), s_node.span.clone(), false));
                        }
                    }
                }
            }
        }

        if !defined.is_empty() {
            let reads = ident_reads_in_block(block);
            for (name, span, fixable) in &defined {
                // Names prefixed with `_` are intentionally unused by convention.
                if name.starts_with('_') {
                    continue;
                }
                if !reads.contains(name.as_str()) {
                    self.warn(
                        format!("unused variable `{name}`"),
                        Some(span.clone()),
                        Some(
                            "remove this binding, use its value, or prefix with `_` to silence"
                                .into(),
                        ),
                        *fixable,
                    );
                }
            }
        }

        // Recurse into nested control-flow blocks.
        for s_node in block {
            match &s_node.kind {
                Stmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.check_block_unused(then_body);
                    if let Some(eb) = else_body {
                        self.check_block_unused(eb);
                    }
                }
                Stmt::For { body, .. } => self.check_block_unused(body),
                Stmt::While { body, .. } => self.check_block_unused(body),
                Stmt::When { arms, .. } => {
                    for arm in arms {
                        self.check_block_unused(&arm.body);
                    }
                }
                Stmt::TryCatch { body, catches } => {
                    self.check_block_unused(body);
                    for c in catches {
                        self.check_block_unused(&c.body);
                    }
                }
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Rule 3: Ai.* calls outside an agent
    // -----------------------------------------------------------------------

    fn check_block_ai_outside_agent(&mut self, block: &Block) {
        for s_node in block {
            for method in ai_methods_in_stmt(&s_node.kind) {
                self.warn(
                    format!(
                        "`Ai.{method}` called outside an agent — no `@role` or `@model` context"
                    ),
                    Some(s_node.span.clone()),
                    Some("wrap in an agent body with `@role` and `@model` attributes".into()),
                    false,
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Rule 4: agent state fields written but never read
    // -----------------------------------------------------------------------

    fn check_agent_state(&mut self, agent: &AgentDecl) {
        let mut state_fields: Vec<String> = Vec::new();
        for item in &agent.items {
            if let AgentItem::State(fields) = item {
                for f in fields {
                    state_fields.push(f.name.clone());
                }
            }
        }
        if state_fields.is_empty() {
            return;
        }

        let mut reads: HashSet<String> = HashSet::new();
        let mut written: HashSet<String> = HashSet::new();

        for item in &agent.items {
            match item {
                AgentItem::Task(t) => self_accesses(&t.body, &mut reads, &mut written),
                AgentItem::On(h) => self_accesses(&h.body, &mut reads, &mut written),
                AgentItem::Attribute(attr) => {
                    self_accesses_in_attribute(&attr.body, &mut reads, &mut written);
                }
                AgentItem::State(_) => {}
            }
        }

        for field in &state_fields {
            if written.contains(field) && !reads.contains(field) {
                self.warn(
                    format!(
                        "state field `self.{field}` in agent `{}` is written but never read",
                        agent.name
                    ),
                    None,
                    Some(format!(
                        "either read `self.{field}` somewhere or remove this field from state"
                    )),
                    false,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Collect declared task names with spans (for Rule 2)
// ---------------------------------------------------------------------------

fn declared_tasks(program: &Program) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    for node in &program.declarations {
        match &node.kind {
            Decl::Task(t) => out.push((t.name.clone(), node.span.clone())),
            Decl::Agent(a) => {
                for item in &a.items {
                    if let AgentItem::Task(t) = item {
                        // Use the agent declaration's span as a proxy since
                        // AgentItem doesn't carry its own span.
                        out.push((t.name.clone(), node.span.clone()));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Collect all Ident reads across the entire program (for Rule 2)
// ---------------------------------------------------------------------------

fn all_ident_reads(program: &Program) -> HashSet<String> {
    let mut reads = HashSet::new();
    for node in &program.declarations {
        match &node.kind {
            Decl::Task(t) => collect_ident_reads_in_block(&t.body, &mut reads),
            Decl::Agent(a) => {
                for item in &a.items {
                    match item {
                        AgentItem::Task(t) => collect_ident_reads_in_block(&t.body, &mut reads),
                        AgentItem::On(h) => collect_ident_reads_in_block(&h.body, &mut reads),
                        AgentItem::Attribute(attr) => match &attr.body {
                            AttributeBody::Block(b) => collect_ident_reads_in_block(b, &mut reads),
                            AttributeBody::Expr(e) => collect_ident_reads_in_expr(e, &mut reads),
                            AttributeBody::Tools(entries) => {
                                for entry in entries {
                                    if let Some(cond) = &entry.condition {
                                        collect_ident_reads_in_expr(cond, &mut reads);
                                    }
                                }
                            }
                        },
                        AgentItem::State(_) => {}
                    }
                }
            }
            Decl::Stmt(stmt_node) => {
                collect_ident_reads_in_stmt(&stmt_node.kind, &stmt_node.span, &mut reads)
            }
            _ => {}
        }
    }
    reads
}

// ---------------------------------------------------------------------------
// Ident reads walker — returns owned set (used by Rule 1 per-block check)
// ---------------------------------------------------------------------------

fn ident_reads_in_block(block: &Block) -> HashSet<String> {
    let mut out = HashSet::new();
    collect_ident_reads_in_block(block, &mut out);
    out
}

fn collect_ident_reads_in_block(block: &Block, out: &mut HashSet<String>) {
    let mut visitor = IdentReads { reads: out };
    visitor.visit_block(block);
}

fn collect_ident_reads_in_stmt(stmt: &Stmt, span: &Span, out: &mut HashSet<String>) {
    let mut visitor = IdentReads { reads: out };
    visitor.visit_stmt(stmt, span);
}

fn collect_ident_reads_in_expr(expr: &SpannedExpr, out: &mut HashSet<String>) {
    let mut visitor = IdentReads { reads: out };
    visitor.visit_expr(expr);
}

struct IdentReads<'a> {
    reads: &'a mut HashSet<String>,
}

impl Visitor for IdentReads<'_> {
    fn visit_expr(&mut self, spanned: &SpannedExpr) {
        let expr = &spanned.kind;
        if let Expr::Ident(name) = expr {
            self.reads.insert(name.clone());
        }
        if let Expr::MethodCall { object, method, .. } = expr
            && matches!(&object.as_ref().kind, Expr::SelfRef)
        {
            self.reads.insert(method.clone());
        }
        if let Expr::Call { callee, .. } = expr
            && let Expr::SelfAccess { field: method, .. } = &callee.as_ref().kind
        {
            self.reads.insert(method.clone());
        }
        visit::walk_expr(self, spanned);
    }
}

// ---------------------------------------------------------------------------
// Ai.* method call detector (for Rule 3)
// ---------------------------------------------------------------------------

fn ai_methods_in_stmt(stmt: &Stmt) -> Vec<String> {
    let mut visitor = AiCalls {
        methods: Vec::new(),
    };
    visitor.visit_stmt(stmt, &(0..0));
    visitor.methods
}

struct AiCalls {
    methods: Vec<String>,
}

impl Visitor for AiCalls {
    fn visit_expr(&mut self, spanned: &SpannedExpr) {
        let expr = &spanned.kind;
        if let Expr::MethodCall { object, method, .. } = expr
            && let Expr::Ident(name) = &object.as_ref().kind
            && name == "Ai"
        {
            self.methods.push(method.clone());
        }
        visit::walk_expr(self, spanned);
    }
}

// ---------------------------------------------------------------------------
// Self-field access tracker (for Rule 4)
// ---------------------------------------------------------------------------

fn self_accesses(block: &Block, reads: &mut HashSet<String>, written: &mut HashSet<String>) {
    let mut visitor = SelfAccesses { reads, written };
    visitor.visit_block(block);
}

fn self_accesses_in_attribute(
    body: &AttributeBody,
    reads: &mut HashSet<String>,
    written: &mut HashSet<String>,
) {
    let mut visitor = SelfAccesses { reads, written };
    visitor.visit_attribute_body(body);
}

struct SelfAccesses<'a> {
    reads: &'a mut HashSet<String>,
    written: &'a mut HashSet<String>,
}

impl Visitor for SelfAccesses<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt, span: &Span) {
        if let Stmt::SelfAssign { field, .. } = stmt {
            self.written.insert(field.clone());
        }
        visit::walk_stmt(self, stmt, span);
    }

    fn visit_expr(&mut self, spanned: &SpannedExpr) {
        let expr = &spanned.kind;
        if let Expr::SelfAccess { field, .. } = expr {
            self.reads.insert(field.clone());
        }
        visit::walk_expr(self, spanned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};
    use miette::NamedSource;

    fn warnings_for(source: &str) -> Vec<String> {
        let named = NamedSource::new("t.keel", source.to_string());
        let tokens = lexer::lex(source, &named).expect("lex failed");
        let program = parser::parse(tokens, source.len(), &named).expect("parse failed");
        lint(&program).into_iter().map(|w| w.message).collect()
    }

    fn assert_clean(source: &str) {
        let w = warnings_for(source);
        assert!(w.is_empty(), "unexpected warnings: {w:?}");
    }

    fn assert_warns(source: &str, needle: &str) {
        let w = warnings_for(source);
        assert!(
            w.iter().any(|m| m.contains(needle)),
            "expected warning containing {needle:?}, got: {w:?}"
        );
    }

    #[test]
    fn clean_agent_no_warnings() {
        assert_clean(
            r#"
agent Greeter {
  @role "greeter"
  @on_start {
    Io.show("hi")
    stop(self)
  }
}
run(Greeter)
"#,
        );
    }

    #[test]
    fn unused_let_binding_warns() {
        assert_warns(
            r#"
task greet() {
  x = 42
  Io.show("done")
}
greet()
"#,
            "unused variable `x`",
        );
    }

    #[test]
    fn used_let_binding_no_warning() {
        assert_clean(
            r#"
task greet() {
  x = 42
  Io.show(x)
}
greet()
"#,
        );
    }

    #[test]
    fn undeclared_task_warns() {
        assert_warns(
            r#"
agent Bot {
  @role "bot"
  task unused_task() {
    Io.show("never called")
  }
  @on_start {
    stop(self)
  }
}
run(Bot)
"#,
            "task `unused_task` is declared but never called",
        );
    }

    #[test]
    fn task_call_inside_interpolation_counts_as_read() {
        assert_clean(
            r#"
task helper() -> str {
  "ok"
}

task main() {
  msg = "value: {helper()}"
  Io.show(msg)
}

main()
"#,
        );
    }

    #[test]
    fn self_task_call_counts_as_read() {
        assert_clean(
            r#"
agent Bot {
  @role "bot"
  task helper() {
    Io.show("ok")
  }
  @on_start {
    self.helper()
    stop(self)
  }
}
run(Bot)
"#,
        );
    }

    #[test]
    fn ai_call_inside_lambda_outside_agent_warns() {
        assert_warns(
            r#"
task main() {
  f = () => Ai.prompt("hello")
  f()
}

main()
"#,
            "`Ai.prompt` called outside an agent",
        );
    }

    #[test]
    fn ai_call_inside_string_interpolation_outside_agent_warns() {
        assert_warns(
            r#"
task main() {
  msg = "answer: {Ai.prompt("hello")}"
  Io.show(msg)
}

main()
"#,
            "`Ai.prompt` called outside an agent",
        );
    }

    #[test]
    fn state_read_inside_string_interpolation_counts_as_read() {
        assert_clean(
            r#"
agent Bot {
  @role "bot"
  state { ready: bool = false }
  @on_start {
    self.ready = true
    Io.show("ready: {self.ready}")
    stop(self)
  }
}

run(Bot)
"#,
        );
    }

    #[test]
    fn state_read_inside_tool_guard_counts_as_read() {
        assert_clean(
            r#"
agent Bot {
  @role "bot"
  state { confirmed: bool = false }
  @tools [Email.send if self.confirmed]
  @on_start {
    self.confirmed = true
    stop(self)
  }
}

run(Bot)
"#,
        );
    }

    #[test]
    fn nested_state_write_without_read_warns() {
        assert_warns(
            r#"
agent Bot {
  @role "bot"
  state { ready: bool = false }
  @on_start {
    if true {
      self.ready = true
    }
    stop(self)
  }
}

run(Bot)
"#,
            "state field `self.ready`",
        );
    }
}
