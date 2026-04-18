pub mod environment;
pub mod value;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use colored::Colorize;
use miette::NamedSource;
use serde_json;

use crate::ast::*;
use crate::lexer::Span;
use crate::runtime::Runtime;
use environment::Environment;
use value::Value;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum InterpreterError {
    #[error("{0}")]
    Runtime(String, Option<Span>),
    #[error("Return value")]
    Return(Value),
}

type IResult = Result<Value, InterpreterError>;

fn err(msg: impl Into<String>) -> InterpreterError {
    InterpreterError::Runtime(msg.into(), None)
}

// ---------------------------------------------------------------------------
// Interpreter state
// ---------------------------------------------------------------------------

pub struct Interpreter {
    env: Environment,
    agents: HashMap<String, AgentDecl>,
    tasks: HashMap<String, TaskDecl>,
    types: HashMap<String, TypeDef>,
    connections: HashMap<String, ConnectDecl>,
    agent_state: HashMap<String, HashMap<String, Value>>,
    runtime: Runtime,
    current_agent: Option<String>,
    /// Resolved email connection (if any connect email via imap)
    email_conn: Option<crate::runtime::email::EmailConnection>,
    /// Source for error reporting (set per-input in REPL/script runs)
    named_src: Option<NamedSource<String>>,
}

impl Interpreter {
    pub fn new() -> Self {
        Interpreter {
            env: Environment::new(),
            agents: HashMap::new(),
            tasks: HashMap::new(),
            types: HashMap::new(),
            connections: HashMap::new(),
            agent_state: HashMap::new(),
            runtime: Runtime::new(),
            current_agent: None,
            email_conn: None,
            named_src: None,
        }
    }

    /// Register every declaration from a program without executing `run` statements.
    pub fn register_program(&mut self, program: &Program) {
        for (decl, _span) in &program.declarations {
            self.register_decl(decl);
        }
    }

    /// Register a single declaration (type/task/agent/connect). `run` is ignored here.
    pub fn register_decl(&mut self, decl: &Decl) {
        match decl {
            Decl::Type(td) => {
                if let TypeDef::SimpleEnum(variants) = &td.def {
                    for v in variants {
                        self.env.define(
                            v.clone(),
                            Value::EnumVariant(td.name.clone(), v.clone()),
                        );
                    }
                }
                self.types.insert(td.name.clone(), td.def.clone());
            }
            Decl::Connect(cd) => {
                self.connections.insert(cd.name.clone(), cd.clone());
            }
            Decl::Task(td) => {
                self.tasks.insert(td.name.clone(), td.clone());
                self.env
                    .define(td.name.clone(), Value::Task(td.name.clone(), td.clone()));
            }
            Decl::Agent(ad) => {
                self.agents.insert(ad.name.clone(), ad.clone());
                for item in &ad.items {
                    if let AgentItem::Task(td) = item {
                        self.tasks.insert(td.name.clone(), td.clone());
                        self.env
                            .define(td.name.clone(), Value::Task(td.name.clone(), td.clone()));
                    }
                }
            }
            Decl::Run(_) => {}
        }
    }

    /// Set the source used for error reports.
    pub fn set_source(&mut self, named_src: NamedSource<String>) {
        self.named_src = Some(named_src);
    }

    /// Names of variables defined in the top-level scope.
    pub fn env_var_names(&self) -> Vec<String> {
        self.env.top_scope_names()
    }

    /// Look up a value by name (searches all scopes).
    pub fn env_get(&self, name: &str) -> Option<Value> {
        self.env.get(name).cloned()
    }

