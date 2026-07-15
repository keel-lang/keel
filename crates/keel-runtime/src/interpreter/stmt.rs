use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use miette::Result;

use crate::ast::{Block, Expr, Pattern, Stmt, StringPart};

use super::bind_value;
use super::environment::Environment;
use super::promote::promote_value;
use super::state::{CallArgValue, Interpreter};
use super::value::Value;
use super::{RuntimeError, RuntimeErrorKind, runtime_error};
use crate::runtime::namespace::make_typed_report;

impl Interpreter {
    pub fn exec_stmt<'a>(
        &'a mut self,
        stmt: &'a Stmt,
        env: &'a mut Environment,
    ) -> Pin<Box<dyn Future<Output = Result<StmtOutcome>> + Send + 'a>> {
        Box::pin(async move {
            match stmt {
                Stmt::Let { binding, ty, value } => {
                    let v = match self.eval_expr(value, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    let v = match ty {
                        Some(ty_node) => promote_value(
                            v,
                            &ty_node.kind,
                            &self.struct_types,
                            &self.struct_aliases,
                        ),
                        None => v,
                    };
                    bind_value(binding, v, env)?;
                    Ok(StmtOutcome::Normal)
                }
                Stmt::SelfAssign { field, value, .. } => {
                    let v = match self.eval_expr(value, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    if let Some(agent) = &self.current_agent {
                        let mut guard = agent.lock();
                        if guard
                            .def
                            .state_fields
                            .iter()
                            .any(|f| f.name == *field && f.readonly)
                        {
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
                    let v = match self.eval_expr(e, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    Ok(StmtOutcome::Value(v))
                }
                Stmt::Return(opt) => {
                    let v = match opt {
                        Some(e) => match self.eval_expr(e, env).await? {
                            ExprFlow::Value(v) => v,
                            // If the return expression itself contained an inner return
                            // (e.g. `return if cond { return x } else { y }`), the
                            // inner return wins — propagate it unchanged.
                            ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                        },
                        None => Value::None,
                    };
                    Ok(StmtOutcome::Return(v))
                }
                Stmt::Assert { cond, message } => {
                    let value = match self.eval_expr(cond, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    match value {
                        Value::Bool(true) => Ok(StmtOutcome::Normal),
                        Value::Bool(false) => {
                            let message = match message {
                                Some(message) => match self.eval_expr(message, env).await? {
                                    ExprFlow::Value(Value::String(s)) => s,
                                    ExprFlow::Value(other) => {
                                        return Err(runtime_error(format!(
                                            "`assert` message expected str, got {}",
                                            other.type_name()
                                        )));
                                    }
                                    ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                                },
                                None => "assertion failed".to_string(),
                            };
                            Err(runtime_error(message))
                        }
                        other => Err(runtime_error(format!(
                            "`assert` expected bool, got {}",
                            other.type_name()
                        ))),
                    }
                }
                Stmt::For {
                    binding,
                    iter,
                    filter,
                    body,
                } => {
                    let iter_v = match self.eval_expr(iter, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    // Unwrap Iterable structs: call items() to get the list.
                    // Only Value::Struct carries a type tag; Value::Map never dispatches
                    // to impl methods after the subset-fallback removal.
                    let iter_v = if matches!(&iter_v, Value::Struct(_, _)) {
                        let task_opt = self.find_impl_task(&iter_v, "items");
                        if let Some(task) = task_opt {
                            self.call_task(
                                "items",
                                &task,
                                vec![CallArgValue {
                                    name: None,
                                    value: iter_v,
                                }],
                            )
                            .await?
                        } else {
                            iter_v
                        }
                    } else {
                        iter_v
                    };
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
                            let pred_val = match self.eval_expr(pred, env).await? {
                                ExprFlow::Value(v) => v,
                                ExprFlow::Return(v) => {
                                    env.pop_scope();
                                    return Ok(StmtOutcome::Return(v));
                                }
                            };
                            if !pred_val.is_truthy() {
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
                    let c = match self.eval_expr(cond, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    if c.is_truthy() {
                        self.exec_block(then_body, env).await
                    } else if let Some(eb) = else_body {
                        self.exec_block(eb, env).await
                    } else {
                        Ok(StmtOutcome::Normal)
                    }
                }
                Stmt::When { subject, arms } => {
                    let s = match self.eval_expr(subject, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    for arm in arms {
                        if let Some(bindings) = self.match_patterns(&arm.patterns, &s) {
                            env.push_scope();
                            for (k, v) in bindings {
                                env.define(k, v);
                            }
                            if let Some(guard) = &arm.guard {
                                let guard_val = match self.eval_expr(guard, env).await? {
                                    ExprFlow::Value(v) => v,
                                    ExprFlow::Return(v) => {
                                        env.pop_scope();
                                        return Ok(StmtOutcome::Return(v));
                                    }
                                };
                                if !guard_val.is_truthy() {
                                    env.pop_scope();
                                    continue;
                                }
                            }
                            let out = self.exec_block(&arm.body, env).await?;
                            env.pop_scope();
                            return Ok(out);
                        }
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::AugAssign { name, op, rhs, .. } => {
                    let rhs_val = match self.eval_expr(rhs, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
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
                    let v = match self.eval_expr(e, env).await? {
                        ExprFlow::Value(v) => v,
                        ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                    };
                    let message = match &v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    Err(make_typed_report(RuntimeErrorKind::UserRaised, message))
                }
                Stmt::While { cond, body } => {
                    loop {
                        let c = match self.eval_expr(cond, env).await? {
                            ExprFlow::Value(v) => v,
                            ExprFlow::Return(v) => return Ok(StmtOutcome::Return(v)),
                        };
                        if !c.is_truthy() {
                            break;
                        }
                        env.push_scope();
                        let outcome = self.exec_block(body, env).await?;
                        env.pop_scope();
                        match outcome {
                            StmtOutcome::Return(v) => return Ok(StmtOutcome::Return(v)),
                            StmtOutcome::Break => break,
                            StmtOutcome::Continue | StmtOutcome::Normal | StmtOutcome::Value(_) => {
                            }
                        }
                    }
                    Ok(StmtOutcome::Normal)
                }
                Stmt::Break => Ok(StmtOutcome::Break),
                Stmt::Continue => Ok(StmtOutcome::Continue),
                Stmt::TryCatch { body, catches } => match self.exec_block(body, env).await {
                    Ok(outcome) => Ok(outcome),
                    Err(err) => {
                        let typed = err.downcast_ref::<RuntimeError>();
                        for clause in catches {
                            let clause_type = match &clause.ty.kind {
                                crate::ast::TypeExpr::Named(n) => n.as_str(),
                                _ => continue,
                            };
                            let matches = match &typed {
                                Some(typed) => {
                                    clause_type == "Error" || clause_type == typed.type_name()
                                }
                                None => clause_type == "Error",
                            };
                            if matches {
                                let error_val = match &typed {
                                    Some(typed) => typed.as_value(),
                                    None => {
                                        let mut m = HashMap::new();
                                        m.insert(
                                            "message".to_string(),
                                            Value::String(err.to_string()),
                                        );
                                        Value::Struct("Error".to_string(), m)
                                    }
                                };
                                env.push_scope();
                                env.define(clause.name.clone(), error_val);
                                let outcome = self.exec_block(&clause.body, env).await;
                                env.pop_scope();
                                return outcome;
                            }
                        }
                        Err(err)
                    }
                },
            }
        })
    }

    pub(crate) async fn exec_block(
        &mut self,
        block: &Block,
        env: &mut Environment,
    ) -> Result<StmtOutcome> {
        let mut last = Value::None;
        for node in block {
            match self.exec_stmt(&node.kind, env).await? {
                StmtOutcome::Return(v) => return Ok(StmtOutcome::Return(v)),
                // Break and Continue bubble up through exec_block; the For
                // loop handler in exec_stmt catches them at the loop boundary.
                StmtOutcome::Break => return Ok(StmtOutcome::Break),
                StmtOutcome::Continue => return Ok(StmtOutcome::Continue),
                StmtOutcome::Value(v) => last = v,
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
                let lit = match &e.kind {
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
            Pattern::Struct { fields } => {
                if let Value::Struct(_, field_map) = value {
                    let mut out = Vec::with_capacity(fields.len());
                    for f in fields {
                        if f == "_" {
                            continue;
                        }
                        let v = field_map.get(f).cloned().unwrap_or(Value::None);
                        out.push((f.clone(), v));
                    }
                    Some(out)
                } else {
                    None
                }
            }
        }
    }
}

/// Control-flow signal returned by [`Interpreter::eval_expr`].
///
/// Keeps `return`-inside-expression propagation out of the [`Value`] enum.
/// All variants are internal to the interpreter execution loop and never
/// escape to user-visible code.
#[derive(Debug, Clone)]
pub(crate) enum ExprFlow {
    /// Expression evaluated to a value normally.
    Value(Value),
    /// A `return` was encountered inside an expression-position `if`/`when`
    /// body.  Propagates upward through [`Interpreter::eval_expr`] callers
    /// until it reaches an [`exec_stmt`] or call boundary, which converts it
    /// to [`StmtOutcome::Return`] or unwraps the inner value, respectively.
    Return(Value),
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
