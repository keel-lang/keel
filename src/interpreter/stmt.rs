use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use miette::Result;

use crate::ast::{Block, Expr, Pattern, Stmt, StringPart};

use super::bind_value;
use super::environment::Environment;
use super::runtime_error;
use super::state::Interpreter;
use super::value::Value;

impl Interpreter {
    pub fn exec_stmt<'a>(
        &'a mut self,
        stmt: &'a Stmt,
        env: &'a mut Environment,
    ) -> Pin<Box<dyn Future<Output = Result<StmtOutcome>> + Send + 'a>> {
        Box::pin(async move {
            match stmt {
                Stmt::Let { binding, value, .. } => {
                    let v = self.eval_expr(value, env).await?;
                    if let Value::EarlyReturn(inner) = v {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    bind_value(binding, v, env)?;
                    Ok(StmtOutcome::Normal)
                }
                Stmt::SelfAssign { field, value } => {
                    let v = self.eval_expr(value, env).await?;
                    if let Value::EarlyReturn(inner) = v {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    if let Some(agent) = &self.current_agent {
                        let mut guard = agent.lock();
                        if guard.def.state_fields.iter().any(|f| f.name == *field && f.readonly) {
                            return Err(runtime_error(format!(
                                "cannot assign to `self.{field}`: field is declared readonly"
                            )));
                        }
                        guard.state.insert(field.clone(), v);
                        Ok(StmtOutcome::Normal)
                    } else {
                        Err(runtime_error(format!(
                            "`self.{field}` used outside an agent"
                        )))
                    }
                }
                Stmt::Expr(e) => {
                    let v = self.eval_expr(e, env).await?;
                    Ok(StmtOutcome::Value(v))
                }
                Stmt::Return(opt) => {
                    let v = match opt {
                        Some(e) => {
                            let v = self.eval_expr(e, env).await?;
                            // If the return expression itself triggered an inner return
                            // (e.g. `return if cond { return x } else { y }`), the
                            // inner return wins — propagate it unchanged.
                            if let Value::EarlyReturn(inner) = v {
                                return Ok(StmtOutcome::Return(*inner));
                            }
                            v
                        }
                        None => Value::None,
                    };
                    Ok(StmtOutcome::Return(v))
                }
                Stmt::For {
                    binding,
                    iter,
                    filter,
                    body,
                } => {
                    let iter_v = self.eval_expr(iter, env).await?;
                    if let Value::EarlyReturn(inner) = iter_v {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    // Build an owned iterator without materializing a range.
                    enum ForIter {
                        List(std::vec::IntoIter<Value>),
                        Range(std::ops::RangeInclusive<i64>),
                    }
                    impl Iterator for ForIter {
                        type Item = Value;
                        fn next(&mut self) -> Option<Value> {
                            match self {
                                ForIter::List(it) => it.next(),
                                ForIter::Range(it) => it.next().map(Value::Integer),
                            }
                        }
                    }
                    let iter: ForIter = match iter_v {
                        Value::List(items) => ForIter::List(items.into_iter()),
                        Value::Range(lo, hi) => ForIter::Range(lo..=hi),
                        other => {
                            return Err(runtime_error(format!(
                                "`for` expects a list, got {}",
                                other.type_name()
                            )));
                        }
                    };
                    for item in iter {
                        env.push_scope();
                        bind_value(binding, item, env)?;
                        if let Some(pred) = filter {
                            let matched = self.eval_expr(pred, env).await?.is_truthy();
                            if !matched {
                                env.pop_scope();
                                continue;
                            }
                        }
                        let outcome = self.exec_block(body, env).await?;
                        env.pop_scope();
                        match outcome {
                            StmtOutcome::Return(v) => return Ok(StmtOutcome::Return(v)),
                            StmtOutcome::Break => break,
                            // Continue falls through to the next iteration.
                            StmtOutcome::Continue | StmtOutcome::Normal | StmtOutcome::Value(_) => {
                            }
                        }
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    let c = self.eval_expr(cond, env).await?;
                    if let Value::EarlyReturn(inner) = c {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    if c.is_truthy() {
                        self.exec_block(then_body, env).await
                    } else if let Some(eb) = else_body {
                        self.exec_block(eb, env).await
                    } else {
                        Ok(StmtOutcome::Normal)
                    }
                }
                Stmt::When { subject, arms } => {
                    let s = self.eval_expr(subject, env).await?;
                    if let Value::EarlyReturn(inner) = s {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    for arm in arms {
                        if let Some(bindings) = self.match_patterns(&arm.patterns, &s) {
                            env.push_scope();
                            for (k, v) in bindings {
                                env.define(k, v);
                            }
                            if let Some(guard) = &arm.guard
                                && !self.eval_expr(guard, env).await?.is_truthy()
                            {
                                env.pop_scope();
                                continue;
                            }
                            let out = self.exec_block(&arm.body, env).await?;
                            env.pop_scope();
                            return Ok(out);
                        }
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::AugAssign { name, op, rhs } => {
                    let rhs_val = self.eval_expr(rhs, env).await?;
                    if let Value::EarlyReturn(inner) = rhs_val {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    let current = env.get(name).cloned().unwrap_or(Value::None);
                    let result = crate::interpreter::binary::eval_binary(*op, current, rhs_val)?;
                    if !env.set(name, result) {
                        return Err(runtime_error(format!(
                            "augmented assignment to undefined variable `{name}`"
                        )));
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::Raise(e) => {
                    let v = self.eval_expr(e, env).await?;
                    if let Value::EarlyReturn(inner) = v {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    let message = match &v {
                        Value::String(s) => s.clone(),
                        other => other.to_display_string(),
                    };
                    Err(runtime_error(message))
                }
                Stmt::Break => Ok(StmtOutcome::Break),
                Stmt::Continue => Ok(StmtOutcome::Continue),
                Stmt::TryCatch { body, catches } => {
                    self.last_typed_error = None;
                    match self.exec_block(body, env).await {
                        Ok(outcome) => Ok(outcome),
                        Err(err) => {
                            let typed = self.last_typed_error.take();
                            for clause in catches {
                                let clause_type = match &clause.ty {
                                    crate::ast::TypeExpr::Named(n) => n.as_str(),
                                    _ => continue,
                                };
                                let matches = match &typed {
                                    Some((type_name, _)) => {
                                        clause_type == "Error" || clause_type == type_name
                                    }
                                    None => clause_type == "Error",
                                };
                                if matches {
                                    let fields = match &typed {
                                        Some((_, f)) => f.clone(),
                                        None => {
                                            let mut m = HashMap::new();
                                            m.insert(
                                                "message".to_string(),
                                                Value::String(err.to_string()),
                                            );
                                            m
                                        }
                                    };
                                    let error_val = Value::Map(fields);
                                    env.push_scope();
                                    env.define(clause.name.clone(), error_val);
                                    let outcome = self.exec_block(&clause.body, env).await;
                                    env.pop_scope();
                                    return outcome;
                                }
                            }
                            Err(err)
                        }
                    }
                }
            }
        })
    }

    pub(crate) async fn exec_block(
        &mut self,
        block: &Block,
        env: &mut Environment,
    ) -> Result<StmtOutcome> {
        let mut last = Value::None;
        for (stmt, _) in block {
            match self.exec_stmt(stmt, env).await? {
                StmtOutcome::Return(v) => return Ok(StmtOutcome::Return(v)),
                // Break and Continue bubble up through exec_block; the For
                // loop handler in exec_stmt catches them at the loop boundary.
                StmtOutcome::Break => return Ok(StmtOutcome::Break),
                StmtOutcome::Continue => return Ok(StmtOutcome::Continue),
                StmtOutcome::Value(v) => {
                    if let Value::EarlyReturn(inner) = v {
                        return Ok(StmtOutcome::Return(*inner));
                    }
                    last = v;
                }
                StmtOutcome::Normal => last = Value::None,
            }
        }
        Ok(StmtOutcome::Value(last))
    }

    pub(crate) fn match_patterns(
        &self,
        patterns: &[Pattern],
        value: &Value,
    ) -> Option<Vec<(String, Value)>> {
        for p in patterns {
            if let Some(b) = self.match_pattern(p, value) {
                return Some(b);
            }
        }
        None
    }

    fn match_pattern(&self, pattern: &Pattern, value: &Value) -> Option<Vec<(String, Value)>> {
        match pattern {
            Pattern::Wildcard => Some(vec![]),
            Pattern::Ident(name) => {
                // Matches an enum variant by name (e.g. `low`, `high`).
                if let Value::EnumVariant(_, variant, _) = value
                    && variant == name
                {
                    return Some(vec![]);
                }
                None
            }
            Pattern::Literal(e) => {
                let lit = match e {
                    Expr::Integer(n) => Value::Integer(*n),
                    Expr::Float(f) => Value::Float(*f),
                    Expr::StringLit(parts) => {
                        // Only literal-only strings match here.
                        let mut s = String::new();
                        for p in parts {
                            if let StringPart::Literal(t) = p {
                                s.push_str(t);
                            } else {
                                return None;
                            }
                        }
                        Value::String(s)
                    }
                    Expr::Bool(b) => Value::Bool(*b),
                    _ => return None,
                };
                if &lit == value { Some(vec![]) } else { None }
            }
            Pattern::Variant { name, bindings } => {
                if let Value::EnumVariant(_ty, variant, fields) = value
                    && variant == name
                {
                    // Bind each named destructure from the variant's
                    // rich fields (if any). Wildcards (`_`) and
                    // missing fields bind to `none`.
                    let mut out = Vec::with_capacity(bindings.len());
                    for b in bindings {
                        if b == "_" {
                            continue;
                        }
                        let v = fields
                            .as_ref()
                            .and_then(|m| m.get(b).cloned())
                            .unwrap_or(Value::None);
                        out.push((b.clone(), v));
                    }
                    return Some(out);
                }
                None
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum StmtOutcome {
    /// Statement executed; no value produced (e.g. a Let).
    Normal,
    /// Expression statement produced a value.
    Value(Value),
    /// `return` reached — propagate to enclosing task.
    Return(Value),
    /// `break` — exit the nearest enclosing `for` loop.
    Break,
    /// `continue` — skip to the next iteration of the nearest `for` loop.
    Continue,
}

// ---------------------------------------------------------------------------
