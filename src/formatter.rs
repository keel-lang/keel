use crate::ast::*;

const INDENT: &str = "  ";

pub fn format_program(program: &Program) -> String {
    let mut out = String::new();
    for (i, (decl, _)) in program.declarations.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_decl(&mut out, decl, 0);
        out.push('\n');
    }
    out
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str(INDENT);
    }
}

// ---------------------------------------------------------------------------
// Declarations
// ---------------------------------------------------------------------------

fn format_decl(out: &mut String, decl: &Decl, level: usize) {
    match decl {
        Decl::Type(td) => format_type_decl(out, td, level),
        Decl::Connect(cd) => format_connect(out, cd, level),
        Decl::Task(td) => format_task(out, td, level),
        Decl::Agent(ad) => format_agent(out, ad, level),
        Decl::Run(rs) => {
            indent(out, level);
            out.push_str(&format!("run {}", rs.agent));
            if rs.background {
                out.push_str(" in background");
            }
        }
    }
}

fn format_type_decl(out: &mut String, td: &TypeDecl, level: usize) {
    indent(out, level);
    match &td.def {
        TypeDef::SimpleEnum(variants) => {
            out.push_str(&format!("type {} = {}", td.name, variants.join(" | ")));
        }
        TypeDef::RichEnum(variants) => {
            out.push_str(&format!("type {} =\n", td.name));
            for (i, v) in variants.iter().enumerate() {
                indent(out, level + 1);
                out.push_str(&format!("| {}", v.name));
                if let Some(fields) = &v.fields {
                    out.push_str(" { ");
                    for (j, f) in fields.iter().enumerate() {
                        if j > 0 {
                            out.push_str(", ");
                        }
                        out.push_str(&format!("{}: {}", f.name, format_type_expr(&f.ty)));
                    }
                    out.push_str(" }");
                }
                if i < variants.len() - 1 {
                    out.push('\n');
                }
            }
        }
        TypeDef::Struct(fields) => {
            out.push_str(&format!("type {} {{\n", td.name));
            for f in fields {
                indent(out, level + 1);
                out.push_str(&format!("{}: {}\n", f.name, format_type_expr(&f.ty)));
            }
            indent(out, level);
            out.push('}');
        }
    }
}

fn format_connect(out: &mut String, cd: &ConnectDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!("connect {} via {}", cd.name, cd.protocol));
    if !cd.config.is_empty() {
        out.push_str(" {\n");
        for (key, val) in &cd.config {
            indent(out, level + 1);
            out.push_str(&format!("{}: {},\n", key, format_expr(val)));
        }
        indent(out, level);
        out.push('}');
    }
}

fn format_task(out: &mut String, td: &TaskDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!("task {}(", td.name));
    for (i, p) in td.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{}: {}", p.name, format_type_expr(&p.ty)));
        if let Some(default) = &p.default {
            out.push_str(&format!(" = {}", format_expr(default)));
        }
    }
    out.push(')');
    if let Some(ret) = &td.return_type {
        out.push_str(&format!(" -> {}", format_type_expr(ret)));
    }
    out.push_str(" {\n");
    format_block(out, &td.body, level + 1);
    indent(out, level);
    out.push('}');
}

fn format_agent(out: &mut String, ad: &AgentDecl, level: usize) {
    indent(out, level);
    out.push_str(&format!("agent {} {{\n", ad.name));
    for (i, item) in ad.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_agent_item(out, item, level + 1);
        out.push('\n');
    }
    indent(out, level);
    out.push('}');
}

