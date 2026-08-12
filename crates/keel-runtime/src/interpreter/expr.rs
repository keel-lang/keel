use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use miette::Result;

use crate::ast::{
    BinOp, CallArg, Expr, Node, SpannedExpr, StringPart, TypeExpr, UnOp, tuple_index,
};

use super::environment::Environment;
use super::host::Host;
use super::runtime_error;
use super::state::{CallArgValue, Interpreter};
use super::stmt::{ExprFlow, StmtOutcome};
use super::value::{self, MapKey, Value};
use super::{eval_binary, is_pascal_case};

/// Parsed components of a format spec string.
struct ParsedSpec {
    align: Option<char>,
    width: Option<usize>,
    precision: Option<usize>,
    type_flag: Option<char>,
}

/// Returns true when the spec needs the raw numeric value (float coercion).
/// Alignment-only specs do not — they work on the string representation.
fn spec_needs_numeric(spec: &ParsedSpec) -> bool {
    spec.type_flag == Some('f') || spec.precision.is_some()
}

fn parse_spec(spec: &str) -> miette::Result<ParsedSpec> {
    let mut s = spec;

    let align = if s.starts_with('<') {
        s = &s[1..];
        Some('<')
    } else if s.starts_with('>') {
        s = &s[1..];
        Some('>')
    } else if s.starts_with('^') {
        s = &s[1..];
        Some('^')
    } else {
        None
    };

    let width = if s.starts_with(|c: char| c.is_ascii_digit()) {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let w: usize = s[..end].parse().unwrap_or(0);
        s = &s[end..];
        Some(w)
    } else {
        None
    };

    let precision = if s.starts_with('.') {
        s = &s[1..];
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let p: usize = s[..end].parse().unwrap_or(0);
        s = &s[end..];
        Some(p)
    } else {
        None
    };

    let type_flag = if s == "f" { Some('f') } else { None };
    if !s.is_empty() && type_flag.is_none() {
        return Err(runtime_error(format!(
            "unknown format spec type `{s}` in `:{spec}`"
        )));
    }

    Ok(ParsedSpec {
        align,
        width,
        precision,
        type_flag,
    })
}

fn apply_padding(s: String, ps: &ParsedSpec) -> String {
    match (ps.width, ps.align) {
        (Some(w), Some('<')) => format!("{:<width$}", s, width = w),
        (Some(w), Some('>')) => format!("{:>width$}", s, width = w),
        (Some(w), Some('^')) => format!("{:^width$}", s, width = w),
        (Some(w), None) => format!("{:>width$}", s, width = w),
        _ => s,
    }
}

/// Apply a raw format spec string (the part after `:` in `{expr:spec}`) to a value
/// and a pre-computed string representation of that value (from `to_str()` dispatch).
///
/// Supported grammar: `[align][width][.precision][type]`
///   align     = `<` | `>` | `^`   (space fill only — custom fill chars like `*>10` are not supported)
///   width     = integer
///   precision = `.` integer
///   type      = `f`
///
/// `base_str` is used for alignment-only specs. Float/precision specs bypass it and
/// coerce the raw numeric value directly, so both paths produce the same result for
/// numeric types while ensuring user-defined `to_str()` impls are respected for
/// alignment specs on custom types.
fn apply_format_spec(v: &Value, base_str: String, spec: &str) -> miette::Result<String> {
    let ps = parse_spec(spec)?;

    let formatted = if spec_needs_numeric(&ps) {
        // Float formatting: coerce int → float if needed.
        let f = match v {
            Value::Float(f) => *f,
            Value::Integer(i) => *i as f64,
            other => {
                return Err(runtime_error(format!(
                    "format spec `:{spec}` requires a float or int, got {}",
                    other.type_name()
                )));
            }
        };
        let prec = ps.precision.unwrap_or(6);
        format!("{:.prec$}", f, prec = prec)
    } else {
        // Alignment-only: use the caller-resolved string (respects to_str() impls).
        base_str
    };

    Ok(apply_padding(formatted, &ps))
}

/// Outcome from evaluating a single argument list.
enum ArgsResult {
    Args(Vec<CallArgValue>),
    Return(Value),
}

fn mock_target_from_expr(expr: &SpannedExpr) -> Option<(&str, &str)> {
    let Expr::FieldAccess(object, method) = &expr.kind else {
        return None;
    };
    let Expr::Ident(namespace) = &object.as_ref().kind else {
        return None;
    };
    Some((namespace.as_str(), method.as_str()))
}

impl Interpreter {
    /// Evaluate an expression against a live, paused frame's environment —
    /// the `evaluate` DAP request's implementation. `env` is typically the
    /// exact `&mut Environment` a `DebugHook::on_statement` call is holding
    /// while paused, so mutations the expression makes (e.g. a debug-console
    /// assignment) are immediately visible once execution resumes.
    pub async fn eval_in_frame(
        &mut self,
        spanned: &SpannedExpr,
        env: &mut Environment,
    ) -> Result<Value> {
        match self.eval_expr(spanned, env).await? {
            ExprFlow::Value(v) | ExprFlow::Return(v) => Ok(v),
        }
    }

