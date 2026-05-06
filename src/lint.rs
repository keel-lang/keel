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

use crate::ast::*;
use crate::lexer::Span;

// ---------------------------------------------------------------------------
// Warning shape
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct LintWarning {
    pub message: String,
    pub span: Option<Span>,
    pub hint: Option<String>,
    /// Whether `keel lint --fix` can automatically remove this warning's source.
    pub fixable: bool,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

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
        for (decl, _) in &program.declarations {
            match decl {
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
                Decl::Stmt((stmt, span)) => {
                    // Rule 3: top-level Ai.* calls
                    for method in ai_methods_in_stmt(stmt) {
                        self.warn(
                            format!("`Ai.{method}` called outside an agent — no `@role` or `@model` context"),
                            Some(span.clone()),
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
        for (stmt, span) in block {
            if let Stmt::Let { binding, .. } = stmt {
                match binding {
                    Binding::Ident(name) => {
                        defined.push((name.clone(), span.clone(), true));
                    }
                    Binding::Destruct(DestructPat::Struct(fields)) => {
                        for (_, local) in fields {
                            defined.push((local.clone(), span.clone(), false));
                        }
                    }
                    Binding::Destruct(DestructPat::Tuple(names)) => {
                        for name in names {
                            defined.push((name.clone(), span.clone(), false));
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
        for (stmt, _) in block {
            match stmt {
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
        for (stmt, span) in block {
            for method in ai_methods_in_stmt(stmt) {
                self.warn(
                    format!(
                        "`Ai.{method}` called outside an agent — no `@role` or `@model` context"
                    ),
                    Some(span.clone()),
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
                    if let AttributeBody::Block(block) = &attr.body {
                        self_accesses(block, &mut reads, &mut written);
                    }
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
    for (decl, span) in &program.declarations {
        match decl {
            Decl::Task(t) => out.push((t.name.clone(), span.clone())),
            Decl::Agent(a) => {
                for item in &a.items {
                    if let AgentItem::Task(t) = item {
                        // Use the agent declaration's span as a proxy since
                        // AgentItem doesn't carry its own span.
                        out.push((t.name.clone(), span.clone()));
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
    for (decl, _) in &program.declarations {
        match decl {
            Decl::Task(t) => ident_reads_in_block_acc(&t.body, &mut reads),
            Decl::Agent(a) => {
                for item in &a.items {
                    match item {
                        AgentItem::Task(t) => ident_reads_in_block_acc(&t.body, &mut reads),
                        AgentItem::On(h) => ident_reads_in_block_acc(&h.body, &mut reads),
                        AgentItem::Attribute(attr) => match &attr.body {
                            AttributeBody::Block(b) => ident_reads_in_block_acc(b, &mut reads),
                            AttributeBody::Expr(e) => ident_reads_in_expr(e, &mut reads),
                        },
                        AgentItem::State(_) => {}
                    }
                }
            }
            Decl::Stmt((stmt, _)) => ident_reads_in_stmt(stmt, &mut reads),
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
    ident_reads_in_block_acc(block, &mut out);
    out
}

fn ident_reads_in_block_acc(block: &Block, out: &mut HashSet<String>) {
    for (stmt, _) in block {
        ident_reads_in_stmt(stmt, out);
    }
}

fn ident_reads_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
    match stmt {
        Stmt::Let { value, .. } => ident_reads_in_expr(value, out),
        Stmt::SelfAssign { value, .. } => ident_reads_in_expr(value, out),
        Stmt::Return(Some(e)) => ident_reads_in_expr(e, out),
        Stmt::Return(None) => {}
        Stmt::For {
            iter, filter, body, ..
        } => {
            ident_reads_in_expr(iter, out);
            if let Some(f) = filter {
                ident_reads_in_expr(f, out);
            }
            ident_reads_in_block_acc(body, out);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            ident_reads_in_expr(cond, out);
            ident_reads_in_block_acc(then_body, out);
            if let Some(eb) = else_body {
                ident_reads_in_block_acc(eb, out);
            }
        }
        Stmt::When { subject, arms } => {
            ident_reads_in_expr(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    ident_reads_in_expr(g, out);
                }
                ident_reads_in_block_acc(&arm.body, out);
            }
        }
        Stmt::TryCatch { body, catches } => {
            ident_reads_in_block_acc(body, out);
            for c in catches {
                ident_reads_in_block_acc(&c.body, out);
            }
        }
        Stmt::Expr(e) => ident_reads_in_expr(e, out),
    }
}

fn ident_reads_in_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Ident(name) => {
            out.insert(name.clone());
        }
        Expr::StringLit(parts) => {
            for p in parts {
                if let StringPart::Interpolation(e) = p {
                    ident_reads_in_expr(e, out);
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            ident_reads_in_expr(left, out);
            ident_reads_in_expr(right, out);
        }
        Expr::UnaryOp { expr: inner, .. } => ident_reads_in_expr(inner, out),
        Expr::NullCoalesce(l, r) | Expr::Pipeline(l, r) | Expr::Range(l, r) => {
            ident_reads_in_expr(l, out);
            ident_reads_in_expr(r, out);
        }
        Expr::NullAssert(e) => ident_reads_in_expr(e, out),
        Expr::Cast { expr: e, .. } => ident_reads_in_expr(e, out),
        Expr::Call { callee, args } => {
            ident_reads_in_expr(callee, out);
            for a in args {
                ident_reads_in_expr(&a.value, out);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            ident_reads_in_expr(object, out);
            for a in args {
                ident_reads_in_expr(&a.value, out);
            }
        }
        Expr::FieldAccess(obj, _) | Expr::NullFieldAccess(obj, _) => {
            ident_reads_in_expr(obj, out);
        }
        Expr::StructLit(fields) => {
            for (_, v) in fields {
                ident_reads_in_expr(v, out);
            }
        }
        Expr::ListLit(items) | Expr::SetLit(items) | Expr::TupleLit(items) => {
            for e in items {
                ident_reads_in_expr(e, out);
            }
        }
        Expr::IfExpr {
            cond,
            then_body,
            else_body,
        } => {
            ident_reads_in_expr(cond, out);
            ident_reads_in_block_acc(then_body, out);
            ident_reads_in_block_acc(else_body, out);
        }
        Expr::WhenExpr { subject, arms } => {
            ident_reads_in_expr(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    ident_reads_in_expr(g, out);
                }
                ident_reads_in_block_acc(&arm.body, out);
            }
        }
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => ident_reads_in_expr(e, out),
            LambdaBody::Block(b) => ident_reads_in_block_acc(b, out),
        },
        Expr::Duration { value, .. } => ident_reads_in_expr(value, out),
        Expr::EnumVariant { fields, .. } => {
            for (_, v) in fields {
                ident_reads_in_expr(v, out);
            }
        }
        // Leaf nodes — no sub-expressions to walk
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::None_
        | Expr::SelfAccess(_)
        | Expr::SelfRef => {}
    }
}

// ---------------------------------------------------------------------------
// Ai.* method call detector (for Rule 3)
// ---------------------------------------------------------------------------

fn ai_methods_in_stmt(stmt: &Stmt) -> Vec<String> {
    let mut out = Vec::new();
    ai_methods_in_stmt_acc(stmt, &mut out);
    out
}

fn ai_methods_in_stmt_acc(stmt: &Stmt, out: &mut Vec<String>) {
    match stmt {
        Stmt::Let { value, .. } | Stmt::SelfAssign { value, .. } | Stmt::Expr(value) => {
            ai_methods_in_expr(value, out);
        }
        Stmt::Return(Some(e)) => ai_methods_in_expr(e, out),
        Stmt::Return(None) => {}
        Stmt::For {
            iter, filter, body, ..
        } => {
            ai_methods_in_expr(iter, out);
            if let Some(f) = filter {
                ai_methods_in_expr(f, out);
            }
            for (s, _) in body {
                ai_methods_in_stmt_acc(s, out);
            }
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            ai_methods_in_expr(cond, out);
            for (s, _) in then_body {
                ai_methods_in_stmt_acc(s, out);
            }
            if let Some(eb) = else_body {
                for (s, _) in eb {
                    ai_methods_in_stmt_acc(s, out);
                }
            }
        }
        Stmt::When { subject, arms } => {
            ai_methods_in_expr(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    ai_methods_in_expr(g, out);
                }
                for (s, _) in &arm.body {
                    ai_methods_in_stmt_acc(s, out);
                }
            }
        }
        Stmt::TryCatch { body, catches } => {
            for (s, _) in body {
                ai_methods_in_stmt_acc(s, out);
            }
            for c in catches {
                for (s, _) in &c.body {
                    ai_methods_in_stmt_acc(s, out);
                }
            }
        }
    }
}

fn ai_methods_in_expr(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            if let Expr::Ident(name) = object.as_ref()
                && name == "Ai"
            {
                out.push(method.clone());
            }
            ai_methods_in_expr(object, out);
            for a in args {
                ai_methods_in_expr(&a.value, out);
            }
        }
        Expr::Call { callee, args } => {
            ai_methods_in_expr(callee, out);
            for a in args {
                ai_methods_in_expr(&a.value, out);
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            ai_methods_in_expr(left, out);
            ai_methods_in_expr(right, out);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::NullAssert(inner)
        | Expr::Cast { expr: inner, .. } => {
            ai_methods_in_expr(inner, out);
        }
        Expr::NullCoalesce(l, r) | Expr::Pipeline(l, r) | Expr::Range(l, r) => {
            ai_methods_in_expr(l, out);
            ai_methods_in_expr(r, out);
        }
        Expr::FieldAccess(obj, _) | Expr::NullFieldAccess(obj, _) => {
            ai_methods_in_expr(obj, out);
        }
        Expr::StructLit(fields) => {
            for (_, v) in fields {
                ai_methods_in_expr(v, out);
            }
        }
        Expr::ListLit(items) | Expr::SetLit(items) | Expr::TupleLit(items) => {
            for e in items {
                ai_methods_in_expr(e, out);
            }
        }
        Expr::IfExpr {
            cond,
            then_body,
            else_body,
        } => {
            ai_methods_in_expr(cond, out);
            for (s, _) in then_body {
                ai_methods_in_stmt_acc(s, out);
            }
            for (s, _) in else_body {
                ai_methods_in_stmt_acc(s, out);
            }
        }
        Expr::WhenExpr { subject, arms } => {
            ai_methods_in_expr(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    ai_methods_in_expr(g, out);
                }
                for (s, _) in &arm.body {
                    ai_methods_in_stmt_acc(s, out);
                }
            }
        }
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => ai_methods_in_expr(e, out),
            LambdaBody::Block(b) => {
                for (s, _) in b {
                    ai_methods_in_stmt_acc(s, out);
                }
            }
        },
        Expr::Duration { value, .. } => ai_methods_in_expr(value, out),
        Expr::EnumVariant { fields, .. } => {
            for (_, v) in fields {
                ai_methods_in_expr(v, out);
            }
        }
        Expr::StringLit(parts) => {
            for p in parts {
                if let StringPart::Interpolation(e) = p {
                    ai_methods_in_expr(e, out);
                }
            }
        }
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::None_
        | Expr::Ident(_)
        | Expr::SelfAccess(_)
        | Expr::SelfRef => {}
    }
}

// ---------------------------------------------------------------------------
// Self-field access tracker (for Rule 4)
// ---------------------------------------------------------------------------

fn self_accesses(block: &Block, reads: &mut HashSet<String>, written: &mut HashSet<String>) {
    for (stmt, _) in block {
        self_in_stmt(stmt, reads, written);
    }
}

fn self_in_stmt(stmt: &Stmt, reads: &mut HashSet<String>, written: &mut HashSet<String>) {
    match stmt {
        Stmt::SelfAssign { field, value } => {
            written.insert(field.clone());
            self_in_expr(value, reads, written);
        }
        Stmt::Let { value, .. } | Stmt::Expr(value) => {
            self_in_expr(value, reads, written);
        }
        Stmt::Return(Some(e)) => self_in_expr(e, reads, written),
        Stmt::Return(None) => {}
        Stmt::For {
            iter, filter, body, ..
        } => {
            self_in_expr(iter, reads, written);
            if let Some(f) = filter {
                self_in_expr(f, reads, written);
            }
            self_accesses(body, reads, written);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            self_in_expr(cond, reads, written);
            self_accesses(then_body, reads, written);
            if let Some(eb) = else_body {
                self_accesses(eb, reads, written);
            }
        }
        Stmt::When { subject, arms } => {
            self_in_expr(subject, reads, written);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    self_in_expr(g, reads, written);
                }
                self_accesses(&arm.body, reads, written);
            }
        }
        Stmt::TryCatch { body, catches } => {
            self_accesses(body, reads, written);
            for c in catches {
                self_accesses(&c.body, reads, written);
            }
        }
    }
}

fn self_in_expr(expr: &Expr, reads: &mut HashSet<String>, written: &mut HashSet<String>) {
    match expr {
        Expr::SelfAccess(field) => {
            reads.insert(field.clone());
        }
        Expr::BinaryOp { left, right, .. } => {
            self_in_expr(left, reads, written);
            self_in_expr(right, reads, written);
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::NullAssert(inner)
        | Expr::Cast { expr: inner, .. } => {
            self_in_expr(inner, reads, written);
        }
        Expr::NullCoalesce(l, r) | Expr::Pipeline(l, r) | Expr::Range(l, r) => {
            self_in_expr(l, reads, written);
            self_in_expr(r, reads, written);
        }
        Expr::Call { callee, args } => {
            self_in_expr(callee, reads, written);
            for a in args {
                self_in_expr(&a.value, reads, written);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            self_in_expr(object, reads, written);
            for a in args {
                self_in_expr(&a.value, reads, written);
            }
        }
        Expr::FieldAccess(obj, _) | Expr::NullFieldAccess(obj, _) => {
            self_in_expr(obj, reads, written);
        }
        Expr::StructLit(fields) => {
            for (_, v) in fields {
                self_in_expr(v, reads, written);
            }
        }
        Expr::ListLit(items) | Expr::SetLit(items) | Expr::TupleLit(items) => {
            for e in items {
                self_in_expr(e, reads, written);
            }
        }
        Expr::IfExpr {
            cond,
            then_body,
            else_body,
        } => {
            self_in_expr(cond, reads, written);
            self_accesses(then_body, reads, written);
            self_accesses(else_body, reads, written);
        }
        Expr::WhenExpr { subject, arms } => {
            self_in_expr(subject, reads, written);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    self_in_expr(g, reads, written);
                }
                self_accesses(&arm.body, reads, written);
            }
        }
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(e) => self_in_expr(e, reads, written),
            LambdaBody::Block(b) => self_accesses(b, reads, written),
        },
        Expr::Duration { value, .. } => self_in_expr(value, reads, written),
        Expr::EnumVariant { fields, .. } => {
            for (_, v) in fields {
                self_in_expr(v, reads, written);
            }
        }
        Expr::StringLit(parts) => {
            for p in parts {
                if let StringPart::Interpolation(e) = p {
                    self_in_expr(e, reads, written);
                }
            }
        }
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Bool(_)
        | Expr::None_
        | Expr::Ident(_)
        | Expr::SelfRef => {}
    }
}
