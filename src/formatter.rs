//! Formatter — placeholder for v0.1.
//!
//! `keel fmt` lands in a follow-up change; for now this emits a placeholder
//! message.

use crate::ast::*;

fn binding_str(b: &Binding) -> String {
    match b {
        Binding::Ident(name) => name.clone(),
        Binding::Destruct(DestructPat::Struct(fields)) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(src, local)| {
                    if src == local {
                        src.clone()
                    } else {
                        format!("{src}: {local}")
                    }
                })
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }
        Binding::Destruct(DestructPat::Tuple(names)) => {
            format!("({})", names.join(", "))
        }
    }
}

#[must_use]
pub fn format_program(program: &Program) -> String {
    let mut f = Fmt::new();
    for (i, node) in program.declarations.iter().enumerate() {
        if i > 0 {
            f.blank_line();
        }
        f.decl(&node.kind);
    }
    let mut out = f.into_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

const INDENT: &str = "  ";

struct Fmt {
    buf: String,
    indent: usize,
    /// True when the last line we wrote was blank; used to collapse
    /// consecutive blank separators.
    at_line_start: bool,
}

impl Fmt {
    fn new() -> Self {
        // 4 KiB is a reasonable starting buffer for most .keel files;
        // avoids small-string reallocations during formatting.
        Fmt {
            buf: String::with_capacity(4096),
            indent: 0,
            at_line_start: true,
        }
    }

    fn into_string(self) -> String {
        self.buf
    }

    fn push(&mut self, s: &str) {
        if self.at_line_start {
            for _ in 0..self.indent {
                self.buf.push_str(INDENT);
            }
            self.at_line_start = false;
        }
        self.buf.push_str(s);
    }

    fn newline(&mut self) {
        self.buf.push('\n');
        self.at_line_start = true;
    }

    fn blank_line(&mut self) {
        // Ensure there's a blank line in the buffer without stacking.
        if !self.buf.ends_with("\n\n") {
            if !self.buf.ends_with('\n') {
                self.buf.push('\n');
            }
            self.buf.push('\n');
        }
        self.at_line_start = true;
    }

    fn indent(&mut self) {
        self.indent += 1;
    }
    fn dedent(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;
        }
    }

    // -----------------------------------------------------------------
    // Declarations
    // -----------------------------------------------------------------

    fn decl(&mut self, decl: &Decl) {
        match decl {
            Decl::Type(t) => self.type_decl(t),
            Decl::Interface(i) => self.interface_decl(i),
            Decl::Impl(i) => self.impl_decl(i),
            Decl::Task(t) => self.task_decl(t),
            Decl::Extern(e) => self.extern_decl(e),
            Decl::Agent(a) => self.agent_decl(a),
            Decl::Use(u) => self.use_decl(u),
            Decl::Stmt(stmt_node) => {
                self.stmt(&stmt_node.kind);
                self.newline();
            }
        }
    }

    fn type_decl(&mut self, t: &TypeDecl) {
        let header = if t.type_params.is_empty() {
            t.name.clone()
        } else {
            format!("{}[{}]", t.name, t.type_params.join(", "))
        };
        match &t.def {
            TypeDef::SimpleEnum(variants) => {
                self.push(&format!("type {} = ", header));
                self.push(&variants.join(" | "));
                self.newline();
            }
            TypeDef::RichEnum(variants) => {
                self.push(&format!("type {} =", header));
                self.newline();
                self.indent();
                for v in variants {
                    self.push("| ");
                    self.push(&v.name);
                    if let Some(fields) = &v.fields {
                        self.push(" { ");
                        for (i, f) in fields.iter().enumerate() {
                            if i > 0 {
                                self.push(", ");
                            }
                            self.push(&format!("{}: ", f.name));
                            self.push(&self.type_expr_str(&f.ty.kind));
                        }
                        self.push(" }");
                    }
                    self.newline();
                }
                self.dedent();
            }
            TypeDef::Struct(fields) => {
                self.push(&format!("type {} {{", header));
                self.newline();
                self.indent();
                for f in fields {
                    self.push(&format!("{}: ", f.name));
                    self.push(&self.type_expr_str(&f.ty.kind));
                    self.newline();
                }
                self.dedent();
                self.push("}");
                self.newline();
            }
            TypeDef::Alias(ty_node) => {
                self.push(&format!("type {} = ", header));
                self.push(&self.type_expr_str(&ty_node.kind));
                self.newline();
            }
        }
    }

    fn impl_decl(&mut self, i: &ImplDecl) {
        self.push(&format!("impl {} for {} {{", i.interface_name, i.type_name));
        self.newline();
        self.indent();
        for method in &i.methods {
            self.task_decl(method);
        }
        self.dedent();
        self.push("}");
        self.newline();
    }

    fn interface_decl(&mut self, i: &InterfaceDecl) {
        self.push(&format!("interface {} {{", i.name));
        self.newline();
        self.indent();
        for method in &i.methods {
            self.push(&format!("task {}(", method.name));
            self.params(&method.params);
            self.push(")");
            if let Some(ret_node) = &method.return_type {
                self.push(" -> ");
                self.push(&self.type_expr_str(&ret_node.kind));
            }
            self.newline();
        }
        self.dedent();
        self.push("}");
        self.newline();
    }

    fn extern_decl(&mut self, e: &ExternDecl) {
        self.push(&format!("extern task {}(", e.name));
        self.params(&e.params);
        self.push(") -> ");
        self.push(&self.type_expr_str(&e.return_type.kind));
        self.push(&format!(" from \"{}\"", e.source));
        self.newline();
    }

    fn use_decl(&mut self, u: &UseDecl) {
        match &u.kind {
            UseKind::File(path) => {
                self.push(&format!("use \"{path}\""));
            }
            UseKind::Symbol { name, source } => {
                self.push(&format!("use {name} from \"{source}\""));
            }
            UseKind::Package(parts) => {
                self.push(&format!("use {}", parts.join("/")));
            }
        }
        self.newline();
    }

    fn task_decl(&mut self, t: &TaskDecl) {
        let header = if t.type_params.is_empty() {
            t.name.clone()
        } else {
            format!("{}[{}]", t.name, t.type_params.join(", "))
        };
        self.push(&format!("task {}(", header));
        self.params(&t.params);
        self.push(")");
        if let Some(ret_node) = &t.return_type {
            self.push(" -> ");
            self.push(&self.type_expr_str(&ret_node.kind));
        }
        self.push(" {");
        self.newline();
        self.indent();
        for stmt_node in &t.body {
            self.stmt(&stmt_node.kind);
            self.newline();
        }
        self.dedent();
        self.push("}");
        self.newline();
    }

    fn params(&mut self, params: &[Param]) {
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            if p.variadic {
                self.push("...");
            }
            // impl receiver: emit bare `self` for the receiver param
            if matches!(&p.ty.kind, TypeExpr::SelfType) {
                self.push("self");
                continue;
            }
            self.push(&format!("{}: ", binding_str(&p.name)));
            self.push(&self.type_expr_str(&p.ty.kind));
            if let Some(default) = &p.default {
                self.push(" = ");
                self.push(&self.expr_str(default));
            }
        }
    }

    fn agent_decl(&mut self, a: &AgentDecl) {
        self.push(&format!("agent {} {{", a.name));
        self.newline();
        self.indent();
        let mut first = true;
        for item in &a.items {
            if !first {
                // Blank line between non-attribute items. Attributes
                // stay packed together; state / tasks / handlers get
                // spacing.
                match item {
                    AgentItem::Attribute(_) => {}
                    _ => self.blank_line(),
                }
            }
            first = false;
            self.agent_item(item);
        }
        self.dedent();
        self.push("}");
        self.newline();
    }

    fn agent_item(&mut self, item: &AgentItem) {
        match item {
            AgentItem::Attribute(attr) => {
                self.push(&format!("@{} ", attr.name));
                match &attr.body {
                    AttributeBody::Expr(e) => {
                        self.push(&self.expr_str(e));
                        self.newline();
                    }
                    AttributeBody::Tools(entries) => {
                        self.push("[");
                        let parts: Vec<String> = entries
                            .iter()
                            .map(|e| {
                                let mut s = e.namespace.clone();
                                if let Some(m) = &e.method {
                                    s.push('.');
                                    s.push_str(m);
                                }
                                if let Some(cond) = &e.condition {
                                    s.push_str(" if ");
                                    s.push_str(&self.expr_str(cond));
                                }
                                s
                            })
                            .collect();
                        self.push(&parts.join(", "));
                        self.push("]");
                        self.newline();
                    }
                    AttributeBody::Block(body) => {
                        self.push("{");
                        self.newline();
                        self.indent();
                        for s_node in body {
                            self.stmt(&s_node.kind);
                            self.newline();
                        }
                        self.dedent();
                        self.push("}");
                        self.newline();
                    }
                }
            }
            AgentItem::State(fields) => {
                self.push("state {");
                self.newline();
                self.indent();
                for f in fields {
                    self.push(&format!("{}: ", f.name));
                    if f.readonly {
                        self.push("readonly ");
                    }
                    self.push(&self.type_expr_str(&f.ty.kind));
                    self.push(" = ");
                    self.push(&self.expr_str(&f.default));
                    self.newline();
                }
                self.dedent();
                self.push("}");
                self.newline();
            }
            AgentItem::Task(t) => self.task_decl(t),
            AgentItem::On(h) => {
                self.push(&format!("on {}", h.event));
                if let Some(p) = &h.param {
                    self.push(&format!("({}: ", binding_str(&p.name)));
                    self.push(&self.type_expr_str(&p.ty.kind));
                    self.push(")");
                } else {
                    self.push("()");
                }
                self.push(" {");
                self.newline();
                self.indent();
                for s_node in &h.body {
                    self.stmt(&s_node.kind);
                    self.newline();
                }
                self.dedent();
                self.push("}");
                self.newline();
            }
        }
    }

    // -----------------------------------------------------------------
    // Statements
    // -----------------------------------------------------------------

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { binding, ty, value } => {
                self.push(&binding_str(binding));
                if let Some(ty_node) = ty {
                    self.push(": ");
                    self.push(&self.type_expr_str(&ty_node.kind));
                }
                self.push(" = ");
                self.push(&self.expr_str(value));
            }
            Stmt::SelfAssign { field, value, .. } => {
                self.push(&format!("self.{field} = "));
                self.push(&self.expr_str(value));
            }
            Stmt::Return(opt) => {
                self.push("return");
                if let Some(e) = opt {
                    self.push(" ");
                    self.push(&self.expr_str(e));
                }
            }
            Stmt::For {
                binding,
                iter,
                filter,
                body,
            } => {
                self.push(&format!("for {} in ", binding_str(binding)));
                self.push(&self.expr_str(iter));
                if let Some(pred) = filter {
                    self.push(" if ");
                    self.push(&self.expr_str(pred));
                }
                self.push(" {");
                self.newline();
                self.indent();
                for s_node in body {
                    self.stmt(&s_node.kind);
                    self.newline();
                }
                self.dedent();
                self.push("}");
            }
            Stmt::While { cond, body } => {
                self.push("while ");
                self.push(&self.expr_str(cond));
                self.push(" {");
                self.newline();
                self.indent();
                for s_node in body {
                    self.stmt(&s_node.kind);
                    self.newline();
                }
                self.dedent();
                self.push("}");
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.push("if ");
                self.push(&self.expr_str(cond));
                self.push(" {");
                self.newline();
                self.indent();
                for s_node in then_body {
                    self.stmt(&s_node.kind);
                    self.newline();
                }
                self.dedent();
                self.push("}");
                if let Some(eb) = else_body {
                    if eb.len() == 1 && matches!(eb[0].kind, Stmt::If { .. }) {
                        self.push(" else ");
                        self.stmt(&eb[0].kind);
                    } else {
                        self.push(" else {");
                        self.newline();
                        self.indent();
                        for s_node in eb {
                            self.stmt(&s_node.kind);
                            self.newline();
                        }
                        self.dedent();
                        self.push("}");
                    }
                }
            }
            Stmt::When { subject, arms } => {
                self.push("when ");
                self.push(&self.expr_str(subject));
                self.push(" {");
                self.newline();
                self.indent();
                for arm in arms {
                    self.when_arm(arm);
                }
                self.dedent();
                self.push("}");
            }
            Stmt::TryCatch { body, catches } => {
                self.push("try {");
                self.newline();
                self.indent();
                for s_node in body {
                    self.stmt(&s_node.kind);
                    self.newline();
                }
                self.dedent();
                self.push("}");
                for c in catches {
                    self.push(&format!(" catch {}: ", c.name));
                    self.push(&self.type_expr_str(&c.ty.kind));
                    self.push(" {");
                    self.newline();
                    self.indent();
                    for s_node in &c.body {
                        self.stmt(&s_node.kind);
                        self.newline();
                    }
                    self.dedent();
                    self.push("}");
                }
            }
            Stmt::AugAssign { name, op, rhs, .. } => {
                self.push(&format!("{name} {}= ", binop_str(*op)));
                self.push(&self.expr_str(rhs));
            }
            Stmt::Raise(e) => {
                self.push("raise ");
                self.push(&self.expr_str(e));
            }
            Stmt::Break => self.push("break"),
            Stmt::Continue => self.push("continue"),
            Stmt::Expr(e) => {
                self.push(&self.expr_str(e));
            }
        }
    }

    fn when_arm(&mut self, arm: &WhenArm) {
        for (i, p) in arm.patterns.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.push(&self.pattern_str(p));
        }
        if let Some(g) = &arm.guard {
            self.push(" where ");
            self.push(&self.expr_str(g));
        }
        self.push(" => ");
        // Single-expression arms stay inline; multi-stmt arms open a block.
        if arm.body.len() == 1
            && let Stmt::Expr(e) = &arm.body[0].kind
        {
            self.push(&self.expr_str(e));
            self.newline();
            return;
        }
        self.push("{");
        self.newline();
        self.indent();
        for s_node in &arm.body {
            self.stmt(&s_node.kind);
            self.newline();
        }
        self.dedent();
        self.push("}");
        self.newline();
    }

    fn pattern_str(&self, p: &Pattern) -> String {
        match p {
            Pattern::Ident(name) => name.clone(),
            Pattern::Wildcard => "_".into(),
            Pattern::Literal(e) => self.expr_str(e),
            Pattern::Variant { name, bindings } => {
                if bindings.is_empty() {
                    name.clone()
                } else {
                    format!("{} {{ {} }}", name, bindings.join(", "))
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Expressions — produce strings so we can compose inline.
    // -----------------------------------------------------------------

    fn expr_str(&self, spanned: &SpannedExpr) -> String {
        self.expr_at(spanned, self.indent)
    }

    fn expr_at(&self, spanned: &SpannedExpr, indent: usize) -> String {
        let expr = &spanned.kind;
        match expr {
            Expr::Integer(n) => n.to_string(),
            Expr::Float(f) => {
                let s = f.to_string();
                if s.contains('.') { s } else { format!("{s}.0") }
            }
            Expr::Bool(b) => b.to_string(),
            Expr::None_ => "none".into(),
            Expr::StringLit(parts) => self.string_lit(parts),
            Expr::Ident(name) => name.clone(),
            Expr::SelfAccess { field, .. } => format!("self.{field}"),
            Expr::SelfRef => "self".to_string(),
            Expr::FieldAccess(obj, f) => format!("{}.{}", self.expr_str(obj), f),
            Expr::NullFieldAccess(obj, f) => format!("{}?.{}", self.expr_str(obj), f),
            Expr::NullAssert(e) => format!("{}!", self.expr_str(e)),
            Expr::StructLit(fields) => {
                if fields.is_empty() {
                    "{}".into()
                } else {
                    let parts: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("{}: {}", map_key_form(k), self.expr_at(v, indent)))
                        .collect();
                    format!("{{ {} }}", parts.join(", "))
                }
            }
            Expr::StructSpreadUpdate { base, overrides } => {
                let base_str = self.expr_at(base, indent);
                if overrides.is_empty() {
                    format!("{{ ...{} }}", base_str)
                } else {
                    let parts: Vec<String> = overrides
                        .iter()
                        .map(|(k, v)| format!("{}: {}", k, self.expr_at(v, indent)))
                        .collect();
                    format!("{{ ...{}, {} }}", base_str, parts.join(", "))
                }
            }
            Expr::ListLit(items) => {
                let parts: Vec<String> = items.iter().map(|e| self.expr_str(e)).collect();
                format!("[{}]", parts.join(", "))
            }
            Expr::SetLit(items) => {
                let parts: Vec<String> = items.iter().map(|e| self.expr_str(e)).collect();
                format!("set[{}]", parts.join(", "))
            }
            Expr::TupleLit(items) => {
                let parts: Vec<String> = items.iter().map(|e| self.expr_str(e)).collect();
                format!("({})", parts.join(", "))
            }
            Expr::BinaryOp { left, op, right } => {
                format!(
                    "{} {} {}",
                    self.expr_str(left),
                    binop_str(*op),
                    self.expr_str(right)
                )
            }
            Expr::UnaryOp { op, expr } => match op {
                UnOp::Neg => format!("-{}", self.expr_str(expr)),
                UnOp::Not => format!("not {}", self.expr_str(expr)),
            },
            Expr::NullCoalesce(l, r) => format!("{} ?? {}", self.expr_str(l), self.expr_str(r)),
            Expr::Pipeline(l, r) => format!("{} |> {}", self.expr_str(l), self.expr_str(r)),
            Expr::Range(l, r) => format!("{}..{}", self.expr_str(l), self.expr_str(r)),
            Expr::Call { callee, args } => {
                format!(
                    "{}({})",
                    self.expr_at(callee, indent),
                    self.args_at(args, indent)
                )
            }
            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                format!(
                    "{}.{}({})",
                    self.expr_at(object, indent),
                    method,
                    self.args_at(args, indent)
                )
            }
            Expr::Cast { expr, ty } => {
                format!(
                    "{} as {}",
                    self.expr_str(expr),
                    self.type_expr_str(&ty.kind)
                )
            }
            Expr::IfExpr {
                cond,
                then_body,
                else_body,
            } => {
                let then_str = self.block_inline(then_body);
                // When else_body is a single if-expression, emit `else if …`
                // so that re-parsing produces the same AST (idempotent).
                if else_body.len() == 1
                    && let Stmt::Expr(inner) = &else_body[0].kind
                    && let Expr::IfExpr { .. } = &inner.kind
                {
                    return format!(
                        "if {} {{ {} }} else {}",
                        self.expr_str(cond),
                        then_str,
                        self.expr_str(inner),
                    );
                }
                format!(
                    "if {} {{ {} }} else {{ {} }}",
                    self.expr_str(cond),
                    then_str,
                    self.block_inline(else_body),
                )
            }
            Expr::Lambda { params, body } => {
                let params_str = if params.len() == 1 && params[0].ty.is_none() {
                    params[0].name.clone()
                } else {
                    let parts: Vec<String> = params
                        .iter()
                        .map(|p| match &p.ty {
                            Some(ty_node) => {
                                format!("{}: {}", p.name, self.type_expr_str(&ty_node.kind))
                            }
                            None => p.name.clone(),
                        })
                        .collect();
                    format!("({})", parts.join(", "))
                };
                match body {
                    LambdaBody::Expr(e) => format!("{params_str} => {}", self.expr_at(e, indent)),
                    LambdaBody::Block(b) => self.lambda_block(&params_str, b, indent),
                }
            }
            Expr::WhenExpr { subject, arms } => {
                let mut s = format!("when {} {{\n", self.expr_at(subject, indent));
                for arm in arms {
                    for _ in 0..=indent {
                        s.push_str(INDENT);
                    }
                    self.write_when_arm(&mut s, arm, indent + 1);
                }
                for _ in 0..indent {
                    s.push_str(INDENT);
                }
                s.push('}');
                s
            }
            Expr::Index { object, index } => {
                format!("{}[{}]", self.expr_str(object), self.expr_str(index))
            }
            Expr::Duration { value, unit } => {
                format!("{}.{}", self.expr_str(value), unit.canonical_name())
            }
            Expr::EnumVariant {
                ty,
                variant,
                fields,
            } => {
                if fields.is_empty() {
                    format!("{ty}.{variant}")
                } else {
                    let parts: Vec<String> = fields
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", self.expr_str(v)))
                        .collect();
                    format!("{ty}.{variant} {{ {} }}", parts.join(", "))
                }
            }
        }
    }

    /// Multi-line lambda body: `params => {\n  stmt\n  stmt\n}` with the
    /// closing brace re-indented to `indent`. Ensures the formatter is
    /// idempotent even for complex closure bodies.
    fn lambda_block(&self, params_str: &str, body: &Block, indent: usize) -> String {
        let inner_indent = indent + 1;
        let mut s = format!("{params_str} => {{\n");
        for stmt_node in body {
            for _ in 0..inner_indent {
                s.push_str(INDENT);
            }
            self.write_stmt(&mut s, &stmt_node.kind, inner_indent);
            s.push('\n');
        }
        for _ in 0..indent {
            s.push_str(INDENT);
        }
        s.push('}');
        s
    }

    /// Write a statement into a string buffer at the given indent.
    /// Mirrors `Fmt::stmt` but outputs to `s` instead of `self.buf`.
    fn write_stmt(&self, s: &mut String, stmt: &Stmt, indent: usize) {
        match stmt {
            Stmt::Expr(e) => s.push_str(&self.expr_at(e, indent)),
            Stmt::Let { binding, ty, value } => {
                s.push_str(&binding_str(binding));
                if let Some(ty_node) = ty {
                    s.push_str(": ");
                    s.push_str(&self.type_expr_str(&ty_node.kind));
                }
                s.push_str(" = ");
                s.push_str(&self.expr_at(value, indent));
            }
            Stmt::SelfAssign { field, value, .. } => {
                s.push_str(&format!("self.{field} = "));
                s.push_str(&self.expr_at(value, indent));
            }
            Stmt::Raise(e) => {
                s.push_str("raise ");
                s.push_str(&self.expr_at(e, indent));
            }
            Stmt::Return(Some(e)) => {
                s.push_str("return ");
                s.push_str(&self.expr_at(e, indent));
            }
            Stmt::Return(None) => s.push_str("return"),
            Stmt::For {
                binding,
                iter,
                filter,
                body,
            } => {
                s.push_str(&format!("for {} in ", binding_str(binding)));
                s.push_str(&self.expr_at(iter, indent));
                if let Some(pred) = filter {
                    s.push_str(" if ");
                    s.push_str(&self.expr_at(pred, indent));
                }
                s.push_str(" {\n");
                self.write_block(s, body, indent + 1);
                for _ in 0..indent {
                    s.push_str(INDENT);
                }
                s.push('}');
            }
            Stmt::While { cond, body } => {
                s.push_str("while ");
                s.push_str(&self.expr_at(cond, indent));
                s.push_str(" {\n");
                self.write_block(s, body, indent + 1);
                for _ in 0..indent {
                    s.push_str(INDENT);
                }
                s.push('}');
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                s.push_str("if ");
                s.push_str(&self.expr_at(cond, indent));
                s.push_str(" {\n");
                self.write_block(s, then_body, indent + 1);
                for _ in 0..indent {
                    s.push_str(INDENT);
                }
                s.push('}');
                if let Some(eb) = else_body {
                    if eb.len() == 1 && matches!(eb[0].kind, Stmt::If { .. }) {
                        s.push_str(" else ");
                        self.write_stmt(s, &eb[0].kind, indent);
                    } else {
                        s.push_str(" else {\n");
                        self.write_block(s, eb, indent + 1);
                        for _ in 0..indent {
                            s.push_str(INDENT);
                        }
                        s.push('}');
                    }
                }
            }
            Stmt::When { subject, arms } => {
                s.push_str("when ");
                s.push_str(&self.expr_at(subject, indent));
                s.push_str(" {\n");
                for arm in arms {
                    for _ in 0..(indent + 1) {
                        s.push_str(INDENT);
                    }
                    self.write_when_arm(s, arm, indent + 1);
                }
                for _ in 0..indent {
                    s.push_str(INDENT);
                }
                s.push('}');
            }
            Stmt::TryCatch { body, catches } => {
                s.push_str("try {\n");
                self.write_block(s, body, indent + 1);
                for _ in 0..indent {
                    s.push_str(INDENT);
                }
                s.push('}');
                for c in catches {
                    s.push_str(&format!(" catch {}: ", c.name));
                    s.push_str(&self.type_expr_str(&c.ty.kind));
                    s.push_str(" {\n");
                    self.write_block(s, &c.body, indent + 1);
                    for _ in 0..indent {
                        s.push_str(INDENT);
                    }
                    s.push('}');
                }
            }
            Stmt::AugAssign { name, op, rhs, .. } => {
                s.push_str(&format!("{name} {}= ", binop_str(*op)));
                s.push_str(&self.expr_at(rhs, indent));
            }
            Stmt::Break => s.push_str("break"),
            Stmt::Continue => s.push_str("continue"),
        }
    }

    fn write_block(&self, s: &mut String, block: &Block, indent: usize) {
        for stmt_node in block {
            for _ in 0..indent {
                s.push_str(INDENT);
            }
            self.write_stmt(s, &stmt_node.kind, indent);
            s.push('\n');
        }
    }

    fn write_when_arm(&self, s: &mut String, arm: &WhenArm, indent: usize) {
        let pats: Vec<String> = arm.patterns.iter().map(|p| self.pattern_str(p)).collect();
        s.push_str(&pats.join(", "));
        if let Some(g) = &arm.guard {
            s.push_str(" where ");
            s.push_str(&self.expr_at(g, indent));
        }
        s.push_str(" => ");
        if arm.body.len() == 1
            && let Stmt::Expr(e) = &arm.body[0].kind
        {
            s.push_str(&self.expr_at(e, indent));
            s.push('\n');
            return;
        }
        s.push_str("{\n");
        self.write_block(s, &arm.body, indent + 1);
        for _ in 0..indent {
            s.push_str(INDENT);
        }
        s.push_str("}\n");
    }

    fn args_at(&self, args: &[CallArg], indent: usize) -> String {
        let parts: Vec<String> = args
            .iter()
            .map(|a| {
                if a.spread {
                    format!("...{}", self.expr_at(&a.value, indent))
                } else {
                    match &a.name {
                        Some(n) => format!("{n}: {}", self.expr_at(&a.value, indent)),
                        None => self.expr_at(&a.value, indent),
                    }
                }
            })
            .collect();
        parts.join(", ")
    }

    fn block_inline(&self, block: &Block) -> String {
        let parts: Vec<String> = block
            .iter()
            .map(|s_node| self.stmt_inline(&s_node.kind))
            .collect();
        parts.join("; ")
    }

    fn stmt_inline(&self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Expr(e) => self.expr_str(e),
            Stmt::Return(Some(e)) => format!("return {}", self.expr_str(e)),
            Stmt::Return(None) => "return".into(),
            Stmt::Let { binding, ty, value } => {
                let ty_str = ty
                    .as_ref()
                    .map(|ty_node| format!(": {}", self.type_expr_str(&ty_node.kind)))
                    .unwrap_or_default();
                format!(
                    "{}{ty_str} = {}",
                    binding_str(binding),
                    self.expr_str(value)
                )
            }
            Stmt::SelfAssign { field, value, .. } => {
                format!("self.{field} = {}", self.expr_str(value))
            }
            Stmt::AugAssign { name, op, rhs, .. } => {
                format!("{name} {}= {}", binop_str(*op), self.expr_str(rhs))
            }
            Stmt::Raise(e) => format!("raise {}", self.expr_str(e)),
            Stmt::Break => "break".into(),
            Stmt::Continue => "continue".into(),
            _ => "...".into(), // fallback for complex stmts inline
        }
    }

    fn string_lit(&self, parts: &[StringPart]) -> String {
        let mut s = String::from("\"");
        for p in parts {
            match p {
                StringPart::Literal(t) => {
                    for ch in t.chars() {
                        match ch {
                            '\\' => s.push_str("\\\\"),
                            '"' => s.push_str("\\\""),
                            '\n' => s.push_str("\\n"),
                            '\t' => s.push_str("\\t"),
                            '\r' => s.push_str("\\r"),
                            '{' => s.push_str("\\{"),
                            '}' => s.push_str("\\}"),
                            c => s.push(c),
                        }
                    }
                }
                StringPart::Interpolation(e, spec) => {
                    s.push('{');
                    s.push_str(&self.expr_str(e));
                    if let Some(sp) = spec {
                        s.push(':');
                        s.push_str(sp);
                    }
                    s.push('}');
                }
                // Preserve the raw slot text so `keel fmt` round-trips
                // broken source without data loss.
                StringPart::ParseError(raw) => {
                    s.push('{');
                    s.push_str(raw);
                    s.push('}');
                }
            }
        }
        s.push('"');
        s
    }

    #[allow(clippy::only_used_in_recursion)]
    fn type_expr_str(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(n) => n.clone(),
            TypeExpr::Nullable(inner) => format!("{}?", self.type_expr_str(inner)),
            TypeExpr::List(inner) => format!("list[{}]", self.type_expr_str(inner)),
            TypeExpr::Map(k, v) => {
                format!("map[{}, {}]", self.type_expr_str(k), self.type_expr_str(v))
            }
            TypeExpr::Set(inner) => format!("set[{}]", self.type_expr_str(inner)),
            TypeExpr::Struct(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, self.type_expr_str(&f.ty.kind)))
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            }
            TypeExpr::Tuple(items) => {
                let parts: Vec<String> = items.iter().map(|t| self.type_expr_str(t)).collect();
                format!("({})", parts.join(", "))
            }
            TypeExpr::Func(params, ret) => {
                let parts: Vec<String> = params.iter().map(|t| self.type_expr_str(t)).collect();
                format!("({}) -> {}", parts.join(", "), self.type_expr_str(ret))
            }
            TypeExpr::Generic(name, args) => {
                let parts: Vec<String> = args.iter().map(|t| self.type_expr_str(t)).collect();
                format!("{name}[{}]", parts.join(", "))
            }
            TypeExpr::Dynamic => "dynamic".into(),
            TypeExpr::SelfType => "self".into(),
        }
    }
}

