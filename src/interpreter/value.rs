use std::collections::HashMap;
use std::fmt;

use crate::ast::{DurationUnit, Expr, LambdaParam, TaskDecl};

/// Runtime value representation for the Keel interpreter.
#[derive(Debug, Clone)]
pub enum Value {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    None,

    /// List of values
    List(Vec<Value>),
    /// Map / struct — keys are always strings
    Map(HashMap<String, Value>),

    /// An enum variant: (type_name, variant_name)
    EnumVariant(String, String),

    /// Duration in seconds
    Duration(f64),

    /// A callable task (name, declaration)
    Task(String, TaskDecl),

    /// Reference to an agent instance
    AgentRef(String),

    /// A closure: (param names, body expression)
    Closure(Vec<LambdaParam>, Box<Expr>),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Integer(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "str",
            Value::Bool(_) => "bool",
            Value::None => "none",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::EnumVariant(_, _) => "enum",
            Value::Duration(_) => "duration",
            Value::Task(_, _) => "task",
            Value::AgentRef(_) => "agent",
            Value::Closure(_, _) => "closure",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::None => false,
            Value::Integer(0) => false,
            Value::String(s) if s.is_empty() => false,
            Value::List(l) if l.is_empty() => false,
            _ => true,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            other => format!("{other}"),
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(n) => Some(*n),
            _ => None,
        }
    }

    pub fn duration_seconds(value: i64, unit: DurationUnit) -> f64 {
        match unit {
            DurationUnit::Seconds => value as f64,
            DurationUnit::Minutes => value as f64 * 60.0,
            DurationUnit::Hours => value as f64 * 3600.0,
            DurationUnit::Days => value as f64 * 86400.0,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::None => write!(f, "none"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Map(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::EnumVariant(ty, variant) => write!(f, "{ty}.{variant}"),
            Value::Duration(secs) => {
                if *secs >= 86400.0 {
                    write!(f, "{} days", secs / 86400.0)
                } else if *secs >= 3600.0 {
                    write!(f, "{} hours", secs / 3600.0)
                } else if *secs >= 60.0 {
                    write!(f, "{} minutes", secs / 60.0)
                } else {
                    write!(f, "{secs} seconds")
                }
            }
            Value::Task(name, _) => write!(f, "<task {name}>"),
            Value::AgentRef(name) => write!(f, "<agent {name}>"),
            Value::Closure(params, _) => {
                let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
                write!(f, "<closure ({})>", names.join(", "))
            }
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::None, Value::None) => true,
            (Value::EnumVariant(t1, v1), Value::EnumVariant(t2, v2)) => t1 == t2 && v1 == v2,
            _ => false,
        }
    }
}