    /// Snapshot of registered types (for REPL introspection).
    pub fn types_snapshot(&self) -> Vec<(String, TypeDef)> {
        self.types
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Evaluate REPL statements at the top level (no new scope).
    /// Returns the value of the last expression statement, if any.
    pub async fn eval_repl_stmts(
        &mut self,
        stmts: &[Spanned<Stmt>],
    ) -> miette::Result<Option<Value>> {
        let mut last_expr_val: Option<Value> = None;
        for (stmt, span) in stmts {
            match eval_stmt(self, stmt).await {
                Ok(val) => {
                    last_expr_val = match stmt {
                        Stmt::Expr(_) => Some(val),
                        _ => None,
                    };
                }
                Err(InterpreterError::Return(val)) => {
                    last_expr_val = Some(val);
                }
                Err(InterpreterError::Runtime(msg, err_span)) => {
                    return Err(self.runtime_report(msg, err_span.or_else(|| Some(span.clone()))));
                }
            }
        }
        Ok(last_expr_val)
    }

    fn runtime_report(&self, msg: String, span: Option<Span>) -> miette::Report {
        build_runtime_report(msg, span, self.named_src.clone())
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

fn build_runtime_report(
    msg: String,
    span: Option<Span>,
    named_src: Option<NamedSource<String>>,
) -> miette::Report {
    match (span, named_src) {
        (Some(span), Some(src)) => miette::miette!(
            labels = vec![miette::LabeledSpan::at(span, msg.clone())],
            "Runtime error: {msg}"
        )
        .with_source_code(src),
        _ => miette::miette!("Runtime error: {msg}"),
    }
}

// All async functions use `-> Pin<Box<dyn Future<...>>>` to support recursion.

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn run(program: Program) -> miette::Result<()> {
    run_with_source(program, None).await
}

pub async fn run_with_source(
    program: Program,
    named_src: Option<NamedSource<String>>,
) -> miette::Result<()> {
    let mut interp = Interpreter::new();
    interp.named_src = named_src;

    // First pass: register all declarations
    for (decl, _span) in &program.declarations {
        match decl {
            Decl::Type(td) => {
                if let TypeDef::SimpleEnum(variants) = &td.def {
                    for v in variants {
                        interp.env.define(
                            v.clone(),
                            Value::EnumVariant(td.name.clone(), v.clone()),
                        );
                    }
                }
                interp.types.insert(td.name.clone(), td.def.clone());
            }
            Decl::Connect(cd) => {
                // Try to resolve email connections
                if cd.protocol == "imap" || cd.protocol == "smtp" {
                    // Evaluate config values
                    let mut config_values: Vec<(String, Value)> = Vec::new();
                    for (key, expr) in &cd.config {
                        // For env.VAR references, resolve from environment
                        let val = match expr {
                            Expr::EnvAccess(var) => {
                                Value::String(std::env::var(var).unwrap_or_default())
                            }
                            Expr::StringLit(parts) => {
                                let mut s = String::new();
                                for part in parts {
                                    match part {
                                        StringPart::Literal(lit) => s.push_str(lit),
                                        _ => {}
                                    }
                                }
                                Value::String(s)
                            }
                            _ => Value::String(String::new()),
                        };
                        config_values.push((key.clone(), val));
                    }
                    match crate::runtime::email::EmailConnection::from_config(&config_values) {
                        Ok(conn) => {
                            println!(
                                "  {} Email connection: {} ({})",
                                "✓".bright_green(),
                                cd.name,
                                conn.imap_host.dimmed()
                            );
                            interp.email_conn = Some(conn);
                        }
                        Err(e) => {
                            println!(
                                "  {} Email connection '{}': {}",
                                "⚠".bright_yellow(),
                                cd.name,
                                e.dimmed()
                            );
                        }
                    }
                }
                interp.connections.insert(cd.name.clone(), cd.clone());
            }
            Decl::Task(td) => {
                interp.tasks.insert(td.name.clone(), td.clone());
                interp
                    .env
                    .define(td.name.clone(), Value::Task(td.name.clone(), td.clone()));
            }
            Decl::Agent(ad) => {
                interp.agents.insert(ad.name.clone(), ad.clone());
                for item in &ad.items {
                    if let AgentItem::Task(td) = item {
                        interp.tasks.insert(td.name.clone(), td.clone());
                        interp
                            .env
                            .define(td.name.clone(), Value::Task(td.name.clone(), td.clone()));
                    }
                }
            }
            Decl::Run(_) => {}
        }
    }

    // Second pass: execute run statements
    for (decl, _span) in &program.declarations {
        if let Decl::Run(rs) = decl {
            if let Err(e) = run_agent(&mut interp, &rs.agent).await {
                return Err(match e {
                    InterpreterError::Runtime(msg, span) => {
                        build_runtime_report(msg, span, interp.named_src.clone())
                    }
                    InterpreterError::Return(_) => {
                        miette::miette!("Runtime error: unexpected return")
                    }
                });
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Agent execution
// ---------------------------------------------------------------------------

fn run_agent<'a>(
    interp: &'a mut Interpreter,
    name: &'a str,
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        let agent = interp
            .agents
            .get(name)
            .cloned()
            .ok_or_else(|| err(format!("Agent '{name}' not found")))?;

        interp.current_agent = Some(name.to_string());

        let mut state = HashMap::new();
        let mut role = String::new();
        let mut model = String::from("claude-sonnet");

        for item in &agent.items {
            match item {
                AgentItem::Role(r) => role = r.clone(),
                AgentItem::Model(m) => model = m.clone(),
                AgentItem::State(fields) => {
                    for field in fields {
                        let val = eval_expr(interp, &field.default).await?;
                        state.insert(field.name.clone(), val);
                    }
                }
                _ => {}
            }
        }

        interp.agent_state.insert(name.to_string(), state);
        interp.runtime.set_model(&model);

        let quiet = std::env::var("KEEL_REPL").as_deref() == Ok("1");
        if !quiet {
            println!(
                "{} {} {}",
                "▸".bright_green(),
                "Starting agent".bright_green().bold(),
                name.bright_white().bold()
            );
            if !role.is_empty() {
                println!("  {}: {}", "role".dimmed(), role.dimmed());
            }
            println!("  {}: {}", "model".dimmed(), interp.runtime.describe_model(&model).dimmed());
            println!();
        }

        // Collect every blocks
        let every_blocks: Vec<&EveryBlock> = agent
            .items
            .iter()
            .filter_map(|item| {
                if let AgentItem::Every(every) = item {
                    Some(every)
                } else {
                    None
                }
            })
            .collect();

        if every_blocks.is_empty() {
            interp.current_agent = None;
            return Ok(Value::None);
        }

        // Resolve intervals and run the first tick immediately
        let mut intervals_secs: Vec<f64> = Vec::new();
        for every in &every_blocks {
            let interval = eval_expr(interp, &every.interval).await?;
            let secs = match &interval {
                Value::Duration(s) => *s,
                _ => {
                    return Err(err("every requires a duration"));
                }
            };
            intervals_secs.push(secs);
            if !quiet {
                println!(
                    "  {} polling every {}",
                    "⏱".dimmed(),
                    format!("{interval}").dimmed()
                );
            }

            // Execute body immediately (first tick — errors are fatal)
            eval_block(interp, &every.body).await?;
        }

        // In oneshot mode (tests, CI), skip the scheduling loop
        let oneshot = std::env::var("KEEL_ONESHOT").as_deref() == Ok("1")
            || std::env::var("KEEL_LLM").as_deref() == Ok("mock");
        if oneshot {
            interp.current_agent = None;
            return Ok(Value::None);
        }

        // Schedule recurring execution with tokio
        println!(
            "\n  {} Agent running. Press Ctrl+C to stop.",
            "▸".bright_green()
        );

        let shutdown = tokio::signal::ctrl_c();
        tokio::pin!(shutdown);

        // Create interval timers for each every block
        let mut tick_timers: Vec<tokio::time::Interval> = intervals_secs
            .iter()
            .map(|secs| {
                let dur = std::time::Duration::from_secs_f64(*secs);
                let mut interval = tokio::time::interval(dur);
                // Skip the first tick (already executed above)
                interval.reset();
                interval
            })
            .collect();

        loop {
            // Wait for any timer tick or Ctrl+C
            let tick_index = tokio::select! {
                _ = &mut shutdown => {
                    println!(
                        "\n  {} Stopping agent {}",
                        "■".bright_red(),
                        name.bright_white().bold()
                    );
                    break;
                }
                idx = next_tick(&mut tick_timers) => idx,
            };

            // Execute the corresponding every block body
            if let Some(every) = every_blocks.get(tick_index) {
                if let Err(e) = eval_block(interp, &every.body).await {
                    match e {
                        InterpreterError::Return(_) => {}
                        InterpreterError::Runtime(msg, _span) => {
                            eprintln!("  {} {}", "✗".bright_red(), msg);
                        }
                    }
                }
            }
        }

        interp.current_agent = None;
        Ok(Value::None)
    })
}

// ---------------------------------------------------------------------------
// Block & statement evaluation
// ---------------------------------------------------------------------------

fn eval_block<'a>(
    interp: &'a mut Interpreter,
    block: &'a Block,
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        interp.env.push_scope();
        let mut last = Value::None;

        for (stmt, span) in block {
            match eval_stmt(interp, stmt).await {
                Ok(val) => last = val,
                Err(InterpreterError::Return(val)) => {
                    interp.env.pop_scope();
                    return Err(InterpreterError::Return(val));
                }
                Err(InterpreterError::Runtime(msg, err_span)) => {
                    interp.env.pop_scope();
                    // Attach this stmt's span if the error didn't already carry one.
                    return Err(InterpreterError::Runtime(
                        msg,
                        err_span.or_else(|| Some(span.clone())),
                    ));
                }
            }
        }

        interp.env.pop_scope();
        Ok(last)
    })
}

fn eval_stmt<'a>(
    interp: &'a mut Interpreter,
    stmt: &'a Stmt,
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let val = eval_expr(interp, value).await?;
                interp.env.define(name.clone(), val.clone());
                Ok(val)
            }

            Stmt::SelfAssign { field, value } => {
                let val = eval_expr(interp, value).await?;
                if let Some(agent_name) = &interp.current_agent.clone() {
                    if let Some(state) = interp.agent_state.get_mut(agent_name) {
                        state.insert(field.clone(), val.clone());
                    }
                }
                Ok(val)
            }

            Stmt::Expr(expr) => eval_expr(interp, expr).await,

            Stmt::Return(expr) => {
                let val = if let Some(e) = expr {
                    eval_expr(interp, e).await?
                } else {
                    Value::None
                };
                Err(InterpreterError::Return(val))
            }

            Stmt::For {
                binding,
                iter,
                filter,
                body,
            } => {
                let collection = eval_expr(interp, iter).await?;
                match collection {
                    Value::List(items) => {
                        for item in items {
                            interp.env.push_scope();
                            interp.env.define(binding.clone(), item);

                            if let Some(pred) = filter {
                                let should_include = eval_expr(interp, pred).await?;
                                if !should_include.is_truthy() {
                                    interp.env.pop_scope();
                                    continue;
                                }
                            }

                            eval_block(interp, body).await?;
                            interp.env.pop_scope();
                        }
                    }
                    _ => return Err(err("for loop requires a list")),
                }
                Ok(Value::None)
            }

            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let condition = eval_expr(interp, cond).await?;
                if condition.is_truthy() {
                    eval_block(interp, then_body).await
                } else if let Some(else_b) = else_body {
                    eval_block(interp, else_b).await
                } else {
                    Ok(Value::None)
                }
            }

            Stmt::When { subject, arms } => {
                let val = eval_expr(interp, subject).await?;
                eval_when(interp, &val, arms).await
            }

            Stmt::Notify { message } => {
                let msg = eval_expr(interp, message).await?;
                interp.runtime.notify(&msg.as_string());
                Ok(Value::None)
            }

            Stmt::Show { value } => {
                let val = eval_expr(interp, value).await?;
                interp.runtime.show(&val);
                Ok(Value::None)
            }

            Stmt::Send { value, target } => {
                let val = eval_expr(interp, value).await?;
                let tgt = eval_expr(interp, target).await?;
                println!(
                    "  {} Sending to {}: {}",
                    "→".bright_blue(),
                    tgt.as_string().bright_cyan(),
                    truncate(&val.as_string(), 80).dimmed()
                );
                Ok(Value::None)
            }

            Stmt::Archive { value } => {
                let val = eval_expr(interp, value).await?;
                println!(
                    "  {} Archived: {}",
                    "📦".dimmed(),
                    truncate(&val.as_string(), 60).dimmed()
                );
                Ok(Value::None)
            }

            Stmt::ConfirmThen {
                message,
                then_body,
            } => {
                let msg = eval_expr(interp, message).await?;
                let confirmed = interp.runtime.confirm(&msg.as_string());
                if confirmed {
                    eval_block(interp, then_body).await?;
                }
                Ok(Value::Bool(confirmed))
            }

            Stmt::Remember { value } => {
                let val = eval_expr(interp, value).await?;
                println!(
                    "  {} Remembered: {}",
                    "💾".dimmed(),
                    truncate(&val.as_string(), 60).dimmed()
                );
                Ok(Value::None)
            }

            Stmt::After { delay, body } => {
                let d = eval_expr(interp, delay).await?;
                let secs = match &d {
                    Value::Duration(s) => *s,
                    _ => return Err(err("after requires a duration")),
                };
                println!(
                    "  {} Scheduled: executing after {}",
                    "⏱".dimmed(),
                    d.as_string().dimmed()
                );
                tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
                eval_block(interp, body).await
            }

            Stmt::Retry {
                count,
                backoff,
                body,
            } => {
                let n = eval_expr(interp, count).await?;
                let max_attempts = match n {
                    Value::Integer(n) => n as u32,
                    _ => return Err(err("retry count must be an integer")),
                };

                let mut last_err = String::new();
                for attempt in 1..=max_attempts {
                    match eval_block(interp, body).await {
                        Ok(val) => return Ok(val),
                        Err(InterpreterError::Runtime(msg, _span)) => {
                            last_err = msg.clone();
                            if attempt < max_attempts {
                                let delay = if *backoff {
                                    // Exponential backoff: 1s, 2s, 4s, 8s...
                                    std::time::Duration::from_secs(1 << (attempt - 1))
                                } else {
                                    std::time::Duration::from_secs(1)
                                };
                                println!(
                                    "  {} Retry {}/{} in {:?}: {}",
                                    "↻".bright_yellow(),
                                    attempt,
                                    max_attempts,
                                    delay,
                                    msg.dimmed()
                                );
                                tokio::time::sleep(delay).await;
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
                Err(err(format!(
                    "Failed after {max_attempts} retries: {last_err}"
                )))
            }

            Stmt::Wait { duration, condition } => {
                if let Some(dur_expr) = duration {
                    let d = eval_expr(interp, dur_expr).await?;
                    let secs = match &d {
                        Value::Duration(s) => *s,
                        _ => return Err(err("wait requires a duration")),
                    };
                    println!("  {} Waiting {}...", "⏳".dimmed(), d.as_string().dimmed());
                    tokio::time::sleep(std::time::Duration::from_secs_f64(secs)).await;
                } else if let Some(cond_expr) = condition {
                    println!("  {} Waiting until condition...", "⏳".dimmed());
                    loop {
                        let val = eval_expr(interp, cond_expr).await?;
                        if val.is_truthy() {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
                Ok(Value::None)
            }

            Stmt::TryCatch { body, catches } => match eval_block(interp, body).await {
                Ok(val) => Ok(val),
                Err(InterpreterError::Runtime(msg, span)) => {
                    for clause in catches {
                        interp.env.push_scope();
                        let mut error_map = HashMap::new();
                        error_map.insert("message".to_string(), Value::String(msg.clone()));
                        interp
                            .env
                            .define(clause.name.clone(), Value::Map(error_map));
                        let result = eval_block(interp, &clause.body).await;
                        interp.env.pop_scope();
                        return result;
                    }
                    Err(InterpreterError::Runtime(msg, span))
                }
                Err(e) => Err(e),
            },
        }
    })
}

// ---------------------------------------------------------------------------
// When (pattern matching)
// ---------------------------------------------------------------------------

fn eval_when<'a>(
    interp: &'a mut Interpreter,
    subject: &'a Value,
    arms: &'a [WhenArm],
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        for arm in arms {
            for pattern in &arm.patterns {
                if matches_pattern(subject, pattern) {
                    if let Some(guard) = &arm.guard {
                        let guard_val = eval_expr(interp, guard).await?;
                        if !guard_val.is_truthy() {
                            continue;
                        }
                    }
                    return eval_block(interp, &arm.body).await;
                }
            }
        }
        Err(err("Non-exhaustive match: no pattern matched"))
    })
}