fn format_agent_item(out: &mut String, item: &AgentItem, level: usize) {
    match item {
        AgentItem::Role(r) => {
            indent(out, level);
            out.push_str(&format!("role \"{}\"", escape_str(r)));
        }
        AgentItem::Model(m) => {
            indent(out, level);
            out.push_str(&format!("model \"{}\"", m));
        }
        AgentItem::Tools(tools) => {
            indent(out, level);
            out.push_str(&format!("tools [{}]", tools.join(", ")));
        }
        AgentItem::Memory(mode) => {
            indent(out, level);
            let m = match mode {
                MemoryMode::None_ => "none",
                MemoryMode::Session => "session",
                MemoryMode::Persistent => "persistent",
            };
            out.push_str(&format!("memory {m}"));
        }
        AgentItem::State(fields) => {
            indent(out, level);
            out.push_str("state {\n");
            for f in fields {
                indent(out, level + 1);
                out.push_str(&format!(
                    "{}: {} = {}\n",
                    f.name,
                    format_type_expr(&f.ty),
                    format_expr(&f.default)
                ));
            }
            indent(out, level);
            out.push('}');
        }
        AgentItem::Config(entries) => {
            indent(out, level);
            out.push_str("config {\n");
            for (key, val) in entries {
                indent(out, level + 1);
                out.push_str(&format!("{}: {},\n", key, format_expr(val)));
            }
            indent(out, level);
            out.push('}');
        }
        AgentItem::Task(td) => format_task(out, td, level),
        AgentItem::Every(every) => {
            indent(out, level);
            out.push_str(&format!("every {} {{\n", format_expr(&every.interval)));
            format_block(out, &every.body, level + 1);
            indent(out, level);
            out.push('}');
        }
        AgentItem::On(handler) => {
            indent(out, level);
            out.push_str(&format!("on {}", handler.event));
            if let Some(p) = &handler.param {
                out.push_str(&format!("({}: {})", p.name, format_type_expr(&p.ty)));
            }
            out.push_str(" {\n");
            format_block(out, &handler.body, level + 1);
            indent(out, level);
            out.push('}');
        }
        AgentItem::Team(agents) => {
            indent(out, level);
            out.push_str(&format!("team [{}]", agents.join(", ")));
        }
        AgentItem::Rules(exprs) => {
            indent(out, level);
            out.push_str("rules [");
            for (i, e) in exprs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format_expr(e));
            }
            out.push(']');
        }
    }
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

fn format_block(out: &mut String, block: &Block, level: usize) {
    for (stmt, _) in block {
        format_stmt(out, stmt, level);
        out.push('\n');
    }
}

