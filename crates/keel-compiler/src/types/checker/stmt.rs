//! Statement-level type checking for the validation pass.
//!
//! Implements the second-pass check that walks task and agent bodies,
//! validates statement types (let, for, if, when, while, return, …),
//! and dispatches to expression inference for embedded expressions.

use std::collections::HashSet;

use crate::ast::*;
use crate::lexer::Span;
use crate::types::diagnostics::TypeDiagnostic;
use crate::types::scope::Scope;
use crate::types::ty::{Ty, UnknownReason, describe_ty};

use super::{Checker, binop::check_binop};

fn mock_registration_target_from_expr(expr: &SpannedExpr) -> Option<(String, String)> {
    let Expr::MethodCall {
        object,
        method,
        args: returns_args,
    } = &expr.kind
    else {
        return None;
    };
    if method != "returns" || returns_args.is_empty() {
        return None;
    }
    let Expr::MethodCall {
        object: mock_object,
        method: mock_method,
        args: mock_args,
    } = &object.as_ref().kind
    else {
        return None;
    };
    if mock_method != "mock" {
        return None;
    }
    let Expr::Ident(namespace) = &mock_object.as_ref().kind else {
        return None;
    };
    if namespace != "testing" {
        return None;
    }
    let target = mock_args.first()?;
    let Expr::FieldAccess(target_object, target_method) = &target.value.kind else {
        return None;
    };
    let Expr::Ident(target_namespace) = &target_object.as_ref().kind else {
        return None;
    };
    Some((target_namespace.clone(), target_method.clone()))
}

fn collect_mock_targets_from_expr(expr: &SpannedExpr, out: &mut HashSet<(String, String)>) {
    if let Some(target) = mock_registration_target_from_expr(expr) {
        out.insert(target);
    }
    match &expr.kind {
        Expr::StructLit(fields) => {
            for (_, value) in fields {
                collect_mock_targets_from_expr(value, out);
            }
        }
        Expr::StructSpreadUpdate { base, overrides } => {
            collect_mock_targets_from_expr(base, out);
            for (_, value) in overrides {
                collect_mock_targets_from_expr(value, out);
            }
        }
        Expr::ListLit(items) | Expr::SetLit(items) | Expr::TupleLit(items) => {
            for item in items {
                collect_mock_targets_from_expr(item, out);
            }
        }
        Expr::BinaryOp { left, right, .. }
        | Expr::NullCoalesce(left, right)
        | Expr::Pipeline(left, right)
        | Expr::Range(left, right) => {
            collect_mock_targets_from_expr(left, out);
            collect_mock_targets_from_expr(right, out);
        }
        Expr::UnaryOp { expr, .. }
        | Expr::NullFieldAccess(expr, _)
        | Expr::NullAssert(expr)
        | Expr::Duration { value: expr, .. }
        | Expr::Cast { expr, .. } => collect_mock_targets_from_expr(expr, out),
        Expr::FieldAccess(object, _) => collect_mock_targets_from_expr(object, out),
        Expr::Call { callee, args } => {
            collect_mock_targets_from_expr(callee, out);
            for arg in args {
                collect_mock_targets_from_expr(&arg.value, out);
            }
        }
        Expr::MethodCall { object, args, .. } => {
            collect_mock_targets_from_expr(object, out);
            for arg in args {
                collect_mock_targets_from_expr(&arg.value, out);
            }
        }
        Expr::Index { object, index } => {
            collect_mock_targets_from_expr(object, out);
            collect_mock_targets_from_expr(index, out);
        }
        Expr::IfExpr {
            cond,
            then_body,
            else_body,
        } => {
            collect_mock_targets_from_expr(cond, out);
            collect_mock_targets_from_block(then_body, out);
            collect_mock_targets_from_block(else_body, out);
        }
        Expr::WhenExpr { subject, arms } => {
            collect_mock_targets_from_expr(subject, out);
            for arm in arms {
                for pattern in &arm.patterns {
                    if let Pattern::Literal(expr) = pattern {
                        collect_mock_targets_from_expr(expr, out);
                    }
                }
                if let Some(guard) = &arm.guard {
                    collect_mock_targets_from_expr(guard, out);
                }
                collect_mock_targets_from_block(&arm.body, out);
            }
        }
        Expr::Lambda { body, .. } => match body {
            LambdaBody::Expr(expr) => collect_mock_targets_from_expr(expr, out),
            LambdaBody::Block(block) => collect_mock_targets_from_block(block, out),
        },
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::StringLit(_)
        | Expr::Bool(_)
        | Expr::None_
        | Expr::Ident(_)
        | Expr::SelfAccess { .. }
        | Expr::SelfRef
        | Expr::EnumVariant { .. } => {}
    }
}

