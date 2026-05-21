use std::collections::HashSet;

use miette::Result;

use super::environment::Environment;
use super::runtime_error;
use super::state::{CallArgValue, Interpreter};
use super::value::Value;

impl Interpreter {
    /// Find the `impl` TaskDecl for `method` on `value`.
    /// For type-tagged `Value::Struct` this is O(1); for untagged `Value::Map`
    /// (struct literals with no type annotation) a field-set fallback is used.
    /// Returns `None` if no impl is found.
    /// Clones the TaskDecl to avoid borrow-across-await issues.
    pub(crate) fn find_impl_task(
        &self,
        value: &Value,
        method: &str,
    ) -> Option<crate::ast::TaskDecl> {
        match value {
            Value::Struct(type_name, _) => self
                .impl_methods
                .get(type_name.as_str())?
                .get(method)
                .cloned(),
            Value::Map(m) => {
                // Fallback for untagged maps (no type annotation at binding site).
                let map_keys: HashSet<&str> = m.keys().map(String::as_str).collect();
                self.impl_methods.iter().find_map(|(type_name, methods)| {
                    methods.get(method).and_then(|task| {
                        self.struct_types.get(type_name).and_then(|schema| {
                            let type_fields: HashSet<&str> =
                                schema.iter().map(|(k, _)| k.as_str()).collect();
                            if type_fields.is_subset(&map_keys) {
                                Some(task.clone())
                            } else {
                                None
                            }
                        })
                    })
                })
            }
            _ => None,
        }
    }