fn format_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    match stmt {
        Stmt::Let { name, ty, value } => {
            out.push_str(name);
            if let Some(t) = ty {
                out.push_str(&format!(": {}", format_type_expr(t)));
            }
            out.push_str(&format!(" = {}", format_expr(value)));
        }
        Stmt::SelfAssign { field, value } => {
            out.push_str(&format!("self.{} = {}", field, format_expr(value)));
        }
        Stmt::Expr(expr) => {
            out.push_str(&format_expr(expr));
        }
        Stmt::Return(expr) => {
            out.push_str("return");
            if let Some(e) = expr {
                out.push_str(&format!(" {}", format_expr(e)));
            }
        }
        Stmt::For {
            binding,
            iter,
            filter,
            body,
        } => {
            out.push_str(&format!("for {} in {}", binding, format_expr(iter)));
            if let Some(pred) = filter {
                out.push_str(&format!(" where {}", format_expr(pred)));
            }
            out.push_str(" {\n");
            format_block(out, body, level + 1);
            indent(out, level);
            out.push('}');
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            out.push_str(&format!("if {} {{\n", format_expr(cond)));
            format_block(out, then_body, level + 1);
            indent(out, level);
            out.push('}');
            if let Some(else_b) = else_body {
                out.push_str(" else {\n");
                format_block(out, else_b, level + 1);
                indent(out, level);
                out.push('}');
            }
        }
        Stmt::When { subject, arms } => {
            out.push_str(&format!("when {} {{\n", format_expr(subject)));
            for arm in arms {
                indent(out, level + 1);
                let pats: Vec<String> = arm.patterns.iter().map(format_pattern).collect();
                out.push_str(&pats.join(", "));
                if let Some(guard) = &arm.guard {
                    out.push_str(&format!(" where {}", format_expr(guard)));
                }
                out.push_str(" => ");
                if arm.body.len() == 1 {
                    // Single-expression arm
                    if let Stmt::Expr(e) = &arm.body[0].0 {
                        out.push_str(&format_expr(e));
                        out.push('\n');
                        continue;
                    }
                }
                out.push_str("{\n");
                format_block(out, &arm.body, level + 2);
                indent(out, level + 1);
                out.push_str("}\n");
            }
            indent(out, level);
            out.push('}');
        }
        Stmt::Notify { message } => {
            out.push_str(&format!("notify user {}", format_expr(message)));
        }
        Stmt::Show { value } => {
            out.push_str(&format!("show user {}", format_expr(value)));
        }
        Stmt::Send { value, target } => {
            out.push_str(&format!(
                "send {} to {}",
                format_expr(value),
                format_expr(target)
            ));
        }
        Stmt::Archive { value } => {
            out.push_str(&format!("archive {}", format_expr(value)));
        }
        Stmt::ConfirmThen {
            message,
            then_body,
        } => {
            out.push_str(&format!("confirm user {} then ", format_expr(message)));
            if then_body.len() == 1 {
                if let Stmt::Send { value, target } = &then_body[0].0 {
                    out.push_str(&format!(
                        "send {} to {}",
                        format_expr(value),
                        format_expr(target)
                    ));
                    return;
                }
            }
            out.push_str("{\n");
            format_block(out, then_body, level + 1);
            indent(out, level);
            out.push('}');
        }
        Stmt::Remember { value } => {
            out.push_str(&format!("remember {}", format_expr(value)));
        }
        Stmt::After { delay, body } => {
            out.push_str(&format!("after {} {{\n", format_expr(delay)));
            format_block(out, body, level + 1);
            indent(out, level);
            out.push('}');
        }
        Stmt::Wait { duration, condition } => {
            if let Some(dur) = duration {
                out.push_str(&format!("wait {}", format_expr(dur)));
            } else if let Some(cond) = condition {
                out.push_str(&format!("wait until {}", format_expr(cond)));
            }
        }
        Stmt::Retry {
            count,
            backoff,
            body,
        } => {
            out.push_str(&format!("retry {} times", format_expr(count)));
            if *backoff {
                out.push_str(" with backoff");
            }
            out.push_str(" {\n");
            format_block(out, body, level + 1);
            indent(out, level);
            out.push('}');
        }
        Stmt::TryCatch { body, catches } => {
            out.push_str("try {\n");
            format_block(out, body, level + 1);
            indent(out, level);
            out.push('}');
            for clause in catches {
                out.push_str(&format!(
                    " catch {}: {} {{\n",
                    clause.name,
                    format_type_expr(&clause.ty)
                ));
                format_block(out, &clause.body, level + 1);
                indent(out, level);
                out.push('}');
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Integer(n) => n.to_string(),
        Expr::Float(n) => n.to_string(),
        Expr::Bool(b) => b.to_string(),
        Expr::None_ => "none".to_string(),
        Expr::Now => "now".to_string(),
        Expr::StringLit(parts) => {
            let mut s = String::from('"');
            for part in parts {
                match part {
                    StringPart::Literal(lit) => s.push_str(&escape_str(lit)),
                    StringPart::Interpolation(e) => {
                        s.push('{');
                        s.push_str(&format_expr(e));
                        s.push('}');
                    }
                }
            }
            s.push('"');
            s
        }
        Expr::Ident(name) => name.clone(),
        Expr::FieldAccess(obj, field) => format!("{}.{}", format_expr(obj), field),
        Expr::NullFieldAccess(obj, field) => format!("{}?.{}", format_expr(obj), field),
        Expr::NullAssert(inner) => format!("{}!", format_expr(inner)),
        Expr::EnvAccess(var) => format!("env.{var}"),
        Expr::SelfAccess(field) => format!("self.{field}"),
        Expr::StructLit(fields) => {
            let inner: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", k, format_expr(v)))
                .collect();
            format!("{{{}}}", inner.join(", "))
        }
        Expr::ListLit(items) => {
            let inner: Vec<String> = items.iter().map(|e| format_expr(e)).collect();
            format!("[{}]", inner.join(", "))
        }
        Expr::BinaryOp { left, op, right } => {
            let op_str = match op {
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
            };
            format!("{} {} {}", format_expr(left), op_str, format_expr(right))
        }
        Expr::UnaryOp { op, expr } => {
            let op_str = match op {
                UnOp::Neg => "-",
                UnOp::Not => "not ",
            };
            format!("{}{}", op_str, format_expr(expr))
        }
        Expr::NullCoalesce(left, right) => {
            format!("{} ?? {}", format_expr(left), format_expr(right))
        }
        Expr::Pipeline(left, right) => {
            format!("{} |> {}", format_expr(left), format_expr(right))
        }
        Expr::Call { callee, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_call_arg).collect();
            format!("{}({})", format_expr(callee), arg_strs.join(", "))
        }
        Expr::MethodCall {
            object,
            method,
            args,
        } => {
            let arg_strs: Vec<String> = args.iter().map(format_call_arg).collect();
            format!("{}.{}({})", format_expr(object), method, arg_strs.join(", "))
        }
        Expr::Classify {
            input,
            target,
            criteria,
            fallback,
            model,
        } => {
            let mut s = format!("classify {} as ", format_expr(input));
            match target {
                ClassifyTarget::Named(name) => s.push_str(name),
                ClassifyTarget::Inline(variants) => {
                    s.push_str(&format!("[{}]", variants.join(", ")));
                }
            }
            if let Some(criteria) = criteria {
                s.push_str(" considering [\n");
                for (desc, variant) in criteria {
                    s.push_str(&format!("    \"{}\" => {},\n", desc, variant));
                }
                s.push_str("  ]");
            }
            if let Some(fb) = fallback {
                s.push_str(&format!(" fallback {}", format_expr(fb)));
            }
            if let Some(m) = model {
                s.push_str(&format!(" using \"{}\"", m));
            }
            s
        }
        Expr::Summarize {
            input,
            length,
            format: fmt,
            fallback,
            model,
        } => {
            let mut s = format!("summarize {}", format_expr(input));
            if let Some((n, unit)) = length {
                s.push_str(&format!(" in {} {}", n, unit));
            }
            if let Some(f) = fmt {
                s.push_str(&format!(" format {}", f));
            }
            if let Some(fb) = fallback {
                s.push_str(&format!(" fallback {}", format_expr(fb)));
            }
            if let Some(m) = model {
                s.push_str(&format!(" using \"{}\"", m));
            }
            s
        }
        Expr::Draft {
            description,
            options,
            model,
        } => {
            let mut s = format!("draft {}", format_expr(description));
            if !options.is_empty() {
                s.push_str(" {\n");
                for (key, val) in options {
                    s.push_str(&format!("    {}: {},\n", key, format_expr(val)));
                }
                s.push_str("  }");
            }
            if let Some(m) = model {
                s.push_str(&format!(" using \"{}\"", m));
            }
            s
        }
        Expr::Ask { prompt, options } => {
            let mut s = format!("ask user {}", format_expr(prompt));
            if let Some(opts) = options {
                s.push_str(&format!(" options {}", format_expr(opts)));
            }
            s
        }
        Expr::Confirm { message } => {
            format!("confirm user {}", format_expr(message))
        }
        Expr::Fetch { source, filter } => {
            let mut s = format!("fetch {}", format_expr(source));
            if let Some(f) = filter {
                s.push_str(&format!(" where {}", format_expr(f)));
            }
            s
        }
        Expr::Recall { query, limit } => {
            let mut s = format!("recall {}", format_expr(query));
            if let Some(n) = limit {
                s.push_str(&format!(" limit {}", n));
            }
            s
        }
        Expr::Delegate { task_call, agent } => {
            format!("delegate {} to {}", format_expr(task_call), agent)
        }
        Expr::IfExpr {
            cond,
            then_body,
            else_body,
        } => {
            let mut s = format!("if {} {{\n", format_expr(cond));
            format_block(&mut s, then_body, 1);
            s.push_str("} else {\n");
            format_block(&mut s, else_body, 1);
            s.push('}');
            s
        }
        Expr::WhenExpr { subject, arms } => {
            // Reuse the Stmt::When path by building a temporary stmt string.
            let mut s = format!("when {} {{\n", format_expr(subject));
            for arm in arms {
                indent(&mut s, 1);
                let pats: Vec<String> = arm.patterns.iter().map(format_pattern).collect();
                s.push_str(&pats.join(", "));
                if let Some(guard) = &arm.guard {
                    s.push_str(&format!(" where {}", format_expr(guard)));
                }
                s.push_str(" => ");
                if arm.body.len() == 1 {
                    if let Stmt::Expr(e) = &arm.body[0].0 {
                        s.push_str(&format_expr(e));
                        s.push('\n');
                        continue;
                    }
                }
                s.push_str("{\n");
                format_block(&mut s, &arm.body, 2);
                indent(&mut s, 1);
                s.push_str("}\n");
            }
            s.push('}');
            s
        }
        Expr::Lambda { params, body } => {
            if params.len() == 1 {
                format!("{} => {}", params[0].name, format_expr(body))
            } else {
                let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                format!("({}) => {}", names.join(", "), format_expr(body))
            }
        }
        Expr::Duration { value, unit } => {
            let u = match unit {
                DurationUnit::Seconds => "seconds",
                DurationUnit::Minutes => "minutes",
                DurationUnit::Hours => "hours",
                DurationUnit::Days => "days",
            };
            format!("{}.{}", format_expr(value), u)
        }
        Expr::EnumVariant(ty, var) => format!("{ty}.{var}"),
        Expr::Extract { schema, source, model } => {
            let fields: Vec<String> = schema
                .iter()
                .map(|f| format!("{}: {}", f.name, format_type_expr(&f.ty)))
                .collect();
            let mut s = format!("extract {{{}}} from {}", fields.join(", "), format_expr(source));
            if let Some(m) = model {
                s.push_str(&format!(" using \"{}\"", m));
            }
            s
        }
        Expr::Translate {
            input,
            target_lang,
            model,
        } => {
            let mut s = format!("translate {} to ", format_expr(input));
            if target_lang.len() == 1 {
                s.push_str(&target_lang[0]);
            } else {
                s.push_str(&format!("[{}]", target_lang.join(", ")));
            }
            if let Some(m) = model {
                s.push_str(&format!(" using \"{}\"", m));
            }
            s
        }
        Expr::Decide {
            input,
            options,
            model,
        } => {
            let mut s = format!("decide {} {{", format_expr(input));
            for (key, val) in options {
                s.push_str(&format!(" {}: {},", key, format_expr(val)));
            }
            s.push_str(" }");
            if let Some(m) = model {
                s.push_str(&format!(" using \"{}\"", m));
            }
            s
        }
        Expr::Prompt {
            config,
            target_type,
        } => {
            let mut s = "prompt {\n".to_string();
            for (key, val) in config {
                s.push_str(&format!("    {}: {},\n", key, format_expr(val)));
            }
            s.push_str(&format!("  }} as {}", target_type));
            s
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_call_arg(arg: &CallArg) -> String {
    if let Some(name) = &arg.name {
        format!("{}: {}", name, format_expr(&arg.value))
    } else {
        format_expr(&arg.value)
    }
}

fn format_pattern(pat: &Pattern) -> String {
    match pat {
        Pattern::Wildcard => "_".to_string(),
        Pattern::Ident(name) => name.clone(),
        Pattern::Literal(expr) => format_expr(expr),
        Pattern::Variant { name, bindings } => {
            if bindings.is_empty() {
                name.clone()
            } else {
                format!("{} {{ {} }}", name, bindings.join(", "))
            }
        }
    }
}

fn format_type_expr(ty: &TypeExpr) -> String {
    match ty {
        TypeExpr::Named(name) => name.clone(),
        TypeExpr::Nullable(inner) => format!("{}?", format_type_expr(inner)),
        TypeExpr::List(inner) => format!("list[{}]", format_type_expr(inner)),
        TypeExpr::Map(k, v) => format!("map[{}, {}]", format_type_expr(k), format_type_expr(v)),
        TypeExpr::Set(inner) => format!("set[{}]", format_type_expr(inner)),
        TypeExpr::Struct(fields) => {
            let fs: Vec<String> = fields
                .iter()
                .map(|f| format!("{}: {}", f.name, format_type_expr(&f.ty)))
                .collect();
            format!("{{{}}}", fs.join(", "))
        }
        TypeExpr::Tuple(types) => {
            let ts: Vec<String> = types.iter().map(|t| format_type_expr(t)).collect();
            format!("({})", ts.join(", "))
        }
        TypeExpr::Func(params, ret) => {
            let ps: Vec<String> = params.iter().map(|t| format_type_expr(t)).collect();
            format!("({}) -> {}", ps.join(", "), format_type_expr(ret))
        }
    }
}

fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}