fn collect_mock_targets_from_stmt(stmt: &Stmt, out: &mut HashSet<(String, String)>) {
    match stmt {
        Stmt::Let { value, .. }
        | Stmt::SelfAssign { value, .. }
        | Stmt::Return(Some(value))
        | Stmt::Raise(value)
        | Stmt::AugAssign { rhs: value, .. }
        | Stmt::Expr(value) => collect_mock_targets_from_expr(value, out),
        Stmt::Assert { cond, message } => {
            collect_mock_targets_from_expr(cond, out);
            if let Some(message) = message {
                collect_mock_targets_from_expr(message, out);
            }
        }
        Stmt::For {
            iter, filter, body, ..
        } => {
            collect_mock_targets_from_expr(iter, out);
            if let Some(filter) = filter {
                collect_mock_targets_from_expr(filter, out);
            }
            collect_mock_targets_from_block(body, out);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_mock_targets_from_expr(cond, out);
            collect_mock_targets_from_block(then_body, out);
            if let Some(else_body) = else_body {
                collect_mock_targets_from_block(else_body, out);
            }
        }
        Stmt::When { subject, arms } => {
            collect_mock_targets_from_expr(subject, out);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_mock_targets_from_expr(guard, out);
                }
                collect_mock_targets_from_block(&arm.body, out);
            }
        }
        Stmt::While { cond, body } => {
            collect_mock_targets_from_expr(cond, out);
            collect_mock_targets_from_block(body, out);
        }
        Stmt::TryCatch { body, catches } => {
            collect_mock_targets_from_block(body, out);
            for catch in catches {
                collect_mock_targets_from_block(&catch.body, out);
            }
        }
        Stmt::Return(None) | Stmt::Break | Stmt::Continue => {}
    }
}

fn collect_mock_targets_from_block(block: &Block, out: &mut HashSet<(String, String)>) {
    for stmt in block {
        collect_mock_targets_from_stmt(&stmt.kind, out);
    }
}

