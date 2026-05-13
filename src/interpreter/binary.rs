use miette::Result;

use crate::ast::BinOp;

use super::runtime_error;
use super::value::Value;

pub(crate) fn eval_binary(op: BinOp, l: Value, r: Value) -> Result<Value> {
    use BinOp::*;
    match (op, &l, &r) {
        (Add, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
        (Sub, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
        (Mul, Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
        (Div, Value::Integer(a), Value::Integer(b)) => {
            if *b == 0 {
                return Err(runtime_error("Division by zero"));
            }
            Ok(Value::Integer(a / b))
        }
        (Mod, Value::Integer(a), Value::Integer(b)) => {
            if *b == 0 {
                return Err(runtime_error("Modulo by zero"));
            }
            Ok(Value::Integer(a % b))
        }
        (Add, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
        (Sub, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
        (Mul, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
        (Div, Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
        // float op int (and int op float) — promote int to float
        (Add, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a + *b as f64)),
        (Sub, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a - *b as f64)),
        (Mul, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a * *b as f64)),
        (Div, Value::Float(a), Value::Integer(b)) => Ok(Value::Float(a / *b as f64)),
        (Add, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 + b)),
        (Sub, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 - b)),
        (Mul, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 * b)),
        (Div, Value::Integer(a), Value::Float(b)) => Ok(Value::Float(*a as f64 / b)),
        (Lt, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a < *b as f64)),
        (Gt, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a > *b as f64)),
        (Lte, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a <= *b as f64)),
        (Gte, Value::Float(a), Value::Integer(b)) => Ok(Value::Bool(*a >= *b as f64)),
        (Lt, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) < *b)),
        (Gt, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) > *b)),
        (Lte, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) <= *b)),
        (Gte, Value::Integer(a), Value::Float(b)) => Ok(Value::Bool((*a as f64) >= *b)),
        (Add, Value::String(a), Value::String(b)) => Ok(Value::String(format!("{a}{b}"))),
        (Add, Value::List(a), Value::List(b)) => {
            let mut result = a.clone();
            result.extend(b.clone());
            Ok(Value::List(result))
        }
        // Concatenating with a range materializes it — user explicitly asked for a list.
        (Add, Value::Range(lo, hi), Value::List(b)) => {
            let mut result: Vec<Value> = (*lo..=*hi).map(Value::Integer).collect();
            result.extend(b.clone());
            Ok(Value::List(result))
        }
        (Add, Value::List(a), Value::Range(lo, hi)) => {
            let mut result = a.clone();
            result.extend((*lo..=*hi).map(Value::Integer));
            Ok(Value::List(result))
        }
        (Add, Value::Range(lo1, hi1), Value::Range(lo2, hi2)) => {
            let mut result: Vec<Value> = (*lo1..=*hi1).map(Value::Integer).collect();
            result.extend((*lo2..=*hi2).map(Value::Integer));
            Ok(Value::List(result))
        }
        (Eq, a, b) => Ok(Value::Bool(a == b)),
        (Neq, a, b) => Ok(Value::Bool(a != b)),
        (Lt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a > b)),
        (Lte, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a <= b)),
        (Gte, Value::Integer(a), Value::Integer(b)) => Ok(Value::Bool(a >= b)),
        (Lt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a < b)),
        (Gt, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a > b)),
        (Lte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a <= b)),
        (Gte, Value::Float(a), Value::Float(b)) => Ok(Value::Bool(a >= b)),
        (And, a, b) => Ok(Value::Bool(a.is_truthy() && b.is_truthy())),
        (Or, a, b) => Ok(Value::Bool(a.is_truthy() || b.is_truthy())),
        // datetime (ISO string) ± duration → ISO string  (millisecond precision)
        (Add, Value::String(s), Value::Duration(secs)) => {
            use chrono::SecondsFormat;
            let dt = parse_dt(s).ok_or_else(|| {
                runtime_error(format!("cannot add duration to non-datetime string {s:?}"))
            })?;
            let ms = (*secs * 1000.0) as i64;
            let shifted = dt + chrono::Duration::milliseconds(ms);
            Ok(Value::String(
                shifted.to_rfc3339_opts(SecondsFormat::Millis, true),
            ))
        }
        (Sub, Value::String(s), Value::Duration(secs)) => {
            use chrono::SecondsFormat;
            let dt = parse_dt(s).ok_or_else(|| {
                runtime_error(format!(
                    "cannot subtract duration from non-datetime string {s:?}"
                ))
            })?;
            let ms = (*secs * 1000.0) as i64;
            let shifted = dt - chrono::Duration::milliseconds(ms);
            Ok(Value::String(
                shifted.to_rfc3339_opts(SecondsFormat::Millis, true),
            ))
        }
        // datetime - datetime → duration (seconds)
        (Sub, Value::String(a), Value::String(b)) => {
            let da = parse_dt(a).ok_or_else(|| {
                runtime_error(format!("cannot subtract: {a:?} is not a datetime string"))
            })?;
            let db = parse_dt(b).ok_or_else(|| {
                runtime_error(format!("cannot subtract: {b:?} is not a datetime string"))
            })?;
            let secs = (da - db).num_milliseconds() as f64 / 1000.0;
            Ok(Value::Duration(secs))
        }
        // datetime string comparison
        (Lt, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da < db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `<`"
            ))),
        },
        (Gt, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da > db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `>`"
            ))),
        },
        (Lte, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da <= db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `<=`"
            ))),
        },
        (Gte, Value::String(a), Value::String(b)) => match (parse_dt(a), parse_dt(b)) {
            (Some(da), Some(db)) => Ok(Value::Bool(da >= db)),
            _ => Err(runtime_error(format!(
                "cannot compare strings {a:?} and {b:?} with `>=`"
            ))),
        },
        _ => Err(runtime_error(format!(
            "Cannot apply `{:?}` to {} and {}",
            op,
            l.type_name(),
            r.type_name()
        ))),
    }
}

pub(crate) fn is_pascal_case(s: &str) -> bool {
    s.chars()
        .next()
        .map(|c| c.is_ascii_uppercase())
        .unwrap_or(false)
}

fn parse_dt(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d",
    ] {
        if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
        if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, fmt) {
            let ndt = nd.and_hms_opt(0, 0, 0)?;
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                ndt,
                chrono::Utc,
            ));
        }
    }
    None
}