/// Emit a struct/map key as a bare identifier when it's a valid ident,
/// or as a quoted string literal when it contains spaces or other
/// non-identifier characters.
fn map_key_form(k: &crate::ast::MapLitKey) -> String {
    use crate::ast::MapLitKey;
    match k {
        MapLitKey::Ident(s) => s.clone(),
        MapLitKey::Int(n) => n.to_string(),
        MapLitKey::Bool(b) => b.to_string(),
        MapLitKey::Str(s) => {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\r' => out.push_str("\\r"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
    }
}

fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Neq => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Lte => "<=",
        BinOp::Gte => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

#[cfg(test)]
mod tests {
    use super::format_program;
    use crate::lexer::lex;
    use crate::parser::parse;
    use miette::NamedSource;
    use std::path::PathBuf;

    fn project_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn format_source(src: &str) -> String {
        let named = NamedSource::new("t.keel", src.to_string());
        let tokens = lex(src, &named).expect("lex");
        let program = parse(tokens, src.len(), &named).expect("parse");
        format_program(&program)
    }

    fn assert_idempotent(src: &str) {
        let once = format_source(src);
        let twice = format_source(&once);
        assert_eq!(
            once, twice,
            "formatter not idempotent.\n--- once ---\n{once}\n--- twice ---\n{twice}"
        );
    }

    #[test]
    fn format_minimal_program() {
        let src = r#"agent G {
  @role "hi"
}
run(G)
"#;
        let out = format_source(src);
        assert!(out.contains("agent G {"), "output:\n{out}");
        assert!(out.contains("@role"));
        assert!(out.contains("run(G)"));
    }

    #[test]
    fn idempotent_on_every_example() {
        let examples_dir = project_root().join("examples");
        let mut count = 0;
        for entry in std::fs::read_dir(&examples_dir).expect("read examples dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.extension().map(|e| e != "keel").unwrap_or(true) {
                continue;
            }
            let src = std::fs::read_to_string(&path).expect("read keel file");
            let once = format_source(&src);
            let twice = format_source(&once);
            assert_eq!(
                once,
                twice,
                "formatter not idempotent on {}\n--- once ---\n{once}\n--- twice ---\n{twice}",
                path.display()
            );
            count += 1;
        }
        assert!(count > 5, "expected many example files, found {count}");
    }

    #[test]
    fn idempotent_rich_enum_construction() {
        assert_idempotent(
            r#"
type Action =
  | reply { to: str, tone: str }
  | archive

task t() {
  a = Action.reply { to: "x", tone: "y" }
}
"#,
        );
    }

    #[test]
    fn idempotent_when_arms() {
        assert_idempotent(
            r#"
type U = low | medium | high

task t(u: U) -> str {
  when u {
    low => "lo"
    medium, high => "hi"
  }
}
"#,
        );
    }

    #[test]
    fn idempotent_nested_blocks() {
        assert_idempotent(
            r#"
agent Bot {
  @role "..."

  @on_start {
    if true {
      Io.notify("a")
    } else {
      Io.notify("b")
    }
    for x in [1, 2, 3] {
      Io.notify(x.to_str())
    }
  }
}

run(Bot)
"#,
        );
    }

    #[test]
    fn idempotent_format_specifiers() {
        assert_idempotent(
            r#"
agent A {
  @on_start {
    pi = 3.14159
    n = 7
    s = "hello"
    Io.show("{pi:.2f}")
    Io.show("{n:>10}")
    Io.show("{s:<10}")
    Io.show("{s:^10}")
    Io.show("{n:>10.2f}")
  }
}

run(A)
"#,
        );
    }

    #[test]
    fn malformed_interpolation_with_spec_round_trips() {
        let src = "task go() { x = \"{1 +:>10}\" }\n";
        let formatted = format_source(src);
        assert!(
            formatted.contains("{1 +:>10}"),
            "format spec lost from broken slot; got: {formatted:?}"
        );
        let twice = format_source(&formatted);
        assert_eq!(
            formatted, twice,
            "formatter not idempotent on broken slot with spec"
        );
    }

    #[test]
    fn underscore_ident_not_mangled_by_formatter() {
        let src = "task go(x1_2: int) -> str { \"{x1_2}\" }\n";
        let formatted = format_source(src);
        assert!(
            formatted.contains("x1_2"),
            "underscore identifier mangled by formatter; got: {formatted:?}"
        );
        assert_idempotent(src);
    }
}