fn matches_pattern(value: &Value, pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Wildcard => true,
        Pattern::Ident(name) => {
            if let Value::EnumVariant(_, variant) = value {
                variant == name
            } else if let Value::String(s) = value {
                s == name
            } else {
                false
            }
        }
        Pattern::Literal(Expr::Integer(n)) => matches!(value, Value::Integer(v) if v == n),
        Pattern::Literal(Expr::StringLit(parts)) => {
            if let (Value::String(s), [StringPart::Literal(lit)]) = (value, parts.as_slice()) {
                s == lit
            } else {
                false
            }
        }
        Pattern::Literal(Expr::Bool(b)) => matches!(value, Value::Bool(v) if v == b),
        Pattern::Variant { name, .. } => {
            if let Value::EnumVariant(_, variant) = value {
                variant == name
            } else {
                false
            }
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Expression evaluation
// ---------------------------------------------------------------------------

fn eval_expr<'a>(
    interp: &'a mut Interpreter,
    expr: &'a Expr,
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        match expr {
            // ── Literals ─────────────────────────────────────────
            Expr::Integer(n) => Ok(Value::Integer(*n)),
            Expr::Float(n) => Ok(Value::Float(*n)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::None_ => Ok(Value::None),
            Expr::Now => Ok(Value::String(chrono_now())),

            Expr::StringLit(parts) => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        StringPart::Literal(s) => result.push_str(s),
                        StringPart::Interpolation(e) => {
                            let val = eval_expr(interp, e).await?;
                            result.push_str(&val.as_string());
                        }
                    }
                }
                Ok(Value::String(result))
            }

            // ── Identifiers & access ─────────────────────────────
            Expr::Ident(name) => {
                // Check variables, then connections (for fetch source names)
                if let Some(val) = interp.env.get(name) {
                    Ok(val.clone())
                } else if interp.connections.contains_key(name) {
                    Ok(Value::String(name.clone()))
                } else {
                    Err(err(format!("Undefined variable: '{name}'")))
                }
            }

            Expr::FieldAccess(obj, field) => {
                let val = eval_expr(interp, obj).await?;
                field_access(&val, field)
            }

            Expr::NullFieldAccess(obj, field) => {
                let val = eval_expr(interp, obj).await?;
                if let Value::None = val {
                    return Ok(Value::None);
                }
                field_access(&val, field)
            }

            Expr::NullAssert(inner) => {
                let val = eval_expr(interp, inner).await?;
                if let Value::None = val {
                    Err(err("Null assertion failed: value is none"))
                } else {
                    Ok(val)
                }
            }

            Expr::EnvAccess(var) => {
                let val = std::env::var(var).unwrap_or_default();
                Ok(Value::String(val))
            }

            Expr::SelfAccess(field) => {
                if let Some(agent_name) = &interp.current_agent.clone() {
                    if let Some(state) = interp.agent_state.get(agent_name) {
                        return state
                            .get(field)
                            .cloned()
                            .ok_or_else(|| err(format!("No state field '{field}'")));
                    }
                }
                Err(err("self can only be used inside an agent"))
            }

            // ── Compound literals ────────────────────────────────
            Expr::StructLit(fields) => {
                let mut map = HashMap::new();
                for (key, val_expr) in fields {
                    let val = eval_expr(interp, val_expr).await?;
                    map.insert(key.clone(), val);
                }
                Ok(Value::Map(map))
            }

            Expr::ListLit(items) => {
                let mut vals = Vec::new();
                for item in items {
                    vals.push(eval_expr(interp, item).await?);
                }
                Ok(Value::List(vals))
            }

            // ── Binary operators ─────────────────────────────────
            Expr::BinaryOp { left, op, right } => {
                let l = eval_expr(interp, left).await?;
                let r = eval_expr(interp, right).await?;
                eval_binop(&l, *op, &r)
            }

            Expr::UnaryOp { op, expr: inner } => {
                let val = eval_expr(interp, inner).await?;
                match op {
                    UnOp::Neg => match val {
                        Value::Integer(n) => Ok(Value::Integer(-n)),
                        Value::Float(n) => Ok(Value::Float(-n)),
                        _ => Err(err("Cannot negate non-numeric value")),
                    },
                    UnOp::Not => Ok(Value::Bool(!val.is_truthy())),
                }
            }

            Expr::NullCoalesce(left, right) => {
                let val = eval_expr(interp, left).await?;
                if let Value::None = val {
                    eval_expr(interp, right).await
                } else {
                    Ok(val)
                }
            }

            Expr::Pipeline(left, right) => {
                let input = eval_expr(interp, left).await?;
                match right.as_ref() {
                    Expr::Ident(name) => {
                        let args = vec![CallArg {
                            name: None,
                            value: expr_from_value(&input),
                        }];
                        eval_call(interp, name, &args).await
                    }
                    Expr::Call { callee, args } => {
                        if let Expr::Ident(name) = callee.as_ref() {
                            let mut full_args = vec![CallArg {
                                name: None,
                                value: expr_from_value(&input),
                            }];
                            full_args.extend(args.clone());
                            eval_call(interp, name, &full_args).await
                        } else {
                            Err(err("Pipeline target must be a function name"))
                        }
                    }
                    _ => Err(err("Pipeline target must be a function")),
                }
            }

            // ── Calls ────────────────────────────────────────────
            Expr::Call { callee, args } => {
                if let Expr::Ident(name) = callee.as_ref() {
                    eval_call(interp, name, args).await
                } else {
                    Err(err("Callee must be an identifier"))
                }
            }

            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                let obj = eval_expr(interp, object).await?;
                eval_method_call(interp, &obj, method, args).await
            }

            // ── AI primitives ────────────────────────────────────
            Expr::Classify {
                input,
                target,
                criteria,
                fallback,
                model,
            } => {
                let input_val = eval_expr(interp, input).await?;
                let variants = match target {
                    ClassifyTarget::Named(name) => get_enum_variants(interp, name)?,
                    ClassifyTarget::Inline(names) => names.clone(),
                };
                let type_name = match target {
                    ClassifyTarget::Named(name) => name.clone(),
                    ClassifyTarget::Inline(_) => "inline_enum".to_string(),
                };

                let result = interp
                    .runtime
                    .classify(
                        &input_val.as_string(),
                        &variants,
                        criteria.as_deref(),
                        model.as_deref(),
                    )
                    .await
                    .map_err(|e| err(e))?;

                match result {
                    Some(variant) => Ok(Value::EnumVariant(type_name, variant)),
                    None => {
                        if let Some(fb) = fallback {
                            eval_expr(interp, fb).await
                        } else {
                            Ok(Value::None)
                        }
                    }
                }
            }

            Expr::Summarize {
                input,
                length,
                fallback,
                model,
                ..
            } => {
                let input_val = eval_expr(interp, input).await?;
                let result = interp
                    .runtime
                    .summarize(&input_val.as_string(), length.as_ref(), model.as_deref())
                    .await
                    .map_err(|e| err(e))?;

                match result {
                    Some(summary) => Ok(Value::String(summary)),
                    None => {
                        if let Some(fb) = fallback {
                            eval_expr(interp, fb).await
                        } else {
                            Ok(Value::None)
                        }
                    }
                }
            }

            Expr::Draft {
                description,
                options,
                model,
            } => {
                let desc = eval_expr(interp, description).await?;
                let mut opts = HashMap::new();
                for (key, val_expr) in options {
                    let val = eval_expr(interp, val_expr).await?;
                    opts.insert(key.clone(), val.as_string());
                }

                let result = interp
                    .runtime
                    .draft(&desc.as_string(), &opts, model.as_deref())
                    .await
                    .map_err(|e| err(e))?;

                match result {
                    Some(text) => Ok(Value::String(text)),
                    None => Ok(Value::None),
                }
            }

            Expr::Extract {
                schema,
                source,
                model,
            } => {
                let input_val = eval_expr(interp, source).await?;
                let fields: Vec<(String, String)> = schema
                    .iter()
                    .map(|f| (f.name.clone(), format!("{:?}", f.ty)))
                    .collect();

                let result = interp
                    .runtime
                    .extract(&input_val.as_string(), &fields, model.as_deref())
                    .await
                    .map_err(|e| err(e))?;

                match result {
                    Some(json_str) => {
                        // Try to parse JSON response into a map
                        if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&json_str) {
                            let mut result_map = HashMap::new();
                            for (k, v) in map {
                                result_map.insert(k, json_value_to_keel(v));
                            }
                            Ok(Value::Map(result_map))
                        } else {
                            Ok(Value::String(json_str))
                        }
                    }
                    None => Ok(Value::None),
                }
            }

            Expr::Translate {
                input,
                target_lang,
                model,
            } => {
                let input_val = eval_expr(interp, input).await?;
                let result = interp
                    .runtime
                    .translate(&input_val.as_string(), target_lang, model.as_deref())
                    .await
                    .map_err(|e| err(e))?;

                match result {
                    Some(translations) => {
                        if target_lang.len() == 1 {
                            // Single target → return string
                            Ok(Value::String(
                                translations.into_values().next().unwrap_or_default(),
                            ))
                        } else {
                            // Multi target → return map
                            let map = translations
                                .into_iter()
                                .map(|(k, v)| (k, Value::String(v)))
                                .collect();
                            Ok(Value::Map(map))
                        }
                    }
                    None => Ok(Value::None),
                }
            }

            Expr::Decide {
                input,
                options,
                model,
            } => {
                let input_val = eval_expr(interp, input).await?;
                let mut opts = HashMap::new();
                for (key, val_expr) in options {
                    let val = eval_expr(interp, val_expr).await?;
                    opts.insert(key.clone(), val.as_string());
                }

                let result = interp
                    .runtime
                    .decide(&input_val.as_string(), &opts, model.as_deref())
                    .await
                    .map_err(|e| err(e))?;

                match result {
                    Some((choice, reason)) => {
                        let mut map = HashMap::new();
                        map.insert("choice".to_string(), Value::String(choice));
                        map.insert("reason".to_string(), Value::String(reason));
                        Ok(Value::Map(map))
                    }
                    None => Ok(Value::None),
                }
            }

            // ── Prompt (raw LLM) ─────────────────────────────────
            Expr::Prompt {
                config,
                target_type,
            } => {
                let mut system = String::new();
                let mut user_msg = String::new();
                let mut model_override: Option<String> = None;

                for (key, val_expr) in config {
                    let val = eval_expr(interp, val_expr).await?;
                    match key.as_str() {
                        "system" => system = val.as_string(),
                        "user" => user_msg = val.as_string(),
                        "model" => model_override = Some(val.as_string()),
                        _ => {}
                    }
                }

                let result = interp
                    .runtime
                    .prompt(&system, &user_msg, model_override.as_deref())
                    .await
                    .map_err(|e| err(e))?;

                match result {
                    Some(response) => {
                        if target_type == "dynamic" || target_type == "str" {
                            Ok(Value::String(response))
                        } else {
                            // Try parsing as JSON
                            if let Ok(map) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&response) {
                                let mut result_map = HashMap::new();
                                for (k, v) in map {
                                    result_map.insert(k, json_value_to_keel(v));
                                }
                                Ok(Value::Map(result_map))
                            } else {
                                Ok(Value::String(response))
                            }
                        }
                    }
                    None => Ok(Value::None),
                }
            }

            // ── Human interaction ────────────────────────────────
            Expr::Ask { prompt, .. } => {
                let prompt_val = eval_expr(interp, prompt).await?;
                let answer = interp.runtime.ask(&prompt_val.as_string());
                Ok(Value::String(answer))
            }

            Expr::Confirm { message } => {
                let msg = eval_expr(interp, message).await?;
                let confirmed = interp.runtime.confirm(&msg.as_string());
                Ok(Value::Bool(confirmed))
            }

            // ── Data access ──────────────────────────────────────
            Expr::Fetch { source, filter } => {
                let src = eval_expr(interp, source).await?;
                let source_name = src.as_string();
                let filter_desc = if let Some(f) = filter {
                    match f.as_ref() {
                        Expr::Ident(name) => format!(" where {name}"),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                println!(
                    "  {} Fetching from {}{filter_desc}...",
                    "↓".bright_blue(),
                    source_name.bright_cyan()
                );

                // Check if this is a real email connection
                if let Some(conn) = &interp.email_conn {
                    if interp.connections.contains_key(&source_name) {
                        match crate::runtime::email::fetch_emails(conn) {
                            Ok(emails) => return Ok(Value::List(emails)),
                            Err(e) => return Err(err(format!("Email fetch failed: {e}"))),
                        }
                    }
                }

                // Check if this is a URL fetch
                if source_name.starts_with("http://") || source_name.starts_with("https://") {
                    match interp.runtime.http_get(&source_name).await {
                        Ok(response) => return Ok(response),
                        Err(e) => return Err(err(e)),
                    }
                }

                // Fallback: return empty list for unrecognized sources
                Ok(Value::List(vec![]))
            }

            // ── Memory ───────────────────────────────────────────
            Expr::Recall { query, limit } => {
                let q = eval_expr(interp, query).await?;
                println!(
                    "  {} Recalling: {} (limit: {})",
                    "🧠".dimmed(),
                    q.as_string().dimmed(),
                    limit.unwrap_or(10)
                );
                Ok(Value::List(vec![]))
            }

            // ── Delegate ─────────────────────────────────────────
            Expr::Delegate { task_call, agent } => {
                println!(
                    "  {} Delegating to {}",
                    "→".bright_yellow(),
                    agent.bright_white().bold()
                );
                eval_expr(interp, task_call).await
            }

            // ── Control flow as expressions ──────────────────────
            Expr::IfExpr {
                cond,
                then_body,
                else_body,
            } => {
                let condition = eval_expr(interp, cond).await?;
                if condition.is_truthy() {
                    eval_block(interp, then_body).await
                } else {
                    eval_block(interp, else_body).await
                }
            }

            Expr::WhenExpr { subject, arms } => {
                let val = eval_expr(interp, subject).await?;
                eval_when(interp, &val, arms).await
            }

            Expr::Lambda { params, body } => {
                Ok(Value::Closure(params.clone(), body.clone()))
            }

            // ── Duration ─────────────────────────────────────────
            Expr::Duration { value, unit } => {
                let val = eval_expr(interp, value).await?;
                match val {
                    Value::Integer(n) => {
                        let secs = Value::duration_seconds(n, *unit);
                        Ok(Value::Duration(secs))
                    }
                    _ => Err(err("Duration value must be an integer")),
                }
            }

            // ── Enum variant ─────────────────────────────────────
            Expr::EnumVariant(ty, variant) => {
                Ok(Value::EnumVariant(ty.clone(), variant.clone()))
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Function/task calls
// ---------------------------------------------------------------------------

fn eval_call<'a>(
    interp: &'a mut Interpreter,
    name: &'a str,
    args: &'a [CallArg],
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        let task = interp
            .tasks
            .get(name)
            .cloned()
            .ok_or_else(|| err(format!("Undefined task: '{name}'")))?;

        let mut arg_values = Vec::new();
        for arg in args {
            let val = eval_expr(interp, &arg.value).await?;
            arg_values.push((arg.name.clone(), val));
        }

        interp.env.push_scope();

        for (i, param) in task.params.iter().enumerate() {
            let val = arg_values
                .iter()
                .find(|(n, _)| n.as_deref() == Some(&param.name))
                .map(|(_, v)| v.clone())
                .or_else(|| arg_values.get(i).map(|(_, v)| v.clone()))
                .unwrap_or(Value::None);

            interp.env.define(param.name.clone(), val);
        }

        let result = match eval_block(interp, &task.body).await {
            Ok(val) => val,
            Err(InterpreterError::Return(val)) => val,
            Err(e) => {
                interp.env.pop_scope();
                return Err(e);
            }
        };

        interp.env.pop_scope();
        Ok(result)
    })
}