impl Checker<'_, '_> {
    /// Second-pass validation: walk all task, agent, and top-level statement
    /// declarations and type-check their bodies.
    ///
    /// Must be called after [`Checker::collect`] so that all type and task
    /// signatures are already registered.
    pub(crate) fn check_body(&mut self, program: &Program) {
        // HIR lowering is the canonical source of undefined-name diagnostics.
        self.errors.extend_from_slice(self.hir.diagnostics());

        // Top-level statements form the implicit main: they share one scope
        // across the whole program so a later statement can see the types of
        // earlier bindings — mirroring how the runtime's `execute()` shares
        // one `Environment` across top-level statements at run time.
        let mut top_level_scope = Scope::new();

        for node in &program.declarations {
            match &node.kind {
                Decl::Task(t) => {
                    self.current_agent = None;
                    self.check_task(t);
                }
                Decl::Test(t) => {
                    self.current_agent = None;
                    self.check_test(t);
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
                Decl::Stmt(stmt_node) => {
                    self.check_stmt(
                        &stmt_node.kind,
                        stmt_node.span.clone(),
                        &mut top_level_scope,
                    );
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

    fn check_test(&mut self, test: &TestDecl) {
        let mut scope = self.fresh_scope();
        let mut mock_targets = HashSet::new();
        collect_mock_targets_from_block(&test.setup, &mut mock_targets);
        collect_mock_targets_from_block(&test.body, &mut mock_targets);
        let saved_test_mocks = self.current_test_mocks.replace(mock_targets);
        if let Some(param) = &test.param {
            let cases_ty = self.infer_expr(&param.cases, &mut scope);
            let item_ty = match cases_ty.strip_nullable() {
                Ty::List(inner) => *inner.clone(),
                other if other.is_opaque() => other.clone(),
                other => {
                    self.err_at(
                        format!(
                            "parameterized test cases must be a list, got {}",
                            describe_ty(other)
                        ),
                        param.cases.span.clone(),
                    );
                    Ty::Error
                }
            };
            scope.define(param.name.clone(), item_ty);
        }
        for stmt in &test.setup {
            self.check_stmt(&stmt.kind, stmt.span.clone(), &mut scope);
        }
        for stmt in &test.body {
            self.check_stmt(&stmt.kind, stmt.span.clone(), &mut scope);
        }
        self.current_test_mocks = saved_test_mocks;
    }

    /// Bind `binding` to `ty` in `scope`, expanding destructure patterns field by field.
    fn bind_to_scope(&mut self, binding: &Binding, ty: &Ty, scope: &mut Scope) {
        match binding {
            Binding::Ident(name) => {
                scope.define(name.clone(), ty.clone());
            }
            Binding::Destruct(DestructPat::Struct(fields)) => {
                let struct_fields: Vec<(String, Ty)> = match ty.strip_nullable() {
                    Ty::Struct { fields: f, .. } => f.clone(),
                    other if other.is_opaque() => {
                        for (_, local) in fields {
                            scope.define(
                                local.clone(),
                                Ty::Unknown(UnknownReason::InferenceLimitation),
                            );
                        }
                        return;
                    }
                    other => {
                        self.err(format!(
                            "cannot destructure {} as a struct",
                            describe_ty(other)
                        ));
                        for (_, local) in fields {
                            scope.define(
                                local.clone(),
                                Ty::Unknown(UnknownReason::InferenceLimitation),
                            );
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
                            Ty::Error
                        });
                    scope.define(local.clone(), field_ty);
                }
            }
            Binding::Destruct(DestructPat::Tuple(names)) => {
                let elem_tys: Vec<Ty> = match ty.strip_nullable() {
                    Ty::Tuple(items) => items.clone(),
                    other if other.is_opaque() => {
                        for name in names {
                            scope.define(
                                name.clone(),
                                Ty::Unknown(UnknownReason::InferenceLimitation),
                            );
                        }
                        return;
                    }
                    other => {
                        self.err(format!(
                            "cannot destructure {} as a tuple",
                            describe_ty(other)
                        ));
                        for name in names {
                            scope.define(name.clone(), Ty::Error);
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
                    let t = elem_tys
                        .get(i)
                        .cloned()
                        .unwrap_or(Ty::Unknown(UnknownReason::InferenceLimitation));
                    scope.define(name.clone(), t);
                }
            }
        }
    }

    fn check_task(&mut self, t: &TaskDecl) {
        let declared_return = t
            .return_type
            .as_ref()
            .map(|ty| self.resolve_and_check_type(&ty.kind));
        let prev_return_ty = self.current_return_ty.take();
        self.current_return_ty = declared_return;

        let mut scope = self.fresh_scope();
        for p in &t.params {
            let elem_ty = self.resolve_and_check_type(&p.ty.kind);
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
        let last_stmt = t.body.last();
        let last_is_expr = last_stmt
            .map(|s_node| matches!(s_node.kind, Stmt::Expr(_)))
            .unwrap_or(false);
        let implicit_ty = self.block_type(&t.body, &mut scope);
        if last_is_expr
            && let Some(expected) = &self.current_return_ty.clone()
            && !matches!(expected, Ty::None_)
            && !expected.is_opaque()
        {
            // block_type dispatches Stmt::Expr nodes through infer_expr directly,
            // bypassing check_stmt (the only function that writes current_span).
            // Capture the last expression's own span so the diagnostic points to
            // the result expression rather than to byte 0.
            let expr_span = last_stmt.map(|n| n.span.clone()).unwrap_or(0..0);
            self.expect_at(&implicit_ty, expected, "implicit return", expr_span);
        }

        self.current_return_ty = prev_return_ty;
    }

    fn check_on_handler(&mut self, h: &OnHandler) {
        let mut scope = self.fresh_scope();
        if let Some(p) = &h.param {
            let param_ty = self.resolve_and_check_type(&p.ty.kind);
            self.bind_to_scope(&p.name, &param_ty, &mut scope);
        }
        self.check_block(&h.body, &mut scope);
    }

    fn check_attribute(&mut self, attr: &AttributeDecl) {
        // `@provider` must name a built-in backend. Validate here so a typo is a
        // compile-time error from `keel check` — including names that happen to
        // resolve (e.g. a declared type), which the generic `infer_expr` path
        // below would otherwise accept.
        if attr.name == "provider" {
            self.check_provider_attribute(attr);
            return;
        }
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
                        if !matches!(ty, Ty::Bool) && !ty.is_opaque() {
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

    fn check_provider_attribute(&mut self, attr: &AttributeDecl) {
        let AttributeBody::Expr(node) = &attr.body else {
            let span = self.current_span.clone().unwrap_or(0..0);
            self.err_at(keel_catalog::provider_attribute_error(), span);
            return;
        };
        let Expr::Ident(name) = &node.kind else {
            self.err_at(keel_catalog::provider_attribute_error(), node.span.clone());
            return;
        };
        self.check_provider_name(name, node.span.clone());
    }

    /// Validate a provider reference — `@provider X` or `ai.install(X)`.
    ///
    /// `X` must be a built-in backend name or a type implementing `LlmProvider`.
    /// A user provider type must be field-less: the runtime constructs its
    /// receiver with no fields, so configuration is read from `env.*` inside
    /// `complete()`.
    pub(crate) fn check_provider_name(&mut self, name: &str, span: Span) {
        if keel_catalog::is_builtin_llm_provider(name) {
            return;
        }
        if self.llm_provider_types.contains(name) {
            if let Some(fields) = self.structs.get(name)
                && !fields.is_empty()
            {
                self.err_at(
                    format!(
                        "provider `{name}` must be a field-less type — read configuration \
                         from env.* inside `complete()`, since the runtime constructs the \
                         provider with no fields"
                    ),
                    span,
                );
            }
            return;
        }
        self.err_at(keel_catalog::provider_attribute_error(), span);
    }

    fn check_block(&mut self, block: &Block, scope: &mut Scope) {
        scope.push();
        for node in block {
            self.check_stmt(&node.kind, node.span.clone(), scope);
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
                    Some(ty_node) => {
                        let declared = self.resolve_and_check_type(&ty_node.kind);
                        // Only check when declared type is concrete — an opaque
                        // type means the checker couldn't resolve it (e.g. an
                        // unrecognised named type), so a mismatch here would be
                        // a false positive.
                        if let Binding::Ident(name) = binding
                            && !declared.is_opaque()
                        {
                            // Point the caret at the type annotation, not the
                            // whole statement, so the user sees exactly which
                            // annotation is wrong.
                            let saved = self.current_span.replace(ty_node.span.clone());
                            self.expect(&inferred, &declared, &format!("`{name}`"));
                            self.current_span = saved;
                        }
                        declared
                    }
                    None => {
                        // In strict mode, warn when the inferred type is Unknown
                        // (any reason) — the user should add an annotation.
                        // Dynamic is intentional and never warned about.
                        if self.strict
                            && matches!(inferred, Ty::Unknown(_))
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
            Stmt::SelfAssign { field, value, .. } => {
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
                        && !matches!(expected, Ty::None_)
                        && !expected.is_opaque()
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
                    other if other.is_opaque() => {
                        // Iterable type is opaque — element type is also opaque.
                        other.clone()
                    }
                    Ty::Struct { name: Some(n), .. } => {
                        if self.iterable_types.contains(n.as_str()) {
                            Ty::Unknown(UnknownReason::InferenceLimitation)
                        } else {
                            self.err(format!("`for` expects a list, got struct `{n}`"));
                            Ty::Error
                        }
                    }
                    Ty::Struct { .. } => {
                        self.err("`for` expects a list, got anonymous struct".to_string());
                        Ty::Error
                    }
                    other => {
                        self.err(format!("`for` expects a list, got {}", describe_ty(other)));
                        Ty::Error
                    }
                };
                scope.push();
                self.bind_to_scope(binding, &element_ty, scope);
                if let Some(pred) = filter {
                    let pty = self.infer_expr(pred, scope);
                    self.expect(&pty, &Ty::Bool, "for-if guard");
                }
                for s_node in body {
                    self.check_stmt(&s_node.kind, s_node.span.clone(), scope);
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
                    let ty = self.resolve_type(&c.ty.kind);
                    scope.define(c.name.clone(), ty);
                    for s_node in &c.body {
                        self.check_stmt(&s_node.kind, s_node.span.clone(), scope);
                    }
                    scope.pop();
                }
            }
            Stmt::AugAssign {
                name,
                name_span,
                op,
                rhs,
            } => {
                let var_ty = scope.get(name).cloned().unwrap_or_else(|| {
                    self.err_at(
                        format!("augmented assignment to undefined variable `{name}`"),
                        name_span.clone(),
                    );
                    Ty::Error
                });
                let rhs_ty = self.infer_expr(rhs, scope);
                if let Some(msg) = check_binop(*op, &var_ty, &rhs_ty) {
                    self.err(msg);
                }
            }
            Stmt::Raise(e) => {
                self.infer_expr(e, scope);
            }
            Stmt::Assert { cond, message } => {
                let ty = self.infer_expr(cond, scope);
                self.expect(&ty, &Ty::Bool, "`assert` condition");
                if let Some(message) = message {
                    let ty = self.infer_expr(message, scope);
                    self.expect(&ty, &Ty::Str, "`assert` message");
                }
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
        // An unguarded struct arm is only total when the subject is a
        // *non-nullable* struct: a struct pattern never matches an enum
        // variant or a `none`, so it must not stand in for `_` otherwise.
        let subject_is_plain_struct = matches!(subject_ty, Ty::Struct { .. });
        for arm in arms {
            for p in &arm.patterns {
                match p {
                    Pattern::Wildcard => has_wildcard = true,
                    Pattern::Ident(name) | Pattern::Variant { name, .. } => {
                        covered.insert(name.clone());
                    }
                    Pattern::Struct { .. } if arm.guard.is_none() && subject_is_plain_struct => {
                        has_wildcard = true;
                    }
                    Pattern::Literal(_) | Pattern::Struct { .. } => {}
                }
            }
            scope.push();
            for p in &arm.patterns {
                match p {
                    Pattern::Variant {
                        name: variant_name,
                        bindings,
                    } => {
                        self.check_variant_pattern_fields(subject_ty, variant_name, bindings);
                        for (idx, b) in bindings.iter().enumerate() {
                            if b == "_" {
                                continue;
                            }
                            let field_ty =
                                self.resolve_variant_field(subject_ty, variant_name, b, idx);
                            scope.define(b.clone(), field_ty);
                        }
                    }
                    Pattern::Struct { fields } => {
                        self.bind_struct_pattern_fields(subject_ty, fields, scope);
                    }
                    _ => {}
                }
            }
            if let Some(g) = &arm.guard {
                let g_ty = self.infer_expr(g, scope);
                self.expect(&g_ty, &Ty::Bool, "`when` guard");
            }
            for s_node in &arm.body {
                self.check_stmt(&s_node.kind, s_node.span.clone(), scope);
            }
            scope.pop();
        }

        // Restore the when-statement's span so exhaustiveness errors point
        // to the `when` line, not to whatever was last checked inside arms.
        self.current_span = Some(when_span.clone());

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
                        self.errors.push(TypeDiagnostic::NonExhaustiveWhen {
                            enum_name: name.clone(),
                            missing: names,
                            span: when_span.clone(),
                        });
                    }
                }
            }
            other if other.is_opaque() => {
                // Opaque subject type (Unknown, Dynamic, Error, Unresolved) —
                // don't insist on a wildcard arm; shallow inference can't determine
                // the variant set.
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

    /// Validate the field names a `variant { … }` pattern destructures.
    ///
    /// When the subject is a known rich enum and the variant's declared field
    /// set is known, a binding that names a field the variant does not declare
    /// is a hard error rather than a silent `none` binding. Every other case
    /// (non-enum subject, unknown enum/variant) is left to the lenient
    /// type-resolution path — shallow inference may not have the field set.
    fn check_variant_pattern_fields(
        &mut self,
        subject_ty: &Ty,
        variant_name: &str,
        bindings: &[String],
    ) {
        let Ty::Enum(enum_name, _) = subject_ty.strip_nullable() else {
            return;
        };
        let Some(variant_fields) = self
            .enum_variant_fields
            .get(enum_name)
            .and_then(|variants| variants.get(variant_name))
        else {
            return;
        };
        let unknown: Vec<String> = bindings
            .iter()
            .filter(|b| *b != "_" && !variant_fields.iter().any(|f| f == *b))
            .cloned()
            .collect();
        for b in unknown {
            self.err(format!(
                "variant pattern field `{b}` does not exist on `{enum_name}.{variant_name}`"
            ));
        }
    }

    /// Bind the fields named by a `{ … }` struct pattern into `scope`,
    /// validating them against the subject type.
    ///
    /// A struct pattern only makes sense against a struct subject; the field
    /// names must exist on that struct. Either violation is a hard error
    /// rather than a silent `none` binding — a mistyped field should not pass
    /// the checker. For opaque subjects (shallow inference couldn't resolve a
    /// concrete type) we bind without validation to avoid false positives.
    fn bind_struct_pattern_fields(
        &mut self,
        subject_ty: &Ty,
        fields: &[String],
        scope: &mut Scope,
    ) {
        match subject_ty.strip_nullable() {
            Ty::Struct {
                fields: struct_fields,
                ..
            } => {
                for f in fields {
                    if f == "_" {
                        continue;
                    }
                    match struct_fields.iter().find(|(n, _)| n == f) {
                        Some((_, ty)) => scope.define(f.clone(), ty.clone()),
                        None => {
                            self.err(format!(
                                "struct pattern field `{f}` does not exist on `{}`",
                                describe_ty(subject_ty)
                            ));
                            scope.define(f.clone(), Ty::Error);
                        }
                    }
                }
            }
            other if other.is_opaque() => {
                for f in fields {
                    if f == "_" {
                        continue;
                    }
                    scope.define(f.clone(), self.resolve_struct_field(subject_ty, f));
                }
            }
            _ => {
                self.err(format!(
                    "struct pattern `{{ … }}` requires a struct subject, but `when` subject has type `{}`",
                    describe_ty(subject_ty)
                ));
                for f in fields {
                    if f == "_" {
                        continue;
                    }
                    scope.define(f.clone(), Ty::Error);
                }
            }
        }
    }
}
