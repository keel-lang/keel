//! Statement-level type checking for the validation pass.
//!
//! Implements the second-pass check that walks task and agent bodies,
//! validates statement types (let, for, if, when, while, return, …),
//! and dispatches to expression inference for embedded expressions.

use std::collections::HashSet;

use crate::ast::*;
use crate::lexer::Span;
use crate::types::scope::Scope;
use crate::types::ty::{describe_ty, Ty};

use super::{binop::check_binop, Checker};

impl Checker {
    /// Second-pass validation: walk all task, agent, and top-level statement
    /// declarations and type-check their bodies.
    ///
    /// Must be called after [`Checker::collect`] so that all type and task
    /// signatures are already registered.
    pub(crate) fn check_body(&mut self, program: &Program) {
        for (decl, _) in &program.declarations {
            match decl {
                Decl::Task(t) => {
                    self.current_agent = None;
                    self.check_task(t);
                }
                Decl::Agent(a) => {
                    self.current_agent = Some(a.name.clone());
                    for item in &a.items {
                        match item {
                            AgentItem::Task(t) => self.check_task(t),
                            AgentItem::On(h) => self.check_on_handler(h),
                            AgentItem::Attribute(attr) => self.check_attribute(attr),
                            AgentItem::State(_) => {}
                        }
                    }
                    self.current_agent = None;
                }
                Decl::Stmt((stmt, span)) => {
                    let mut scope = Scope::new();
                    self.check_stmt(stmt, span.clone(), &mut scope);
                }
                _ => {}
            }
        }
    }

    /// New lexical scope for a task, handler, or attribute body.
    /// Agent-owned tasks are explicit `self.task(...)` calls rather than
    /// bare names injected into lexical scope.
    fn fresh_scope(&self) -> Scope {
        Scope::new()
    }

    /// Bind `binding` to `ty` in `scope`, expanding destructure patterns field by field.
    fn bind_to_scope(&mut self, binding: &Binding, ty: &Ty, scope: &mut Scope) {
        match binding {
            Binding::Ident(name) => {
                scope.define(name.clone(), ty.clone());
            }
            Binding::Destruct(DestructPat::Struct(fields)) => {
                let struct_fields: Vec<(String, Ty)> = match ty.strip_nullable() {
                    Ty::Struct(f) => f.clone(),
                    Ty::Unknown | Ty::Dynamic => {
                        for (_, local) in fields {
                            scope.define(local.clone(), Ty::Unknown);
                        }
                        return;
                    }
                    other => {
                        self.err(format!(
                            "cannot destructure {} as a struct",
                            describe_ty(other)
                        ));
                        for (_, local) in fields {
                            scope.define(local.clone(), Ty::Unknown);
                        }
                        return;
                    }
                };
                for (source, local) in fields {
                    let field_ty = struct_fields
                        .iter()
                        .find(|(n, _)| n == source)
                        .map(|(_, t)| t.clone())
                        .unwrap_or_else(|| {
                            self.err(format!("field `{source}` not found in struct"));
                            Ty::Unknown
                        });
                    scope.define(local.clone(), field_ty);
                }
            }
            Binding::Destruct(DestructPat::Tuple(names)) => {
                let elem_tys: Vec<Ty> = match ty.strip_nullable() {
                    Ty::Tuple(items) => items.clone(),
                    Ty::Unknown | Ty::Dynamic => {
                        for name in names {
                            scope.define(name.clone(), Ty::Unknown);
                        }
                        return;
                    }
                    other => {
                        self.err(format!(
                            "cannot destructure {} as a tuple",
                            describe_ty(other)
                        ));
                        for name in names {
                            scope.define(name.clone(), Ty::Unknown);
                        }
                        return;
                    }
                };
                if names.len() != elem_tys.len() {
                    self.err(format!(
                        "tuple destructure expects {} element(s), got {}",
                        elem_tys.len(),
                        names.len()
                    ));
                }
                for (i, name) in names.iter().enumerate() {
                    let t = elem_tys.get(i).cloned().unwrap_or(Ty::Unknown);
                    scope.define(name.clone(), t);
                }
            }
        }
    }

    fn check_task(&mut self, t: &TaskDecl) {
        let declared_return = t
            .return_type
            .as_ref()
            .map(|ty| self.resolve_and_check_type(ty));
        let prev_return_ty = self.current_return_ty.take();
        self.current_return_ty = declared_return;

        let mut scope = self.fresh_scope();
        for p in &t.params {
            let elem_ty = self.resolve_and_check_type(&p.ty);
            // Variadic params are visible inside the body as `list[T]`.
            let param_ty = if p.variadic {
                Ty::List(Box::new(elem_ty))
            } else {
                elem_ty
            };
            self.bind_to_scope(&p.name, &param_ty, &mut scope);
        }

        // Only check implicit return when the last statement is an expression.
        // Control-flow statements (return, when, if, for, try/catch) manage
        // their own return paths; checking them here produces false positives.
        let last_is_expr = t
            .body
            .last()
            .map(|(s, _)| matches!(s, Stmt::Expr(_)))
            .unwrap_or(false);
        let implicit_ty = self.block_type(&t.body, &mut scope);
        if last_is_expr
            && let Some(expected) = &self.current_return_ty.clone()
            && !matches!(expected, Ty::None_ | Ty::Unknown | Ty::Dynamic)
        {
            self.expect(&implicit_ty, expected, "implicit return");
        }

        self.current_return_ty = prev_return_ty;
    }