fn eval_method_call<'a>(
    interp: &'a mut Interpreter,
    object: &'a Value,
    method: &'a str,
    args: &'a [CallArg],
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        match object {
            Value::String(s) => match method {
                "contains" => {
                    let arg = eval_expr(interp, &args[0].value).await?.as_string();
                    Ok(Value::Bool(s.contains(&arg)))
                }
                "starts_with" => {
                    let arg = eval_expr(interp, &args[0].value).await?.as_string();
                    Ok(Value::Bool(s.starts_with(&arg)))
                }
                "ends_with" => {
                    let arg = eval_expr(interp, &args[0].value).await?.as_string();
                    Ok(Value::Bool(s.ends_with(&arg)))
                }
                "trim" => Ok(Value::String(s.trim().to_string())),
                "upper" => Ok(Value::String(s.to_uppercase())),
                "lower" => Ok(Value::String(s.to_lowercase())),
                "split" => {
                    let sep = eval_expr(interp, &args[0].value).await?.as_string();
                    let parts: Vec<Value> =
                        s.split(&sep).map(|p| Value::String(p.to_string())).collect();
                    Ok(Value::List(parts))
                }
                "replace" => {
                    let old = eval_expr(interp, &args[0].value).await?.as_string();
                    let new = eval_expr(interp, &args[1].value).await?.as_string();
                    Ok(Value::String(s.replace(&old, &new)))
                }
                "to_int" => Ok(s.parse::<i64>().map(Value::Integer).unwrap_or(Value::None)),
                "to_float" => Ok(s.parse::<f64>().map(Value::Float).unwrap_or(Value::None)),
                _ => Err(err(format!("Unknown string method: {method}"))),
            },
            Value::List(items) => match method {
                "map" => {
                    let func = resolve_callable(interp, args)?;
                    let mut results = Vec::new();
                    for item in items {
                        results.push(call_callable(interp, &func, item).await?);
                    }
                    Ok(Value::List(results))
                }
                "filter" => {
                    let func = resolve_callable(interp, args)?;
                    let mut results = Vec::new();
                    for item in items {
                        let result = call_callable(interp, &func, item).await?;
                        if result.is_truthy() {
                            results.push(item.clone());
                        }
                    }
                    Ok(Value::List(results))
                }
                "find" => {
                    let func = resolve_callable(interp, args)?;
                    for item in items {
                        let result = call_callable(interp, &func, item).await?;
                        if result.is_truthy() {
                            return Ok(item.clone());
                        }
                    }
                    Ok(Value::None)
                }
                "any" => {
                    let func = resolve_callable(interp, args)?;
                    for item in items {
                        if call_callable(interp, &func, item).await?.is_truthy() {
                            return Ok(Value::Bool(true));
                        }
                    }
                    Ok(Value::Bool(false))
                }
                "all" => {
                    let func = resolve_callable(interp, args)?;
                    for item in items {
                        if !call_callable(interp, &func, item).await?.is_truthy() {
                            return Ok(Value::Bool(false));
                        }
                    }
                    Ok(Value::Bool(true))
                }
                "sort_by" => {
                    // Evaluate key function on each item, then sort
                    let func = resolve_callable(interp, args)?;
                    let mut keyed: Vec<(Value, String)> = Vec::new();
                    for item in items {
                        let key = call_callable(interp, &func, item).await?;
                        keyed.push((item.clone(), key.as_string()));
                    }
                    keyed.sort_by(|a, b| a.1.cmp(&b.1));
                    Ok(Value::List(keyed.into_iter().map(|(v, _)| v).collect()))
                }
                "flat_map" => {
                    let func = resolve_callable(interp, args)?;
                    let mut results = Vec::new();
                    for item in items {
                        let result = call_callable(interp, &func, item).await?;
                        if let Value::List(inner) = result {
                            results.extend(inner);
                        } else {
                            results.push(result);
                        }
                    }
                    Ok(Value::List(results))
                }
                _ => Err(err(format!("Unknown list method: {method}"))),
            },
            Value::Integer(n) => match method {
                "to_str" => Ok(Value::String(n.to_string())),
                "to_float" => Ok(Value::Float(*n as f64)),
                _ => Err(err(format!("Unknown int method: {method}"))),
            },
            _ => Err(err(format!(
                "Cannot call method '{method}' on {}",
                object.type_name()
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Binary operation evaluation
// ---------------------------------------------------------------------------

fn eval_binop(left: &Value, op: BinOp, right: &Value) -> IResult {
    match op {
        BinOp::Add => match (left, right) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
            (Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a + *b as f64)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
            _ => Err(err(format!(
                "Cannot add {} and {}",
                left.type_name(),
                right.type_name()
            ))),
        },
        BinOp::Sub => match (left, right) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            _ => Err(err("Cannot subtract non-numeric values")),
        },
        BinOp::Mul => match (left, right) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Duration(d), Value::Integer(n)) => Ok(Value::Duration(d * *n as f64)),
            _ => Err(err("Cannot multiply non-numeric values")),
        },
        BinOp::Div => match (left, right) {
            (Value::Integer(a), Value::Integer(b)) if *b != 0 => Ok(Value::Integer(a / b)),
            (Value::Float(a), Value::Float(b)) if *b != 0.0 => Ok(Value::Float(a / b)),
            _ => Err(err("Division error")),
        },
        BinOp::Mod => match (left, right) {
            (Value::Integer(a), Value::Integer(b)) if *b != 0 => Ok(Value::Integer(a % b)),
            _ => Err(err("Modulo error")),
        },
        BinOp::Eq => Ok(Value::Bool(left == right)),
        BinOp::Neq => Ok(Value::Bool(left != right)),
        BinOp::Lt => num_cmp(left, right, |a, b| a < b),
        BinOp::Gt => num_cmp(left, right, |a, b| a > b),
        BinOp::Lte => num_cmp(left, right, |a, b| a <= b),
        BinOp::Gte => num_cmp(left, right, |a, b| a >= b),
        BinOp::And => Ok(Value::Bool(left.is_truthy() && right.is_truthy())),
        BinOp::Or => Ok(Value::Bool(left.is_truthy() || right.is_truthy())),
    }
}