    pub(crate) async fn call_method_on_value(
        &mut self,
        obj: Value,
        method: &str,
        args: Vec<CallArgValue>,
        _env: &mut Environment,
    ) -> Result<Value> {
        // Impl methods always win over built-in map methods so that user-defined
        // interfaces can shadow names like "size", "len", etc. on struct types.
        if let Some(task) = self.find_impl_task(&obj, method) {
            let mut call_args = vec![CallArgValue {
                name: None,
                value: obj,
            }];
            call_args.extend(args);
            return self.call_task(method, &task, call_args).await;
        }

        // Minimal built-in methods for v0.1. Extend as examples need.
        match (&obj, method) {
            (Value::String(s), "length" | "len" | "count") => {
                Ok(Value::Integer(s.chars().count() as i64))
            }
            (Value::String(s), "is_empty") => Ok(Value::Bool(s.is_empty())),
            (Value::String(s), "to_str") => Ok(Value::String(s.clone())),
            (Value::String(s), "upper") => Ok(Value::String(s.to_uppercase())),
            (Value::String(s), "lower") => Ok(Value::String(s.to_lowercase())),
            (Value::String(s), "trim" | "strip") => Ok(Value::String(s.trim().to_string())),
            (Value::String(s), "contains") => {
                let needle = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                Ok(Value::Bool(s.contains(&needle)))
            }
            (Value::String(s), "starts_with") => {
                let prefix = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                Ok(Value::Bool(s.starts_with(prefix.as_str())))
            }
            (Value::String(s), "ends_with") => {
                let suffix = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                Ok(Value::Bool(s.ends_with(suffix.as_str())))
            }
            (Value::String(s), "replace") => {
                let from = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                let to = args
                    .get(1)
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                Ok(Value::String(s.replace(from.as_str(), &to)))
            }
            (Value::String(s), "split") => {
                let sep = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_else(|| " ".to_string());
                let parts: Vec<Value> = s
                    .split(sep.as_str())
                    .map(|p| Value::String(p.to_string()))
                    .collect();
                Ok(Value::List(parts))
            }
            (Value::String(s), "to_int") => Ok(s
                .trim()
                .parse::<i64>()
                .map(Value::Integer)
                .unwrap_or(Value::None)),
            (Value::String(s), "to_float") => Ok(s
                .trim()
                .parse::<f64>()
                .map(Value::Float)
                .unwrap_or(Value::None)),
            (Value::String(s), "repeat") => {
                let n = args
                    .first()
                    .and_then(|a| a.value.as_int())
                    .unwrap_or(0)
                    .max(0) as usize;
                Ok(Value::String(s.repeat(n)))
            }
            (Value::String(s), "slice") => {
                let chars: Vec<char> = s.chars().collect();
                let len = chars.len() as i64;
                let start = args.first().and_then(|a| a.value.as_int()).unwrap_or(0);
                let end = args.get(1).and_then(|a| a.value.as_int()).unwrap_or(len);
                let start = start.clamp(0, len) as usize;
                let end = end.clamp(0, len) as usize;
                let end = end.max(start);
                Ok(Value::String(chars[start..end].iter().collect()))
            }
            (Value::String(s), "index_of") => {
                let needle = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                Ok(s.find(needle.as_str())
                    .map(|byte_pos| Value::Integer(s[..byte_pos].chars().count() as i64))
                    .unwrap_or(Value::None))
            }
            (Value::String(s), "trim_start") => Ok(Value::String(s.trim_start().to_string())),
            (Value::String(s), "trim_end") => Ok(Value::String(s.trim_end().to_string())),
            (Value::String(s), "matches") => {
                let pattern = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .ok_or_else(|| runtime_error("matches: missing pattern argument"))?;
                let re = regex::Regex::new(&pattern)
                    .map_err(|e| runtime_error(format!("matches: invalid regex: {e}")))?;
                Ok(Value::Bool(re.is_match(s)))
            }
            (Value::String(s), "extract") => {
                let pattern = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .ok_or_else(|| runtime_error("extract: missing pattern argument"))?;
                let re = regex::Regex::new(&pattern)
                    .map_err(|e| runtime_error(format!("extract: invalid regex: {e}")))?;
                match re.captures(s) {
                    Some(caps) => match caps.get(1) {
                        Some(m) => Ok(Value::String(m.as_str().to_string())),
                        None => Ok(Value::None),
                    },
                    None => Ok(Value::None),
                }
            }
            (Value::String(s), "truncate") => {
                let max_i = args
                    .first()
                    .and_then(|a| a.value.as_int())
                    .ok_or_else(|| runtime_error("truncate: missing max argument"))?;
                if max_i < 0 {
                    return Err(runtime_error(format!(
                        "truncate: max must be non-negative, got {max_i}"
                    )));
                }
                let max_chars = max_i as usize;
                let char_count = s.chars().count();
                if char_count <= max_chars {
                    Ok(Value::String(s.clone()))
                } else {
                    let truncated: String = s.chars().take(max_chars).collect();
                    Ok(Value::String(format!("{truncated}…")))
                }
            }
            (Value::String(s), "pad") => {
                let width_i = args
                    .first()
                    .and_then(|a| a.value.as_int())
                    .ok_or_else(|| runtime_error("pad: missing width argument"))?;
                if width_i < 0 {
                    return Err(runtime_error(format!(
                        "pad: width must be non-negative, got {width_i}"
                    )));
                }
                let width = width_i as usize;
                let pad_char_str = args
                    .iter()
                    .find(|a| a.name.as_deref() == Some("char"))
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_else(|| " ".to_string());
                let pad_char = pad_char_str.chars().next().unwrap_or(' ');
                let len = s.chars().count();
                if len >= width {
                    Ok(Value::String(s.clone()))
                } else {
                    let padding: String = std::iter::repeat_n(pad_char, width - len).collect();
                    Ok(Value::String(format!("{padding}{s}")))
                }
            }
            (Value::String(s), "find_all") => {
                let pattern = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .ok_or_else(|| runtime_error("find_all: missing pattern argument"))?;
                let re = regex::Regex::new(&pattern)
                    .map_err(|e| runtime_error(format!("find_all: invalid regex: {e}")))?;
                let matches: Vec<Value> = re
                    .find_iter(s)
                    .map(|m| Value::String(m.as_str().to_string()))
                    .collect();
                Ok(Value::List(matches))
            }
            (Value::String(s), "sub") => {
                let pattern = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .ok_or_else(|| runtime_error("sub: missing pattern argument"))?;
                let replacement = args
                    .get(1)
                    .map(|a| a.value.to_display_string())
                    .ok_or_else(|| runtime_error("sub: missing replacement argument"))?;
                let re = regex::Regex::new(&pattern)
                    .map_err(|e| runtime_error(format!("sub: invalid regex: {e}")))?;
                Ok(Value::String(
                    re.replace_all(s, replacement.as_str()).to_string(),
                ))
            }
            (Value::Range(lo, hi), "count" | "len") => {
                Ok(Value::Integer(if lo <= hi { hi - lo + 1 } else { 0 }))
            }
            (Value::Range(lo, hi), "is_empty") => Ok(Value::Bool(lo > hi)),
            (Value::Range(lo, hi), "contains") => {
                let target = args.first().and_then(|a| a.value.as_int());
                Ok(Value::Bool(target.is_some_and(|n| n >= *lo && n <= *hi)))
            }
            (Value::Range(lo, hi), "first") => Ok(if lo <= hi {
                Value::Integer(*lo)
            } else {
                Value::None
            }),
            (Value::Range(lo, hi), "last") => Ok(if lo <= hi {
                Value::Integer(*hi)
            } else {
                Value::None
            }),
            (Value::Range(lo, hi), "map") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("map expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("map argument must be a function")),
                };
                let count = if lo <= hi { (hi - lo + 1) as usize } else { 0 };
                let mut out = Vec::with_capacity(count);
                for n in *lo..=*hi {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: Value::Integer(n),
                            }],
                        )
                        .await?;
                    out.push(res);
                }
                Ok(Value::List(out))
            }
            (Value::Range(lo, hi), "filter") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("filter expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("filter argument must be a function")),
                };
                let mut out = Vec::new();
                for n in *lo..=*hi {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: Value::Integer(n),
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        out.push(Value::Integer(n));
                    }
                }
                Ok(Value::List(out))
            }
            (Value::Range(lo, hi), "push") => {
                let mut result: Vec<Value> = (*lo..=*hi).map(Value::Integer).collect();
                if let Some(arg) = args.first() {
                    result.push(arg.value.clone());
                }
                Ok(Value::List(result))
            }
            (Value::List(items), "count" | "len") => Ok(Value::Integer(items.len() as i64)),
            (Value::List(items), "is_empty") => Ok(Value::Bool(items.is_empty())),
            (Value::List(items), "contains") => {
                let target = args.first().map(|a| a.value.clone()).unwrap_or(Value::None);
                Ok(Value::Bool(items.iter().any(|v| v == &target)))
            }
            (Value::List(items), "first") => Ok(items.first().cloned().unwrap_or(Value::None)),
            (Value::List(items), "last") => Ok(items.last().cloned().unwrap_or(Value::None)),
            (Value::List(items), "map") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("map expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("map argument must be a function")),
                };
                let mut out = Vec::with_capacity(items.len());
                for item in items.iter().cloned() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item,
                            }],
                        )
                        .await?;
                    out.push(res);
                }
                Ok(Value::List(out))
            }
            (Value::List(items), "filter") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("filter expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("filter argument must be a function")),
                };
                let mut out = Vec::new();
                for item in items.iter().cloned() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item.clone(),
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        out.push(item);
                    }
                }
                Ok(Value::List(out))
            }
            (Value::List(items), "push") => {
                let mut result = items.clone();
                if let Some(arg) = args.first() {
                    result.push(arg.value.clone());
                }
                Ok(Value::List(result))
            }
            (Value::List(items), "any") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("any expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("any: argument must be a function")),
                };
                for item in items.iter().cloned() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item,
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            (Value::List(items), "all") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("all expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("all: argument must be a function")),
                };
                for item in items.iter().cloned() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item,
                            }],
                        )
                        .await?;
                    if !res.is_truthy() {
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            (Value::List(items), "find") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("find expects a function argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("find: argument must be a function")),
                };
                for item in items.iter().cloned() {
                    let res = self
                        .call_closure(
                            &params,
                            &body,
                            vec![CallArgValue {
                                name: None,
                                value: item.clone(),
                            }],
                        )
                        .await?;
                    if res.is_truthy() {
                        return Ok(item);
                    }
                }
                Ok(Value::None)
            }
            (Value::List(items), "reduce") => {
                let closure = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("reduce expects a function as first argument"))?;
                let (params, body) = match closure {
                    Value::Closure(p, b) => (p, b),
                    _ => return Err(runtime_error("reduce: first argument must be a function")),
                };
                let mut acc = args.get(1).map(|a| a.value.clone()).unwrap_or(Value::None);
                for item in items.iter().cloned() {
                    acc = self
                        .call_closure(
                            &params,
                            &body,
                            vec![
                                CallArgValue {
                                    name: None,
                                    value: acc,
                                },
                                CallArgValue {
                                    name: None,
                                    value: item,
                                },
                            ],
                        )
                        .await?;
                }
                Ok(acc)
            }
            (Value::List(items), "sum") => {
                let mut int_sum: i64 = 0;
                let mut float_sum: f64 = 0.0;
                let mut is_float = false;
                for item in items {
                    match item {
                        Value::Integer(n) => {
                            int_sum += n;
                            float_sum += *n as f64;
                        }
                        Value::Float(f) => {
                            float_sum += f;
                            is_float = true;
                        }
                        _ => return Err(runtime_error("sum: list must contain only numbers")),
                    }
                }
                if is_float {
                    Ok(Value::Float(float_sum))
                } else {
                    Ok(Value::Integer(int_sum))
                }
            }
            (Value::List(items), "min") => {
                if items.is_empty() {
                    return Ok(Value::None);
                }
                let cmp_task = self.find_impl_task(&items[0], "compare");
                if let Some(task) = cmp_task {
                    let mut result = items[0].clone();
                    for item in &items[1..] {
                        let cmp_val = self
                            .call_task(
                                "compare",
                                &task,
                                vec![
                                    CallArgValue {
                                        name: None,
                                        value: result.clone(),
                                    },
                                    CallArgValue {
                                        name: None,
                                        value: item.clone(),
                                    },
                                ],
                            )
                            .await?;
                        if matches!(cmp_val, Value::Integer(n) if n > 0) {
                            result = item.clone();
                        }
                    }
                    return Ok(result);
                }
                let mut result = items[0].clone();
                for item in &items[1..] {
                    let less = match (&result, item) {
                        (Value::Integer(a), Value::Integer(b)) => b < a,
                        (Value::Float(a), Value::Float(b)) => b < a,
                        (Value::Integer(a), Value::Float(b)) => b < &(*a as f64),
                        (Value::Float(a), Value::Integer(b)) => &(*b as f64) < a,
                        (Value::String(a), Value::String(b)) => b < a,
                        _ => false,
                    };
                    if less {
                        result = item.clone();
                    }
                }
                Ok(result)
            }
            (Value::List(items), "max") => {
                if items.is_empty() {
                    return Ok(Value::None);
                }
                let cmp_task = self.find_impl_task(&items[0], "compare");
                if let Some(task) = cmp_task {
                    let mut result = items[0].clone();
                    for item in &items[1..] {
                        let cmp_val = self
                            .call_task(
                                "compare",
                                &task,
                                vec![
                                    CallArgValue {
                                        name: None,
                                        value: result.clone(),
                                    },
                                    CallArgValue {
                                        name: None,
                                        value: item.clone(),
                                    },
                                ],
                            )
                            .await?;
                        if matches!(cmp_val, Value::Integer(n) if n < 0) {
                            result = item.clone();
                        }
                    }
                    return Ok(result);
                }
                let mut result = items[0].clone();
                for item in &items[1..] {
                    let greater = match (&result, item) {
                        (Value::Integer(a), Value::Integer(b)) => b > a,
                        (Value::Float(a), Value::Float(b)) => b > a,
                        (Value::Integer(a), Value::Float(b)) => b > &(*a as f64),
                        (Value::Float(a), Value::Integer(b)) => &(*b as f64) > a,
                        (Value::String(a), Value::String(b)) => b > a,
                        _ => false,
                    };
                    if greater {
                        result = item.clone();
                    }
                }
                Ok(result)
            }
            (Value::List(items), "join") => {
                let sep = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                let parts: Vec<String> = items.iter().map(|v| v.to_display_string()).collect();
                Ok(Value::String(parts.join(&sep)))
            }
            (Value::List(items), "sort") => {
                // If items are structs with a Comparable impl, use it (async insertion sort).
                let cmp_task = items
                    .first()
                    .and_then(|first| self.find_impl_task(first, "compare"));
                if let Some(task) = cmp_task {
                    let mut sorted = items.clone();
                    let n = sorted.len();
                    for i in 1..n {
                        let mut j = i;
                        while j > 0 {
                            let a = sorted[j - 1].clone();
                            let b = sorted[j].clone();
                            let cmp_val = self
                                .call_task(
                                    "compare",
                                    &task,
                                    vec![
                                        CallArgValue {
                                            name: None,
                                            value: a,
                                        },
                                        CallArgValue {
                                            name: None,
                                            value: b,
                                        },
                                    ],
                                )
                                .await?;
                            if matches!(cmp_val, Value::Integer(n) if n > 0) {
                                sorted.swap(j - 1, j);
                                j -= 1;
                            } else {
                                break;
                            }
                        }
                    }
                    return Ok(Value::List(sorted));
                }
                // Primitive fallback.
                let mut sorted = items.clone();
                sorted.sort_by(|a, b| match (a, b) {
                    (Value::Integer(x), Value::Integer(y)) => x.cmp(y),
                    (Value::Float(x), Value::Float(y)) => {
                        x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Value::String(x), Value::String(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                });
                Ok(Value::List(sorted))
            }
            (Value::List(items), "reverse") => {
                let mut reversed = items.clone();
                reversed.reverse();
                Ok(Value::List(reversed))
            }
            (Value::List(items), "flatten") => {
                let mut flat = Vec::new();
                for item in items {
                    match item {
                        Value::List(inner) => flat.extend(inner.clone()),
                        other => flat.push(other.clone()),
                    }
                }
                Ok(Value::List(flat))
            }
            (Value::List(items), "take") => {
                let n = args
                    .first()
                    .and_then(|a| a.value.as_int())
                    .unwrap_or(0)
                    .max(0) as usize;
                Ok(Value::List(items.iter().take(n).cloned().collect()))
            }
            (Value::List(items), "skip") => {
                let n = args
                    .first()
                    .and_then(|a| a.value.as_int())
                    .unwrap_or(0)
                    .max(0) as usize;
                Ok(Value::List(items.iter().skip(n).cloned().collect()))
            }
            (Value::List(items), "zip") => {
                let other = args
                    .first()
                    .map(|a| a.value.clone())
                    .ok_or_else(|| runtime_error("zip expects a list argument"))?;
                let other_items = match other {
                    Value::List(v) => v,
                    _ => return Err(runtime_error("zip argument must be a list")),
                };
                let pairs = items
                    .iter()
                    .zip(other_items.iter())
                    .map(|(a, b)| Value::List(vec![a.clone(), b.clone()]))
                    .collect();
                Ok(Value::List(pairs))
            }
            (Value::Map(m) | Value::Struct(_, m), "keys") => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Ok(Value::List(
                    keys.into_iter().map(|k| Value::String(k.clone())).collect(),
                ))
            }
            (Value::Map(m) | Value::Struct(_, m), "values") => {
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                Ok(Value::List(
                    keys.into_iter().map(|k| m[k].clone()).collect(),
                ))
            }
            (Value::Map(m) | Value::Struct(_, m), "get") => {
                let key = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                Ok(m.get(&key).cloned().unwrap_or(Value::None))
            }
            (Value::Map(m) | Value::Struct(_, m), "count" | "len" | "size") => {
                Ok(Value::Integer(m.len() as i64))
            }
            (Value::Map(m) | Value::Struct(_, m), "is_empty") => Ok(Value::Bool(m.is_empty())),
            (Value::Map(m) | Value::Struct(_, m), "contains" | "has") => {
                let key = args
                    .first()
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                Ok(Value::Bool(m.contains_key(&key)))
            }
            (Value::Integer(n), "abs") => Ok(Value::Integer(n.abs())),
            (Value::Integer(n), "floor" | "ceil" | "round") => Ok(Value::Integer(*n)),
            (Value::Float(f), "abs") => Ok(Value::Float(f.abs())),
            (Value::Float(f), "floor") => Ok(Value::Float(f.floor())),
            (Value::Float(f), "ceil") => Ok(Value::Float(f.ceil())),
            (Value::Float(f), "round") => Ok(Value::Float(f.round())),
            (Value::Integer(n), "to_str") => Ok(Value::String(n.to_string())),
            (Value::Float(f), "to_str") => Ok(Value::String(f.to_string())),
            (Value::Bool(b), "to_str") => Ok(Value::String(b.to_string())),
            (Value::EnumVariant(_, v, _), "to_str") => Ok(Value::String(v.clone())),
            (Value::Uuid(id), "to_str") => Ok(Value::String(id.clone())),
            (Value::Uuid(id), "version") => uuid_version(id)
                .map(Value::Integer)
                .ok_or_else(|| runtime_error(format!("Uuid.version: invalid UUID `{id}`"))),
            (Value::Uuid(id), "format") => {
                let format = args
                    .iter()
                    .find(|a| a.name.as_deref() == Some("as"))
                    .or_else(|| args.first())
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_else(|| "hyphenated".to_string());
                match format.as_str() {
                    "hyphenated" => Ok(Value::String(id.clone())),
                    "simple" => Ok(Value::String(id.replace('-', ""))),
                    "urn" => Ok(Value::String(format!("urn:uuid:{id}"))),
                    other => Err(runtime_error(format!(
                        "Uuid.format: unsupported format `{other}`; expected `hyphenated`, `simple`, or `urn`"
                    ))),
                }
            }
            // datetime methods — dispatched on strings that parse as RFC 3339
            (Value::String(s), "parts") => {
                use chrono::{Datelike, Timelike};
                match chrono::DateTime::parse_from_rfc3339(s) {
                    Ok(dt) => {
                        let mut m = std::collections::HashMap::new();
                        m.insert("year".into(), Value::Integer(dt.year() as i64));
                        m.insert("month".into(), Value::Integer(dt.month() as i64));
                        m.insert("day".into(), Value::Integer(dt.day() as i64));
                        m.insert("hour".into(), Value::Integer(dt.hour() as i64));
                        m.insert("minute".into(), Value::Integer(dt.minute() as i64));
                        m.insert("second".into(), Value::Integer(dt.second() as i64));
                        m.insert(
                            "millisecond".into(),
                            Value::Integer((dt.nanosecond() / 1_000_000) as i64),
                        );
                        m.insert("tz".into(), Value::String(dt.offset().to_string()));
                        Ok(Value::Map(m))
                    }
                    Err(_) => Ok(Value::None),
                }
            }
            (Value::String(s), "format") => {
                let pattern = args
                    .iter()
                    .find(|a| a.name.as_deref() == Some("as"))
                    .or_else(|| args.first())
                    .map(|a| a.value.to_display_string())
                    .unwrap_or_default();
                match chrono::DateTime::parse_from_rfc3339(s) {
                    Ok(dt) => Ok(Value::String(dt.format(&pattern).to_string())),
                    Err(_) => Ok(Value::None),
                }
            }
            _ => Err(runtime_error(format!(
                "Method `{method}` not available on {}",
                obj.type_name()
            ))),
        }
    }
}

fn uuid_version(id: &str) -> Option<i64> {
    let simple = id.strip_prefix("urn:uuid:").unwrap_or(id).replace('-', "");
    if simple.len() != 32 {
        return None;
    }
    simple
        .as_bytes()
        .get(12)
        .and_then(|b| char::from(*b).to_digit(16))
        .map(i64::from)
}
