use miette::Result;

use crate::ast::{Binding, DestructPat};

use super::environment::Environment;
use super::runtime_error;
use super::value::Value;

fn bind_destructure(pat: &DestructPat, value: Value, env: &mut Environment) -> Result<()> {
    match pat {
        DestructPat::Struct(fields) => {
            if !matches!(value, Value::Map(_) | Value::Struct(_, _)) {
                return Err(runtime_error(format!(
                    "cannot destructure {} as a struct",
                    value.type_name()
                )));
            }
            for (source, local) in fields {
                let v = value.get_str_field(source).cloned().unwrap_or(Value::None);
                env.define(local.clone(), v);
            }
        }
        DestructPat::Tuple(names) => {
            let items = match value {
                Value::List(items) => items,
                other => {
                    return Err(runtime_error(format!(
                        "cannot destructure {} as a tuple",
                        other.type_name()
                    )));
                }
            };
            for (i, name) in names.iter().enumerate() {
                let v = items.get(i).cloned().unwrap_or(Value::None);
                env.define(name.clone(), v);
            }
        }
    }
    Ok(())
}

pub(crate) fn bind_value(binding: &Binding, value: Value, env: &mut Environment) -> Result<()> {
    match binding {
        Binding::Ident(name) => {
            env.define(name.clone(), value);
            Ok(())
        }
        Binding::Destruct(pat) => bind_destructure(pat, value, env),
    }
}