fn num_cmp(left: &Value, right: &Value, f: fn(f64, f64) -> bool) -> IResult {
    match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(f(*a as f64, *b as f64))),
        (Value::Float(a), Value::Float(b)) => Ok(Value::Bool(f(*a, *b))),
        (Value::Integer(a), Value::Float(b)) => Ok(Value::Bool(f(*a as f64, *b))),
        (Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(f(*a, *b as f64))),
        (Value::String(a), Value::String(b)) => Ok(Value::Bool(a < b)),
        _ => Err(err("Cannot compare these types")),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn field_access(val: &Value, field: &str) -> IResult {
    match val {
        Value::Map(fields) => fields
            .get(field)
            .cloned()
            .ok_or_else(|| err(format!("No field '{field}' on value"))),
        Value::List(items) => match field {
            "count" => Ok(Value::Integer(items.len() as i64)),
            "first" => Ok(items.first().cloned().unwrap_or(Value::None)),
            "last" => Ok(items.last().cloned().unwrap_or(Value::None)),
            "is_empty" => Ok(Value::Bool(items.is_empty())),
            _ => Err(err(format!("No property '{field}' on list"))),
        },
        Value::String(s) => match field {
            "length" => Ok(Value::Integer(s.len() as i64)),
            "is_empty" => Ok(Value::Bool(s.is_empty())),
            _ => Err(err(format!("No property '{field}' on string"))),
        },
        _ => Err(err(format!(
            "Cannot access field '{field}' on {}",
            val.type_name()
        ))),
    }
}