    pub(crate) fn eval_expr<'a>(
        &'a mut self,
        spanned: &'a SpannedExpr,
        env: &'a mut Environment,
    ) -> Pin<Box<dyn Future<Output = Result<ExprFlow>> + Send + 'a>> {
        Box::pin(async move {
            let expr = &spanned.kind;
            match expr {
                Expr::Integer(n) => Ok(ExprFlow::Value(Value::Integer(*n))),
                Expr::Float(f) => Ok(ExprFlow::Value(Value::Float(*f))),
                Expr::Bool(b) => Ok(ExprFlow::Value(Value::Bool(*b))),
                Expr::None_ => Ok(ExprFlow::Value(Value::None)),

                Expr::StringLit(parts) => {
                    let mut out = String::new();
                    for p in parts {
                        match p {
                            StringPart::Literal(s) => out.push_str(s),
                            // ParseError slots should have been rejected by the type checker.
                            // Treat them as a runtime error defensively.
                            StringPart::ParseError(raw) => {
                                return Err(runtime_error(format!(
                                    "invalid expression in string interpolation: `{raw}`"
                                )));
                            }
                            StringPart::Interpolation(e, spec) => {
                                let v = match self.eval_expr(e, env).await? {
                                    ExprFlow::Value(v) => v,
                                    early => return Ok(early),
                                };
                                // Always resolve to_str() via method dispatch first so
                                // user-defined impl Stringable blocks are respected.
                                // Format specs receive this string for alignment; float
                                // specs bypass it and coerce the raw numeric value.
                                let base = match self
                                    .call_method_on_value(v.clone(), "to_str", vec![])
                                    .await
                                {
                                    Ok(Value::String(s)) => s,
                                    _ => v.to_string(),
                                };
                                let s = if let Some(raw_spec) = spec {
                                    apply_format_spec(&v, base, raw_spec)?
                                } else {
                                    base
                                };
                                out.push_str(&s);
                            }
                        }
                    }
                    Ok(ExprFlow::Value(Value::String(out)))
                }

                Expr::Ident(name) => self.lookup_ident(name, env).map(ExprFlow::Value),

                Expr::SelfAccess { field, .. } => {
                    // impl block receiver: `self` bound in local env as a Map or Struct
                    if let Some(v) = env.get("self").cloned() {
                        return match &v {
                            Value::Map(_) | Value::Struct(_, _) => v
                                .get_str_field(field)
                                .cloned()
                                .ok_or_else(|| {
                                    runtime_error(format!("impl receiver has no field `{field}`"))
                                })
                                .map(ExprFlow::Value),
                            _ => Err(runtime_error(format!(
                                "impl receiver is not a struct (got {})",
                                v.type_name()
                            ))),
                        };
                    }
                    if let Some(agent) = &self.current_agent {
                        let inst = agent.lock();
                        inst.state
                            .get(field)
                            .cloned()
                            .ok_or_else(|| {
                                runtime_error(format!("Agent has no state field `{field}`"))
                            })
                            .map(ExprFlow::Value)
                    } else {
                        // Common cause: invoking `self.{field}` from a
                        // closure that runs outside any agent context —
                        // e.g. an `Http.serve(...)` handler. Those run
                        // on the event loop with no `current_agent`,
                        // so `self.` cannot resolve. Route through
                        // `Agent.send(MyAgent, data, event: "...")` to
                        // hand off to a running agent instead.
                        Err(runtime_error(format!(
                            "`self.{field}` used outside an agent context — \
                             Http.serve handlers and other top-level closures \
                             cannot access agent state. Use \
                             `Agent.send(MyAgent, data, event: \"http_request\")` \
                             to route into a running agent."
                        )))
                    }
                }

                Expr::SelfRef => {
                    // impl block receiver: bare `self` resolves to the Map value
                    if let Some(v) = env.get("self").cloned() {
                        return Ok(ExprFlow::Value(v));
                    }
                    if let Some(agent) = &self.current_agent {
                        let name = agent.lock().def.name.clone();
                        Ok(ExprFlow::Value(Value::AgentRef(name)))
                    } else {
                        Err(runtime_error(
                            "`self` used outside of an agent context".to_string(),
                        ))
                    }
                }

                Expr::FieldAccess(obj, field) => {
                    if matches!(field.as_str(), "called" | "call_count")
                        && let Some((namespace, method)) = mock_target_from_expr(obj)
                    {
                        return match field.as_str() {
                            "called" => self
                                .test_mock_called(namespace, method)
                                .map(Value::Bool)
                                .map(ExprFlow::Value)
                                .ok_or_else(|| {
                                    runtime_error(format!(
                                        "`{namespace}.{method}.{field}` requires \
                                         `testing.mock({namespace}.{method}).returns(...)` in \
                                         the current test"
                                    ))
                                }),
                            "call_count" => self
                                .test_mock_call_count(namespace, method)
                                .map(Value::Integer)
                                .map(ExprFlow::Value)
                                .ok_or_else(|| {
                                    runtime_error(format!(
                                        "`{namespace}.{method}.{field}` requires \
                                         `testing.mock({namespace}.{method}).returns(...)` in \
                                         the current test"
                                    ))
                                }),
                            _ => unreachable!("metadata field was prefiltered"),
                        };
                    }
                    if let Expr::Ident(namespace) = &obj.as_ref().kind
                        && keel_catalog::catalog_method(namespace, field).is_some()
                    {
                        return Ok(ExprFlow::Value(Value::MockTarget {
                            namespace: namespace.clone(),
                            method: field.clone(),
                        }));
                    }
                    if let Expr::Ident(name) = &obj.as_ref().kind
                        && name == "uuid"
                        && let Some(value) =
                            crate::runtime::namespaces::uuid::uuid_namespace_constant(field)
                    {
                        return Ok(ExprFlow::Value(value));
                    }
                    // Agent handler reference: `Foo.process` where Foo is a registered
                    // agent. Produces an AgentHandlerRef consumed by Agent.delegate.
                    if let Expr::Ident(name) = &obj.as_ref().kind
                        && self.store.agents.contains_key(name)
                    {
                        return Ok(ExprFlow::Value(Value::AgentHandlerRef(
                            name.clone(),
                            field.clone(),
                        )));
                    }
                    // Enum variant access: `Urgency.medium`. If `obj` is
                    // a bare identifier naming a registered type, produce
                    // an EnumVariant directly (don't evaluate `obj`, which
                    // might not be bound as a Value).
                    if let Expr::Ident(name) = &obj.as_ref().kind
                        && !self.store.agents.contains_key(name)
                        && self
                            .globals
                            .get(name)
                            .is_none_or(|v| matches!(v, Value::Namespace(_)))
                        && is_pascal_case(name)
                    {
                        return Ok(ExprFlow::Value(Value::EnumVariant(
                            name.clone(),
                            field.clone(),
                            None,
                        )));
                    }
                    let obj_v = match self.eval_expr(obj, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    match &obj_v {
                        Value::Namespace(ns_name) => {
                            if ns_name == "uuid"
                                && let Some(value) =
                                    crate::runtime::namespaces::uuid::uuid_namespace_constant(field)
                            {
                                return Ok(ExprFlow::Value(value));
                            }
                            // Aliased std binding (`use std/file as f`) used as a
                            // mock target: resolve through the canonical name.
                            if keel_catalog::catalog_method(ns_name, field).is_some() {
                                return Ok(ExprFlow::Value(Value::MockTarget {
                                    namespace: ns_name.clone(),
                                    method: field.clone(),
                                }));
                            }
                            Ok(ExprFlow::Value(Value::EnumVariant(
                                ns_name.clone(),
                                field.clone(),
                                None,
                            )))
                        }
                        Value::Module(module) => {
                            if module.tasks.contains(field) {
                                if let Some(value) = self.globals.get(field) {
                                    return Ok(ExprFlow::Value(value.clone()));
                                }
                                return Err(runtime_error(format!(
                                    "module `{}` member `{field}` is not registered",
                                    module.name
                                )));
                            }
                            if module.agents.contains(field) {
                                return Ok(ExprFlow::Value(Value::AgentRef(field.clone())));
                            }
                            Err(runtime_error(format!(
                                "module `{}` has no member `{field}`",
                                module.name
                            )))
                        }
                        Value::Map(_) | Value::Struct(_, _) => {
                            if let Some(v) = obj_v.get_str_field(field) {
                                return Ok(ExprFlow::Value(v.clone()));
                            }
                            // Fall through to property-style method call.
                            self.call_method_on_value(obj_v.clone(), field, vec![])
                                .await
                                .map(ExprFlow::Value)
                                .map_err(|_| runtime_error(format!("Value has no field `{field}`")))
                        }
                        // Positional tuple access (`pair.0`, `SPEC.md` §2.8).
                        // Tuples share the list repr in v0.1 (see
                        // `cast_tuple` below), so this arm cannot tell a
                        // tuple from a list — the type checker is the gate
                        // that rejects `[1, 2].0`, since `Ty::Tuple` is
                        // distinct from `Ty::List` there. Compiled tuples
                        // are by-value LLVM structs, so the two engines
                        // only diverge on input the checker already
                        // rejects; do not "fix" that asymmetry here.
                        Value::List(items) if tuple_index(field).is_some() => {
                            let idx = tuple_index(field).expect("guarded above");
                            items.get(idx).cloned().map(ExprFlow::Value).ok_or_else(|| {
                                runtime_error(format!(
                                    "tuple index {idx} is out of bounds (arity {})",
                                    items.len()
                                ))
                            })
                        }
                        _ => {
                            // Zero-arg method fallback for properties
                            // like `.count`, `.length`, `.is_empty`.
                            self.call_method_on_value(obj_v.clone(), field, vec![])
                                .await
                                .map(ExprFlow::Value)
                                .map_err(|_| {
                                    runtime_error(format!(
                                        "Cannot access `.{field}` on {}",
                                        obj_v.type_name()
                                    ))
                                })
                        }
                    }
                }

                Expr::NullFieldAccess(obj, field) => {
                    let obj_v = match self.eval_expr(obj, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    if matches!(obj_v, Value::None) {
                        Ok(ExprFlow::Value(Value::None))
                    } else {
                        let field_access = Node::new(
                            Expr::FieldAccess(obj.clone(), field.clone()),
                            obj.as_ref().span.clone(),
                        );
                        self.eval_expr(&field_access, env).await
                    }
                }

                Expr::NullAssert(e) => {
                    let v = match self.eval_expr(e, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    if matches!(v, Value::None) {
                        Err(runtime_error("NullError: `!.` on none"))
                    } else {
                        Ok(ExprFlow::Value(v))
                    }
                }

                Expr::StructLit(fields) => {
                    let mut m = HashMap::new();
                    for (k, v) in fields {
                        let val = match self.eval_expr(v, env).await? {
                            ExprFlow::Value(v) => v,
                            early => return Ok(early),
                        };
                        let key = match k {
                            crate::ast::MapLitKey::Ident(s) | crate::ast::MapLitKey::Str(s) => {
                                MapKey::Str(s.clone())
                            }
                            crate::ast::MapLitKey::Int(n) => MapKey::Int(*n),
                            crate::ast::MapLitKey::Bool(b) => MapKey::Bool(*b),
                        };
                        m.insert(key, val);
                    }
                    Ok(ExprFlow::Value(Value::Map(m)))
                }

                Expr::StructSpreadUpdate { base, overrides } => {
                    let base_val = match self.eval_expr(base, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    match base_val {
                        Value::Struct(type_name, mut fields) => {
                            if let Some(schema) = self.struct_types.get(&type_name) {
                                for (k, _) in overrides.iter() {
                                    if !schema.iter().any(|(f, _)| f == k) {
                                        return Err(runtime_error(format!(
                                            "unknown field `{}` in spread-update — \
                                             not a field of `{}`",
                                            k, type_name
                                        )));
                                    }
                                }
                            }
                            for (k, v) in overrides {
                                let val = match self.eval_expr(v, env).await? {
                                    ExprFlow::Value(v) => v,
                                    early => return Ok(early),
                                };
                                fields.insert(k.clone(), val);
                            }
                            Ok(ExprFlow::Value(Value::Struct(type_name, fields)))
                        }
                        Value::Map(mut fields) => {
                            for (k, v) in overrides {
                                let val = match self.eval_expr(v, env).await? {
                                    ExprFlow::Value(v) => v,
                                    early => return Ok(early),
                                };
                                fields.insert(MapKey::Str(k.clone()), val);
                            }
                            Ok(ExprFlow::Value(Value::Map(fields)))
                        }
                        other => Err(runtime_error(format!(
                            "spread-update `{{...base}}` requires a struct or map, got {}",
                            other.type_name()
                        ))),
                    }
                }

                Expr::ListLit(items) => {
                    let mut out = Vec::with_capacity(items.len());
                    for it in items {
                        match self.eval_expr(it, env).await? {
                            ExprFlow::Value(v) => out.push(v),
                            early => return Ok(early),
                        }
                    }
                    Ok(ExprFlow::Value(Value::List(out)))
                }

                Expr::SetLit(items) => {
                    let mut out = Vec::with_capacity(items.len());
                    for it in items {
                        match self.eval_expr(it, env).await? {
                            // Duplicate literal elements collapse here rather
                            // than being an error — `set[1, 1]` is a set of
                            // one, matching how every other set operation
                            // treats a re-added element.
                            ExprFlow::Value(v) => {
                                value::set_insert(&mut out, v);
                            }
                            early => return Ok(early),
                        }
                    }
                    Ok(ExprFlow::Value(Value::Set(out)))
                }

                Expr::TupleLit(items) => {
                    let mut out = Vec::with_capacity(items.len());
                    for it in items {
                        match self.eval_expr(it, env).await? {
                            ExprFlow::Value(v) => out.push(v),
                            early => return Ok(early),
                        }
                    }
                    Ok(ExprFlow::Value(Value::List(out)))
                }

                Expr::BinaryOp { left, op, right } => {
                    let l = match self.eval_expr(left, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    // `and`/`or` short-circuit: the right operand is only
                    // evaluated when the left one didn't already decide the
                    // result (issue #224). Every other operator evaluates
                    // both operands unconditionally.
                    match op {
                        BinOp::And if !l.is_truthy() => {
                            return Ok(ExprFlow::Value(Value::Bool(false)));
                        }
                        BinOp::Or if l.is_truthy() => {
                            return Ok(ExprFlow::Value(Value::Bool(true)));
                        }
                        _ => {}
                    }
                    let r = match self.eval_expr(right, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    eval_binary(*op, l, r).map(ExprFlow::Value)
                }

                Expr::UnaryOp { op, expr: inner } => {
                    let v = match self.eval_expr(inner, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    match op {
                        UnOp::Neg => match v {
                            Value::Integer(n) => Ok(ExprFlow::Value(Value::Integer(-n))),
                            Value::Float(f) => Ok(ExprFlow::Value(Value::Float(-f))),
                            other => Err(runtime_error(format!(
                                "Cannot negate {}",
                                other.type_name()
                            ))),
                        },
                        UnOp::Not => Ok(ExprFlow::Value(Value::Bool(!v.is_truthy()))),
                    }
                }

                Expr::NullCoalesce(left, right) => {
                    let l = match self.eval_expr(left, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    if matches!(l, Value::None) {
                        self.eval_expr(right, env).await
                    } else {
                        Ok(ExprFlow::Value(l))
                    }
                }

                Expr::Range(start, end) => {
                    let s = match self.eval_expr(start, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    let e = match self.eval_expr(end, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    match (s, e) {
                        (Value::Integer(lo), Value::Integer(hi)) => {
                            Ok(ExprFlow::Value(Value::Range(lo, hi)))
                        }
                        (l, r) => Err(runtime_error(format!(
                            "range `..` expects two integers, got {} and {}",
                            l.type_name(),
                            r.type_name()
                        ))),
                    }
                }

                Expr::Pipeline(left, right) => {
                    // `x |> f` ≡ `f(x)` (single positional argument)
                    let l = match self.eval_expr(left, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    let args = vec![CallArgValue {
                        name: None,
                        value: l,
                    }];
                    self.call_value(right, args, env).await
                }

                Expr::Call { callee, args } => {
                    let arg_values = match self.eval_args(args, env).await? {
                        ArgsResult::Args(a) => a,
                        ArgsResult::Return(v) => return Ok(ExprFlow::Return(v)),
                    };
                    if let Expr::SelfAccess {
                        field: task_name, ..
                    } = &callee.as_ref().kind
                    {
                        return self
                            .call_current_agent_task(task_name, arg_values)
                            .await
                            .map(ExprFlow::Value);
                    }
                    self.call_value(callee, arg_values, env).await
                }

                Expr::MethodCall {
                    object,
                    method,
                    args,
                } => {
                    let arg_values = match self.eval_args(args, env).await? {
                        ArgsResult::Args(a) => a,
                        ArgsResult::Return(v) => return Ok(ExprFlow::Return(v)),
                    };
                    if method == "returns" {
                        let obj_val = match self.eval_expr(object, env).await? {
                            ExprFlow::Value(v) => v,
                            early => return Ok(early),
                        };
                        if let Value::MockHandle { namespace, method } = obj_val {
                            let value = arg_values
                                .into_iter()
                                .next()
                                .map(|arg| arg.value)
                                .unwrap_or(Value::None);
                            self.register_test_mock_return(&namespace, &method, value);
                            return Ok(ExprFlow::Value(Value::None));
                        }
                        return Err(runtime_error(format!(
                            "`.returns(...)` can only be called on a mock, got {}",
                            obj_val.type_name()
                        )));
                    }
                    if method == "called_with"
                        && let Some((namespace, target_method)) = mock_target_from_expr(object)
                    {
                        return self
                            .test_mock_called_with(namespace, target_method, &arg_values)
                            .map(Value::Bool)
                            .map(ExprFlow::Value)
                            .ok_or_else(|| {
                                runtime_error(format!(
                                    "`{namespace}.{target_method}.called_with(...)` requires \
                                     `testing.mock({namespace}.{target_method}).returns(...)` in \
                                     the current test"
                                ))
                            });
                    }
                    if matches!(&object.as_ref().kind, Expr::SelfRef) {
                        return self
                            .call_current_agent_task(method, arg_values)
                            .await
                            .map(ExprFlow::Value);
                    }
                    // If object is a namespace, dispatch to its method.
                    let obj_val = match self.eval_expr(object, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    if let Value::Namespace(ns) = &obj_val {
                        let ns_name = ns.clone();
                        return self
                            .call_namespace_method(&ns_name, method, arg_values)
                            .await
                            .map(ExprFlow::Value);
                    }
                    // Module member call: `validation.email(...)` resolves the
                    // member task from the flat global table.
                    if let Value::Module(module) = &obj_val {
                        if module.tasks.contains(method.as_str())
                            && let Some(Value::Task(name, decl)) =
                                self.globals.get(method.as_str()).cloned()
                        {
                            return self
                                .call_task(&name, &decl, arg_values)
                                .await
                                .map(ExprFlow::Value);
                        }
                        if module.agents.contains(method.as_str()) {
                            return Err(runtime_error(format!(
                                "`{}.{method}` is an agent, not a task — start it with \
                                 run({}.{method}) or message it with send(...)",
                                module.name, module.name
                            )));
                        }
                        return Err(runtime_error(format!(
                            "module `{}` has no member `{method}`",
                            module.name
                        )));
                    }
                    // Agent references are still first-class values for lifecycle
                    // and mailbox APIs, but direct cross-agent task invocation is not.
                    if let Value::AgentRef(name) = &obj_val {
                        return Err(runtime_error(format!(
                            "direct agent task calls like `{name}.{method}(...)` are unsupported; \
                             use `self.{method}(...)` inside that agent or mailbox APIs such as \
                             `send(...)` / `delegate(...)` for cross-agent work"
                        )));
                    }
                    // Otherwise: method on a value (e.g., list.map).
                    self.call_method_on_value(obj_val, method, arg_values)
                        .await
                        .map(ExprFlow::Value)
                }

                Expr::Cast { expr: inner, ty } => {
                    let val = match self.eval_expr(inner, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    apply_cast(val, &ty.kind).map(ExprFlow::Value)
                }

                Expr::Index { object, index } => {
                    let obj = match self.eval_expr(object, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    let idx = match self.eval_expr(index, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    if let Value::Map(m) = obj {
                        let key = crate::interpreter::value::MapKey::from_value(&idx).ok_or_else(
                            || {
                                runtime_error(format!(
                                    "map key must be str, int, or bool, got {}",
                                    idx.type_name()
                                ))
                            },
                        )?;
                        return Ok(ExprFlow::Value(m.get(&key).cloned().unwrap_or(Value::None)));
                    }
                    let i = match &idx {
                        Value::Integer(n) => *n,
                        other => {
                            return Err(runtime_error(format!(
                                "subscript index must be int, got {}",
                                other.type_name()
                            )));
                        }
                    };
                    match obj {
                        Value::List(items) => {
                            if i < 0 || i as usize >= items.len() {
                                Err(runtime_error(format!(
                                    "index {i} out of bounds (length {})",
                                    items.len()
                                )))
                            } else {
                                Ok(ExprFlow::Value(items[i as usize].clone()))
                            }
                        }
                        Value::String(s) => {
                            let chars: Vec<char> = s.chars().collect();
                            if i < 0 || i as usize >= chars.len() {
                                Err(runtime_error(format!(
                                    "string index {i} out of bounds (length {})",
                                    chars.len()
                                )))
                            } else {
                                Ok(ExprFlow::Value(Value::String(
                                    chars[i as usize].to_string(),
                                )))
                            }
                        }
                        other => Err(runtime_error(format!(
                            "subscript `[i]` is not supported on {}",
                            other.type_name()
                        ))),
                    }
                }

                Expr::IfExpr {
                    cond,
                    then_body,
                    else_body,
                } => {
                    let c = match self.eval_expr(cond, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    if c.is_truthy() {
                        match self.exec_block(then_body, env).await? {
                            StmtOutcome::Return(v) => Ok(ExprFlow::Return(v)),
                            StmtOutcome::Value(v) => Ok(ExprFlow::Value(v)),
                            StmtOutcome::Normal => Ok(ExprFlow::Value(Value::None)),
                            StmtOutcome::Break | StmtOutcome::Continue => {
                                Err(runtime_error("`break`/`continue` inside an expression"))
                            }
                        }
                    } else {
                        match self.exec_block(else_body, env).await? {
                            StmtOutcome::Return(v) => Ok(ExprFlow::Return(v)),
                            StmtOutcome::Value(v) => Ok(ExprFlow::Value(v)),
                            StmtOutcome::Normal => Ok(ExprFlow::Value(Value::None)),
                            StmtOutcome::Break | StmtOutcome::Continue => {
                                Err(runtime_error("`break`/`continue` inside an expression"))
                            }
                        }
                    }
                }

                Expr::WhenExpr { subject, arms } => {
                    let s = match self.eval_expr(subject, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
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
                                    early => {
                                        env.pop_scope();
                                        return Ok(early);
                                    }
                                };
                                if !guard_val.is_truthy() {
                                    env.pop_scope();
                                    continue;
                                }
                            }
                            let result = match self.exec_block(&arm.body, env).await? {
                                StmtOutcome::Return(v) => ExprFlow::Return(v),
                                StmtOutcome::Value(v) => ExprFlow::Value(v),
                                StmtOutcome::Normal => ExprFlow::Value(Value::None),
                                StmtOutcome::Break | StmtOutcome::Continue => {
                                    env.pop_scope();
                                    return Err(runtime_error(
                                        "`break`/`continue` inside an expression",
                                    ));
                                }
                            };
                            env.pop_scope();
                            return Ok(result);
                        }
                    }
                    Ok(ExprFlow::Value(Value::None))
                }

                Expr::Lambda { params, body } => Ok(ExprFlow::Value(Value::Closure(
                    params.clone(),
                    Box::new(body.clone()),
                ))),

                Expr::Duration { value, unit } => {
                    let v = match self.eval_expr(value, env).await? {
                        ExprFlow::Value(v) => v,
                        early => return Ok(early),
                    };
                    let n = v
                        .as_int()
                        .ok_or_else(|| runtime_error("duration value must be int"))?;
                    Ok(ExprFlow::Value(Value::Duration(Value::duration_seconds(
                        n, *unit,
                    ))))
                }

                Expr::EnumVariant {
                    ty,
                    variant,
                    fields,
                } => {
                    if fields.is_empty() {
                        Ok(ExprFlow::Value(Value::EnumVariant(
                            ty.clone(),
                            variant.clone(),
                            None,
                        )))
                    } else {
                        let mut evaluated = HashMap::new();
                        for (k, v) in fields {
                            let val = match self.eval_expr(v, env).await? {
                                ExprFlow::Value(v) => v,
                                early => return Ok(early),
                            };
                            evaluated.insert(k.clone(), val);
                        }
                        Ok(ExprFlow::Value(Value::EnumVariant(
                            ty.clone(),
                            variant.clone(),
                            Some(evaluated),
                        )))
                    }
                }
            }
        })
    }
    fn lookup_ident(&self, name: &str, env: &Environment) -> Result<Value> {
        if let Some(v) = env.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.globals.get(name) {
            return Ok(v.clone());
        }
        Err(runtime_error(format!("Undefined: `{name}`")))
    }

    async fn eval_args(&mut self, args: &[CallArg], env: &mut Environment) -> Result<ArgsResult> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = match self.eval_expr(&a.value, env).await? {
                ExprFlow::Value(v) => v,
                ExprFlow::Return(v) => return Ok(ArgsResult::Return(v)),
            };
            if a.spread {
                // Expand a list or set into individual positional args
                // (SPEC §11: `...expr` requires `list[T]` or `set[T]`). A set
                // expands in insertion order, so the slot assignment is
                // deterministic.
                let items = match v {
                    Value::List(items) | Value::Set(items) => items,
                    other => {
                        return Err(super::runtime_error(format!(
                            "spread `...` requires a list or set, got {}",
                            other.type_name()
                        )));
                    }
                };
                for item in items {
                    out.push(CallArgValue {
                        name: None,
                        value: item,
                    });
                }
            } else {
                out.push(CallArgValue {
                    name: a.name.clone(),
                    value: v,
                });
            }
        }
        Ok(ArgsResult::Args(out))
    }

    fn call_value<'a>(
        &'a mut self,
        callee: &'a SpannedExpr,
        args: Vec<CallArgValue>,
        env: &'a mut Environment,
    ) -> Pin<Box<dyn Future<Output = Result<ExprFlow>> + Send + 'a>> {
        Box::pin(async move {
            let callee_v = match self.eval_expr(callee, env).await? {
                ExprFlow::Value(v) => v,
                early => return Ok(early),
            };
            match callee_v {
                Value::Task(name, decl) => self
                    .call_task(&name, &decl, args)
                    .await
                    .map(ExprFlow::Value),
                Value::BuiltinFn(name) => self
                    .call_namespace_method("__global", &name, args)
                    .await
                    .map(ExprFlow::Value),
                Value::Namespace(_) => Err(runtime_error("Cannot call a namespace directly")),
                Value::Closure(params, body) => self
                    .call_closure(&params, &body, args)
                    .await
                    .map(ExprFlow::Value),
                other => Err(runtime_error(format!("Cannot call {}", other.type_name()))),
            }
        })
    }
}

fn is_valid_uuid(s: &str) -> bool {
    let raw = s.strip_prefix("urn:uuid:").unwrap_or(s);
    let hex: String = raw.chars().filter(|&c| c != '-').collect();
    hex.len() == 32 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn apply_cast(val: Value, ty: &TypeExpr) -> Result<Value> {
    let target = match ty {
        TypeExpr::Named(n) => n.as_str(),
        TypeExpr::Dynamic => return Ok(val),
        TypeExpr::Nullable(inner) => {
            return apply_cast(val, inner);
        }
        // Container casts assert the runtime shape and recurse element-wise.
        // A `dynamic` element/value type short-circuits to pass-through so the
        // common `json.parse(x) as list[dynamic]` path does no per-element work.
        TypeExpr::List(inner) => return cast_list(val, inner),
        TypeExpr::Map(_k, v_inner) => return cast_map(val, v_inner),
        TypeExpr::Tuple(items) => return cast_tuple(val, items),
        // `set[T]` is not writable as a cast target in v0.1 (`set` is a reserved
        // keyword, so the type parser never produces a `Set` here); it joins the
        // unsupported targets below. Inline struct literals, function types,
        // generic applications, and the synthetic `SelfType` are likewise never
        // `as` targets — raise per the SPEC default ("unsupported conversions
        // raise").
        TypeExpr::Set(_)
        | TypeExpr::Struct(_)
        | TypeExpr::Func(_, _)
        | TypeExpr::Generic(_, _)
        | TypeExpr::SelfType => {
            return Err(runtime_error(format!("cannot cast to {ty:?}")));
        }
    };

    // identity: typeof(val) == target → pass through
    let val_type = match &val {
        Value::Struct(name, _) | Value::EnumVariant(name, _, _) => name.as_str(),
        other => other.type_name(),
    };
    if val_type == target {
        return Ok(val);
    }

    match (val, target) {
        // int <-> float
        (Value::Integer(n), "float") => Ok(Value::Float(n as f64)),
        (Value::Float(f), "int") => Ok(Value::Integer(f as i64)),

        // numeric -> str
        (Value::Integer(n), "str") => Ok(Value::String(n.to_string())),
        (Value::Float(f), "str") => Ok(Value::String(f.to_string())),
        (Value::Bool(b), "str") => Ok(Value::String(b.to_string())),

        // str -> numeric
        (Value::String(s), "int") => s
            .trim()
            .parse::<i64>()
            .map(Value::Integer)
            .map_err(|_| runtime_error(format!("cannot cast \"{s}\" to int"))),
        (Value::String(s), "float") => s
            .trim()
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| runtime_error(format!("cannot cast \"{s}\" to float"))),
        (Value::String(s), "bool") => match s.trim() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(runtime_error(format!("cannot cast \"{s}\" to bool"))),
        },

        // Uuid <-> str
        (Value::Uuid(s), "str") => Ok(Value::String(s)),
        (Value::String(s), "Uuid") => {
            if is_valid_uuid(&s) {
                Ok(Value::Uuid(s))
            } else {
                Err(runtime_error(format!(
                    "cannot cast \"{s}\" to Uuid: invalid UUID format"
                )))
            }
        }

        // none -> anything raises
        (Value::None, target) => Err(runtime_error(format!("cannot cast none to {target}"))),

        (val, target) => Err(runtime_error(format!(
            "cannot cast {} to {target}",
            val.type_name()
        ))),
    }
}

/// Cast a value to `list[inner]` (sets share the list repr in v0.1, but `set`
/// is not writable as a cast target, so this is reached only for `list[T]`).
/// Asserts the value is a list, then recurses the element cast to `inner`.
fn cast_list(val: Value, inner: &TypeExpr) -> Result<Value> {
    match val {
        // `list[dynamic]` is a pass-through — keep the original allocation.
        Value::List(items) if matches!(inner, TypeExpr::Dynamic) => Ok(Value::List(items)),
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(apply_cast(item, inner)?);
            }
            Ok(Value::List(out))
        }
        other => Err(runtime_error(format!(
            "cannot cast {} to list",
            other.type_name()
        ))),
    }
}

/// Cast a value to `map[_, v_inner]`. Asserts the value is a map, then recurses
/// the value cast to `v_inner`. Keys pass through unchanged — v0.1 does not
/// coerce or re-validate map keys (the JSON case always has string keys).
fn cast_map(val: Value, v_inner: &TypeExpr) -> Result<Value> {
    match val {
        Value::Map(m) if matches!(v_inner, TypeExpr::Dynamic) => Ok(Value::Map(m)),
        Value::Map(m) => {
            let mut out = HashMap::with_capacity(m.len());
            for (k, v) in m {
                out.insert(k, apply_cast(v, v_inner)?);
            }
            Ok(Value::Map(out))
        }
        other => Err(runtime_error(format!(
            "cannot cast {} to map",
            other.type_name()
        ))),
    }
}

/// Cast a value to `tuple[items...]`. Tuples share the list repr in v0.1, so
/// the value must be a list of the same arity; each element is cast position-wise.
fn cast_tuple(val: Value, items: &[TypeExpr]) -> Result<Value> {
    match val {
        Value::List(elems) if elems.len() == items.len() => {
            let mut out = Vec::with_capacity(elems.len());
            for (elem, ty) in elems.into_iter().zip(items) {
                out.push(apply_cast(elem, ty)?);
            }
            Ok(Value::List(out))
        }
        Value::List(elems) => Err(runtime_error(format!(
            "cannot cast list of {} element(s) to tuple of {}",
            elems.len(),
            items.len()
        ))),
        other => Err(runtime_error(format!(
            "cannot cast {} to tuple",
            other.type_name()
        ))),
    }
}