    fn check_on_handler(&mut self, h: &OnHandler) {
        let mut scope = self.fresh_scope();
        if let Some(p) = &h.param {
            let param_ty = self.resolve_and_check_type(&p.ty);
            self.bind_to_scope(&p.name, &param_ty, &mut scope);
        }
        self.check_block(&h.body, &mut scope);
    }

    fn check_attribute(&mut self, attr: &AttributeDecl) {
        match &attr.body {
            AttributeBody::Block(body) => {
                let mut scope = self.fresh_scope();
                self.check_block(body, &mut scope);
            }
            AttributeBody::Expr(e) => {
                let mut scope = self.fresh_scope();
                self.infer_expr(e, &mut scope);
            }
            AttributeBody::Tools(entries) => {
                let mut scope = self.fresh_scope();
                for entry in entries {
                    if let Some(cond) = &entry.condition {
                        let ty = self.infer_expr(cond, &mut scope);
                        if !matches!(ty, Ty::Bool | Ty::Unknown) {
                            self.err(format!(
                                "`when` guard on `{}` must be a bool expression",
                                entry.namespace
                            ));
                        }
                    }
                }
            }
        }
    }

    fn check_block(&mut self, block: &Block, scope: &mut Scope) {
        scope.push();
        for (stmt, span) in block {
            self.check_stmt(stmt, span.clone(), scope);
        }
        scope.pop();
    }