fn get_enum_variants(interp: &Interpreter, name: &str) -> Result<Vec<String>, InterpreterError> {
    match interp.types.get(name) {
        Some(TypeDef::SimpleEnum(variants)) => Ok(variants.clone()),
        Some(TypeDef::RichEnum(variants)) => {
            Ok(variants.iter().map(|v| v.name.clone()).collect())
        }
        _ => Err(err(format!("Type '{name}' is not an enum"))),
    }
}

fn expr_from_value(val: &Value) -> Expr {
    match val {
        Value::Integer(n) => Expr::Integer(*n),
        Value::Float(n) => Expr::Float(*n),
        Value::String(s) => Expr::StringLit(vec![StringPart::Literal(s.clone())]),
        Value::Bool(b) => Expr::Bool(*b),
        Value::None => Expr::None_,
        Value::Map(fields) => Expr::StructLit(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), expr_from_value(v)))
                .collect(),
        ),
        Value::List(items) => Expr::ListLit(items.iter().map(expr_from_value).collect()),
        Value::EnumVariant(ty, var) => Expr::EnumVariant(ty.clone(), var.clone()),
        Value::Duration(secs) => Expr::Float(*secs),
        _ => Expr::None_,
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{secs}")
}

/// Wait for the next tick from any of the interval timers.
/// Returns the index of the timer that fired.
async fn next_tick(timers: &mut Vec<tokio::time::Interval>) -> usize {
    if timers.is_empty() {
        std::future::pending::<()>().await;
        return 0;
    }
    // Poll all timers using a simple sequential check with sleep
    // For v1.0 most agents have 1 every block, so this is efficient enough
    loop {
        for (i, timer) in timers.iter_mut().enumerate() {
            // Try to tick without waiting using poll
            let tick = tokio::time::timeout(std::time::Duration::from_millis(0), timer.tick()).await;
            if tick.is_ok() {
                return i;
            }
        }
        // None ready — sleep briefly and retry
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

// ---------------------------------------------------------------------------
// Callable helpers (lambdas and task references)
// ---------------------------------------------------------------------------

/// A resolved callable — either a closure or a named task.
enum Callable {
    Closure(Vec<LambdaParam>, Box<Expr>),
    TaskName(String),
}

/// Extract a callable from method call arguments (e.g., `.map(e => e + 1)` or `.map(triage)`).
fn resolve_callable(interp: &Interpreter, args: &[CallArg]) -> Result<Callable, InterpreterError> {
    let arg = args
        .first()
        .ok_or_else(|| err("Expected a function argument"))?;
    match &arg.value {
        Expr::Lambda { params, body } => {
            Ok(Callable::Closure(params.clone(), body.clone()))
        }
        Expr::Ident(name) => {
            if interp.tasks.contains_key(name) {
                Ok(Callable::TaskName(name.clone()))
            } else {
                Err(err(format!("'{name}' is not a task")))
            }
        }
        _ => Err(err("Expected a lambda or task name")),
    }
}

/// Call a callable with a single argument value.
fn call_callable<'a>(
    interp: &'a mut Interpreter,
    callable: &'a Callable,
    arg: &'a Value,
) -> Pin<Box<dyn Future<Output = IResult> + 'a>> {
    Box::pin(async move {
        match callable {
            Callable::Closure(params, body) => {
                interp.env.push_scope();
                // Bind parameter(s)
                if let Some(param) = params.first() {
                    interp.env.define(param.name.clone(), arg.clone());
                }
                let result = eval_expr(interp, body).await;
                interp.env.pop_scope();
                result
            }
            Callable::TaskName(name) => {
                let call_args = vec![CallArg {
                    name: None,
                    value: expr_from_value(arg),
                }];
                eval_call(interp, name, &call_args).await
            }
        }
    })
}

fn json_value_to_keel(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::None,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else {
                Value::Float(n.as_f64().unwrap_or(0.0))
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(arr) => {
            Value::List(arr.into_iter().map(json_value_to_keel).collect())
        }
        serde_json::Value::Object(obj) => {
            Value::Map(obj.into_iter().map(|(k, v)| (k, json_value_to_keel(v))).collect())
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max])
    } else {
        s.to_string()
    }
}
