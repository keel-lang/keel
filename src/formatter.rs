//! Formatter — placeholder for v0.1.
//!
//! `keel fmt` lands in a follow-up change; for now this emits a placeholder
//! message.

use crate::ast::*;

pub fn format_program(program: &Program) -> String {
    let mut f = Fmt::new();
    for (i, (decl, _)) in program.declarations.iter().enumerate() {
        if i > 0 {
            f.blank_line();
        }
        f.decl(decl);
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
        Fmt { buf: String::new(), indent: 0, at_line_start: true }
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

    fn indent(&mut self) { self.indent += 1; }
    fn dedent(&mut self) { if self.indent > 0 { self.indent -= 1; } }

    // -----------------------------------------------------------------
    // Declarations
    // -----------------------------------------------------------------

    fn decl(&mut self, decl: &Decl) {
        match decl {
            Decl::Type(t) => self.type_decl(t),
            Decl::Interface(i) => self.interface_decl(i),
            Decl::Task(t) => self.task_decl(t),
            Decl::Extern(e) => self.extern_decl(e),
            Decl::Agent(a) => self.agent_decl(a),
            Decl::Use(u) => self.use_decl(u),
            Decl::Stmt((stmt, _)) => {
                self.stmt(stmt);
                self.newline();
            }
        }
    }

    fn type_decl(&mut self, t: &TypeDecl) {
        match &t.def {
            TypeDef::SimpleEnum(variants) => {
                self.push(&format!("type {} = ", t.name));
                self.push(&variants.join(" | "));
                self.newline();
            }
            TypeDef::RichEnum(variants) => {
                self.push(&format!("type {} =", t.name));
                self.newline();
                self.indent();
                for v in variants {
                    self.push("| ");
                    self.push(&v.name);
                    if let Some(fields) = &v.fields {
                        self.push(" { ");
                        for (i, f) in fields.iter().enumerate() {
                            if i > 0 { self.push(", "); }
                            self.push(&format!("{}: ", f.name));
                            self.push(&self.type_expr_str(&f.ty));
                        }
                        self.push(" }");
                    }
                    self.newline();
                }
                self.dedent();
            }
            TypeDef::Struct(fields) => {
                self.push(&format!("type {} {{", t.name));
                self.newline();
                self.indent();
                for f in fields {
                    self.push(&format!("{}: ", f.name));
                    self.push(&self.type_expr_str(&f.ty));
                    self.newline();
                }
                self.dedent();
                self.push("}");
                self.newline();
            }
            TypeDef::Alias(ty) => {
                self.push(&format!("type {} = ", t.name));
                self.push(&self.type_expr_str(ty));
                self.newline();
            }
        }
    }

    fn interface_decl(&mut self, i: &InterfaceDecl) {
        self.push(&format!("interface {} {{", i.name));
        self.newline();
        self.indent();
        for method in &i.methods {
            self.push(&format!("task {}(", method.name));
            self.params(&method.params);
            self.push(")");
            if let Some(ret) = &method.return_type {
                self.push(" -> ");
                self.push(&self.type_expr_str(ret));
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
        self.push(&self.type_expr_str(&e.return_type));
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
        self.push(&format!("task {}(", t.name));
        self.params(&t.params);
        self.push(")");
        if let Some(ret) = &t.return_type {
            self.push(" -> ");
            self.push(&self.type_expr_str(ret));
        }
        self.push(" {");
        self.newline();
        self.indent();
        for (stmt, _) in &t.body {
            self.stmt(stmt);
            self.newline();
        }
        self.dedent();
        self.push("}");
        self.newline();
    }

    fn params(&mut self, params: &[Param]) {
        for (i, p) in params.iter().enumerate() {
            if i > 0 { self.push(", "); }
            self.push(&format!("{}: ", p.name));
            self.push(&self.type_expr_str(&p.ty));
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
                    AttributeBody::Block(body) => {
                        self.push("{");
                        self.newline();
                        self.indent();
                        for (s, _) in body {
                            self.stmt(s);
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
                    self.push(&self.type_expr_str(&f.ty));
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
                    self.push(&format!("({}: ", p.name));
                    self.push(&self.type_expr_str(&p.ty));
                    self.push(")");
                } else {
                    self.push("()");
                }
                self.push(" {");
                self.newline();
                self.indent();
                for (s, _) in &h.body {
                    self.stmt(s);
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
            Stmt::Let { name, ty, value } => {
                self.push(name);
                if let Some(t) = ty {
                    self.push(": ");
                    self.push(&self.type_expr_str(t));
                }
                self.push(" = ");
                self.push(&self.expr_str(value));
            }
            Stmt::SelfAssign { field, value } => {
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
            Stmt::For { binding, iter, filter, body } => {
                self.push(&format!("for {binding} in "));
                self.push(&self.expr_str(iter));
                if let Some(pred) = filter {
                    self.push(" where ");
                    self.push(&self.expr_str(pred));
                }
                self.push(" {");
                self.newline();
                self.indent();
                for (s, _) in body {
                    self.stmt(s);
                    self.newline();
                }
                self.dedent();
                self.push("}");
            }
            Stmt::If { cond, then_body, else_body } => {
                self.push("if ");
                self.push(&self.expr_str(cond));
                self.push(" {");
                self.newline();
                self.indent();
                for (s, _) in then_body {
                    self.stmt(s);
                    self.newline();
                }
                self.dedent();
                self.push("}");
                if let Some(eb) = else_body {
                    self.push(" else {");
                    self.newline();
                    self.indent();
                    for (s, _) in eb {
                        self.stmt(s);
                        self.newline();
                    }
                    self.dedent();
                    self.push("}");
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
                for (s, _) in body {
                    self.stmt(s);
                    self.newline();
                }
                self.dedent();
                self.push("}");
                for c in catches {
                    self.push(&format!(" catch {}: ", c.name));
                    self.push(&self.type_expr_str(&c.ty));
                    self.push(" {");
                    self.newline();
                    self.indent();
                    for (s, _) in &c.body {
                        self.stmt(s);
                        self.newline();
                    }
                    self.dedent();
                    self.push("}");
                }
            }
            Stmt::Expr(e) => {
                self.push(&self.expr_str(e));
            }
        }
    }

    fn when_arm(&mut self, arm: &WhenArm) {
        for (i, p) in arm.patterns.iter().enumerate() {
            if i > 0 { self.push(", "); }
            self.push(&self.pattern_str(p));
        }
        if let Some(g) = &arm.guard {
            self.push(" where ");
            self.push(&self.expr_str(g));
        }
        self.push(" => ");
        // Single-expression arms stay inline; multi-stmt arms open a block.
        if arm.body.len() == 1 {
            if let Stmt::Expr(e) = &arm.body[0].0 {
                self.push(&self.expr_str(e));
                self.newline();
                return;
            }
        }
        self.push("{");
        self.newline();
        self.indent();
        for (s, _) in &arm.body {
            self.stmt(s);
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

    fn expr_str(&self, expr: &Expr) -> String {
        self.expr_at(expr, self.indent)
    }

    fn expr_at(&self, expr: &Expr, indent: usize) -> String {
        match expr {
            Expr::Integer(n) => n.to_string(),
            Expr::Float(f) => {
                let s = f.to_string();
                if s.contains('.') { s } else { format!("{s}.0") }
            }
            Expr::Bool(b) => b.to_string(),
            Expr::None_ => "none".into(),
            Expr::Now => "now".into(),
            Expr::StringLit(parts) => self.string_lit(parts),
            Expr::Ident(name) => name.clone(),
            Expr::SelfAccess(f) => format!("self.{f}"),
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
                format!("{} {} {}", self.expr_str(left), binop_str(*op), self.expr_str(right))
            }
            Expr::UnaryOp { op, expr } => match op {
                UnOp::Neg => format!("-{}", self.expr_str(expr)),
                UnOp::Not => format!("not {}", self.expr_str(expr)),
            },
            Expr::NullCoalesce(l, r) => format!("{} ?? {}", self.expr_str(l), self.expr_str(r)),
            Expr::Pipeline(l, r) => format!("{} |> {}", self.expr_str(l), self.expr_str(r)),
            Expr::Call { callee, args } => {
                format!("{}({})", self.expr_at(callee, indent), self.args_at(args, indent))
            }
            Expr::MethodCall { object, method, args } => {
                format!("{}.{}({})", self.expr_at(object, indent), method, self.args_at(args, indent))
            }
            Expr::Cast { expr, ty } => format!("{} as {}", self.expr_str(expr), self.type_expr_str(ty)),
            Expr::IfExpr { cond, then_body, else_body } => {
                format!(
                    "if {} {{ {} }} else {{ {} }}",
                    self.expr_str(cond),
                    self.block_inline(then_body),
                    self.block_inline(else_body),
                )
            }
            Expr::WhenExpr { subject, arms } => {
                // Multi-arm when as expr — fall back to the same shape
                // as the statement form but inlined.
                let arms_str: Vec<String> = arms.iter().map(|a| self.arm_inline(a)).collect();
                format!("when {} {{ {} }}", self.expr_str(subject), arms_str.join("; "))
            }
            Expr::Lambda { params, body } => {
                let params_str = if params.len() == 1 && params[0].ty.is_none() {
                    params[0].name.clone()
                } else {
                    let parts: Vec<String> = params.iter().map(|p| {
                        match &p.ty {
                            Some(t) => format!("{}: {}", p.name, self.type_expr_str(t)),
                            None => p.name.clone(),
                        }
                    }).collect();
                    format!("({})", parts.join(", "))
                };
                match body {
                    LambdaBody::Expr(e) => format!("{params_str} => {}", self.expr_at(e, indent)),
                    LambdaBody::Block(b) => self.lambda_block(&params_str, b, indent),
                }
            }
            Expr::Duration { value, unit } => {
                format!("{}.{}", self.expr_str(value), unit.canonical_name())
            }
            Expr::EnumVariant { ty, variant, fields } => {
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
        for (stmt, _) in body {
            for _ in 0..inner_indent { s.push_str(INDENT); }
            self.write_stmt(&mut s, stmt, inner_indent);
            s.push('\n');
        }
        for _ in 0..indent { s.push_str(INDENT); }
        s.push('}');
        s
    }

    /// Write a statement into a string buffer at the given indent.
    /// Mirrors `Fmt::stmt` but outputs to `s` instead of `self.buf`.
    fn write_stmt(&self, s: &mut String, stmt: &Stmt, indent: usize) {
        match stmt {
            Stmt::Expr(e) => s.push_str(&self.expr_at(e, indent)),
            Stmt::Let { name, ty, value } => {
                s.push_str(name);
                if let Some(t) = ty {
                    s.push_str(": ");
                    s.push_str(&self.type_expr_str(t));
                }
                s.push_str(" = ");
                s.push_str(&self.expr_at(value, indent));
            }
            Stmt::SelfAssign { field, value } => {
                s.push_str(&format!("self.{field} = "));
                s.push_str(&self.expr_at(value, indent));
            }
            Stmt::Return(Some(e)) => {
                s.push_str("return ");
                s.push_str(&self.expr_at(e, indent));
            }
            Stmt::Return(None) => s.push_str("return"),
            Stmt::For { binding, iter, filter, body } => {
                s.push_str(&format!("for {binding} in "));
                s.push_str(&self.expr_at(iter, indent));
                if let Some(pred) = filter {
                    s.push_str(" where ");
                    s.push_str(&self.expr_at(pred, indent));
                }
                s.push_str(" {\n");
                self.write_block(s, body, indent + 1);
                for _ in 0..indent { s.push_str(INDENT); }
                s.push('}');
            }
            Stmt::If { cond, then_body, else_body } => {
                s.push_str("if ");
                s.push_str(&self.expr_at(cond, indent));
                s.push_str(" {\n");
                self.write_block(s, then_body, indent + 1);
                for _ in 0..indent { s.push_str(INDENT); }
                s.push('}');
                if let Some(eb) = else_body {
                    s.push_str(" else {\n");
                    self.write_block(s, eb, indent + 1);
                    for _ in 0..indent { s.push_str(INDENT); }
                    s.push('}');
                }
            }
            Stmt::When { subject, arms } => {
                s.push_str("when ");
                s.push_str(&self.expr_at(subject, indent));
                s.push_str(" {\n");
                for arm in arms {
                    for _ in 0..(indent + 1) { s.push_str(INDENT); }
                    self.write_when_arm(s, arm, indent + 1);
                }
                for _ in 0..indent { s.push_str(INDENT); }
                s.push('}');
            }
            Stmt::TryCatch { body, catches } => {
                s.push_str("try {\n");
                self.write_block(s, body, indent + 1);
                for _ in 0..indent { s.push_str(INDENT); }
                s.push('}');
                for c in catches {
                    s.push_str(&format!(" catch {}: ", c.name));
                    s.push_str(&self.type_expr_str(&c.ty));
                    s.push_str(" {\n");
                    self.write_block(s, &c.body, indent + 1);
                    for _ in 0..indent { s.push_str(INDENT); }
                    s.push('}');
                }
            }
        }
    }

    fn write_block(&self, s: &mut String, block: &Block, indent: usize) {
        for (stmt, _) in block {
            for _ in 0..indent { s.push_str(INDENT); }
            self.write_stmt(s, stmt, indent);
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
        if arm.body.len() == 1 {
            if let Stmt::Expr(e) = &arm.body[0].0 {
                s.push_str(&self.expr_at(e, indent));
                s.push('\n');
                return;
            }
        }
        s.push_str("{\n");
        self.write_block(s, &arm.body, indent + 1);
        for _ in 0..indent { s.push_str(INDENT); }
        s.push_str("}\n");
    }

    fn args_at(&self, args: &[CallArg], indent: usize) -> String {
        let parts: Vec<String> = args
            .iter()
            .map(|a| match &a.name {
                Some(n) => format!("{n}: {}", self.expr_at(&a.value, indent)),
                None => self.expr_at(&a.value, indent),
            })
            .collect();
        parts.join(", ")
    }

    fn block_inline(&self, block: &Block) -> String {
        let parts: Vec<String> = block
            .iter()
            .map(|(s, _)| self.stmt_inline(s))
            .collect();
        parts.join("; ")
    }

    fn stmt_inline(&self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Expr(e) => self.expr_str(e),
            Stmt::Return(Some(e)) => format!("return {}", self.expr_str(e)),
            Stmt::Return(None) => "return".into(),
            Stmt::Let { name, ty, value } => {
                let ty_str = ty.as_ref().map(|t| format!(": {}", self.type_expr_str(t))).unwrap_or_default();
                format!("{name}{ty_str} = {}", self.expr_str(value))
            }
            Stmt::SelfAssign { field, value } => format!("self.{field} = {}", self.expr_str(value)),
            _ => "...".into(), // fallback for complex stmts inline
        }
    }

    fn arm_inline(&self, arm: &WhenArm) -> String {
        let pats: Vec<String> = arm.patterns.iter().map(|p| self.pattern_str(p)).collect();
        let body = self.block_inline(&arm.body);
        let guard = arm.guard.as_ref().map(|g| format!(" where {}", self.expr_str(g))).unwrap_or_default();
        format!("{}{guard} => {body}", pats.join(", "))
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
                StringPart::Interpolation(e) => {
                    s.push('{');
                    s.push_str(&self.expr_str(e));
                    s.push('}');
                }
            }
        }
        s.push('"');
        s
    }

    fn type_expr_str(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(n) => n.clone(),
            TypeExpr::Nullable(inner) => format!("{}?", self.type_expr_str(inner)),
            TypeExpr::List(inner) => format!("list[{}]", self.type_expr_str(inner)),
            TypeExpr::Map(k, v) => format!("map[{}, {}]", self.type_expr_str(k), self.type_expr_str(v)),
            TypeExpr::Set(inner) => format!("set[{}]", self.type_expr_str(inner)),
            TypeExpr::Struct(fields) => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, self.type_expr_str(&f.ty)))
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
        }
    }
}

/// Emit a struct/map key as a bare identifier when it's a valid ident,
/// or as a quoted string literal when it contains spaces or other
/// non-identifier characters.
fn map_key_form(k: &str) -> String {
    let is_ident = !k.is_empty()
        && k.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if is_ident {
        k.to_string()
    } else {
        let mut s = String::from("\"");
        for ch in k.chars() {
            match ch {
                '\\' => s.push_str("\\\\"),
                '"' => s.push_str("\\\""),
                '\n' => s.push_str("\\n"),
                '\t' => s.push_str("\\t"),
                '\r' => s.push_str("\\r"),
                c => s.push(c),
            }
        }
        s.push('"');
        s
    }
}

fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/", BinOp::Mod => "%",
        BinOp::Eq => "==", BinOp::Neq => "!=", BinOp::Lt => "<", BinOp::Gt => ">",
        BinOp::Lte => "<=", BinOp::Gte => ">=", BinOp::And => "and", BinOp::Or => "or",
    }
}