    /// Type-check a single statement.
    ///
    /// Sets `self.current_span` at entry so that any errors emitted by nested
    /// expression inference automatically carry the statement's location.
    pub(crate) fn check_stmt(&mut self, stmt: &Stmt, span: Span, scope: &mut Scope) {
        self.current_span = Some(span.clone());
        match stmt {
            Stmt::Let { binding, ty, value } => {
                let inferred = self.infer_expr(value, scope);
                let bound = match ty {
                    Some(t) => {
                        let declared = self.resolve_and_check_type(t);
                        // Only check when declared type is concrete — Unknown
                        // means the checker couldn't resolve it (e.g. a named
                        // user-defined type), so a mismatch would be a false positive.
                        if let Binding::Ident(name) = binding
                            && !matches!(declared, Ty::Unknown | Ty::Dynamic)
                        {
                            self.expect(&inferred, &declared, &format!("`{name}`"));
                        }
                        declared
                    }
                    None => {
                        if self.strict
                            && matches!(inferred, Ty::Unknown | Ty::Dynamic)
                            && let Binding::Ident(name) = binding
                        {
                            self.err(format!(
                                "cannot infer type of `{name}`; consider adding a type annotation"
                            ));
                        }
                        inferred
                    }
                };
                self.bind_to_scope(binding, &bound, scope);
            }
            Stmt::SelfAssign { field, value } => {
                let Some(agent_name) = &self.current_agent.clone() else {
                    self.err(format!("`self.{field}` used outside an agent"));
                    return;
                };
                let (field_exists, is_readonly) = self
                    .agents
                    .get(agent_name)
                    .map(|a| {
                        (
                            a.state_fields.contains_key(field),
                            a.readonly_fields.contains(field),
                        )
                    })
                    .unwrap_or((false, false));
                if !field_exists {
                    self.err(format!("agent `{agent_name}` has no state field `{field}`"));
                }
                if is_readonly {
                    self.err(format!(
                        "cannot assign to `self.{field}`: field is declared readonly"
                    ));
                }
                self.infer_expr(value, scope);
            }
            Stmt::Return(opt) => {
                if let Some(e) = opt {
                    let actual = self.infer_expr(e, scope);
                    if let Some(expected) = self.current_return_ty.clone()
                        && !matches!(expected, Ty::None_ | Ty::Unknown | Ty::Dynamic)
                    {
                        self.expect(&actual, &expected, "return value");
                    }
                }
            }
            Stmt::For {
                binding,
                iter,
                filter,
                body,
            } => {
                let iter_ty = self.infer_expr(iter, scope);
                let element_ty = match iter_ty.strip_nullable() {
                    Ty::List(inner) => *inner.clone(),
                    Ty::Unknown | Ty::Dynamic => Ty::Unknown,
                    Ty::Struct(fields) => {
                        // Allow iterating over a struct that implements Iterable.
                        // Find the struct's type name by matching its field set.
                        let field_names: std::collections::HashSet<&str> =
                            fields.iter().map(|(n, _)| n.as_str()).collect();
                        let is_iterable = self.structs.iter().any(|(type_name, schema)| {
                            let schema_names: std::collections::HashSet<&str> =
                                schema.iter().map(|(n, _)| n.as_str()).collect();
                            schema_names == field_names && self.iterable_types.contains(type_name)
                        });
                        if is_iterable {
                            Ty::Unknown
                        } else {
                            self.err("`for` expects a list, got struct".to_string());
                            Ty::Unknown
                        }
                    }
                    other => {
                        self.err(format!("`for` expects a list, got {}", describe_ty(other)));
                        Ty::Unknown
                    }
                };
                scope.push();
                self.bind_to_scope(binding, &element_ty, scope);
                if let Some(pred) = filter {
                    let pty = self.infer_expr(pred, scope);
                    self.expect(&pty, &Ty::Bool, "for-if guard");
                }
                for (s, s_span) in body {
                    self.check_stmt(s, s_span.clone(), scope);
                }
                // Restore span to the for-statement after processing body
                self.current_span = Some(span.clone());
                scope.pop();
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_ty = self.infer_expr(cond, scope);
                self.expect(&cond_ty, &Ty::Bool, "`if` condition");
                self.check_block(then_body, scope);
                if let Some(eb) = else_body {
                    self.check_block(eb, scope);
                }
            }
            Stmt::When { subject, arms } => {
                let subject_ty = self.infer_expr(subject, scope);
                self.check_when_arms(&subject_ty, arms, scope, span);
            }
            Stmt::TryCatch { body, catches } => {
                self.check_block(body, scope);
                for c in catches {
                    scope.push();
                    let ty = self.resolve_type(&c.ty);
                    scope.define(c.name.clone(), ty);
                    for (s, s_span) in &c.body {
                        self.check_stmt(s, s_span.clone(), scope);
                    }
                    scope.pop();
                }
            }
            Stmt::AugAssign { name, op, rhs } => {
                let var_ty = scope.get(name).cloned().unwrap_or_else(|| {
                    self.err(format!(
                        "augmented assignment to undefined variable `{name}`"
                    ));
                    Ty::Unknown
                });
                let rhs_ty = self.infer_expr(rhs, scope);
                if let Some(msg) = check_binop(*op, &var_ty, &rhs_ty) {
                    self.err(msg);
                }
            }
            Stmt::Raise(e) => {
                self.infer_expr(e, scope);
            }
            Stmt::While { cond, body } => {
                let cond_ty = self.infer_expr(cond, scope);
                self.expect(&cond_ty, &Ty::Bool, "`while` condition");
                self.check_block(body, scope);
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Expr(e) => {
                self.infer_expr(e, scope);
            }
        }
    }

    /// Check the arms of a `when` statement or expression.
    ///
    /// Validates guards, binds variant field names into scope for each arm,
    /// and performs exhaustiveness checking against the subject type.
    pub(crate) fn check_when_arms(
        &mut self,
        subject_ty: &Ty,
        arms: &[WhenArm],
        scope: &mut Scope,
        when_span: Span,
    ) {
        let mut has_wildcard = false;
        let mut covered: HashSet<String> = HashSet::new();
        for arm in arms {
            for p in &arm.patterns {
                match p {
                    Pattern::Wildcard => has_wildcard = true,
                    Pattern::Ident(name) | Pattern::Variant { name, .. } => {
                        covered.insert(name.clone());
                    }
                    Pattern::Literal(_) => {}
                }
            }
            scope.push();
            for p in &arm.patterns {
                if let Pattern::Variant {
                    name: variant_name,
                    bindings,
                } = p
                {
                    for (idx, b) in bindings.iter().enumerate() {
                        if b == "_" {
                            continue;
                        }
                        let field_ty = self.resolve_variant_field(subject_ty, variant_name, b, idx);
                        scope.define(b.clone(), field_ty);
                    }
                }
            }
            if let Some(g) = &arm.guard {
                let g_ty = self.infer_expr(g, scope);
                self.expect(&g_ty, &Ty::Bool, "`when` guard");
            }
            for (s, s_span) in &arm.body {
                self.check_stmt(s, s_span.clone(), scope);
            }
            scope.pop();
        }

        // Restore the when-statement's span so exhaustiveness errors point
        // to the `when` line, not to whatever was last checked inside arms.
        self.current_span = Some(when_span);

        // Exhaustiveness
        match subject_ty.strip_nullable() {
            Ty::Enum(name, _) => {
                if has_wildcard {
                    return;
                }
                if let Some(variants) = self.enum_variants.get(name) {
                    let missing: Vec<&String> =
                        variants.iter().filter(|v| !covered.contains(*v)).collect();
                    if !missing.is_empty() {
                        let names: Vec<String> = missing.iter().map(|s| s.to_string()).collect();
                        self.err(format!(
                            "non-exhaustive `when` on enum `{name}` — missing: {}",
                            names.join(", ")
                        ));
                    }
                }
            }
            Ty::Unknown | Ty::Dynamic => {
                // Shallow inference: don't insist on wildcard for unknown subjects.
            }
            _ => {
                if !has_wildcard {
                    self.err(format!(
                        "`when` on non-enum type `{}` requires a `_` wildcard arm",
                        describe_ty(subject_ty)
                    ));
                }
            }
        }
    }
}
