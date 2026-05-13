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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinOp;

    // ── Helpers ──────────────────────────────────────────────────────────

    fn int(n: i64) -> Value {
        Value::Integer(n)
    }
    fn float(n: f64) -> Value {
        Value::Float(n)
    }
    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }
    fn boolv(b: bool) -> Value {
        Value::Bool(b)
    }
    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }
    fn range(lo: i64, hi: i64) -> Value {
        Value::Range(lo, hi)
    }
    fn dur(secs: f64) -> Value {
        Value::Duration(secs)
    }

    macro_rules! eval {
        ($op:ident, $l:expr, $r:expr) => {
            eval_binary(BinOp::$op, $l, $r)
        };
    }

    // ── Integer arithmetic ───────────────────────────────────────────────

    #[test]
    fn int_add() {
        assert_eq!(eval!(Add, int(2), int(3)).unwrap(), int(5));
    }
    #[test]
    fn int_sub() {
        assert_eq!(eval!(Sub, int(10), int(3)).unwrap(), int(7));
    }
    #[test]
    fn int_mul() {
        assert_eq!(eval!(Mul, int(4), int(5)).unwrap(), int(20));
    }
    #[test]
    fn int_div() {
        assert_eq!(eval!(Div, int(10), int(2)).unwrap(), int(5));
    }
    #[test]
    fn int_div_by_zero() {
        let e = eval!(Div, int(1), int(0)).unwrap_err();
        assert!(e.to_string().contains("Division by zero"));
    }
    #[test]
    fn int_mod() {
        assert_eq!(eval!(Mod, int(10), int(3)).unwrap(), int(1));
    }
    #[test]
    fn int_mod_by_zero() {
        let e = eval!(Mod, int(5), int(0)).unwrap_err();
        assert!(e.to_string().contains("Modulo by zero"));
    }

    // ── Float arithmetic ─────────────────────────────────────────────────

    #[test]
    fn float_add() {
        assert_eq!(eval!(Add, float(1.5), float(2.5)).unwrap(), float(4.0));
    }
    #[test]
    fn float_sub() {
        assert_eq!(eval!(Sub, float(5.0), float(1.5)).unwrap(), float(3.5));
    }
    #[test]
    fn float_mul() {
        assert_eq!(eval!(Mul, float(2.0), float(3.0)).unwrap(), float(6.0));
    }
    #[test]
    fn float_div() {
        assert_eq!(eval!(Div, float(7.0), float(2.0)).unwrap(), float(3.5));
    }

    // ── Mixed float-int arithmetic (float op int) ────────────────────────

    #[test]
    fn float_int_add() {
        assert_eq!(eval!(Add, float(1.5), int(2)).unwrap(), float(3.5));
    }
    #[test]
    fn float_int_sub() {
        assert_eq!(eval!(Sub, float(5.0), int(2)).unwrap(), float(3.0));
    }
    #[test]
    fn float_int_mul() {
        assert_eq!(eval!(Mul, float(2.5), int(4)).unwrap(), float(10.0));
    }
    #[test]
    fn float_int_div() {
        assert_eq!(eval!(Div, float(10.0), int(4)).unwrap(), float(2.5));
    }

    // ── Mixed int-float arithmetic (int op float) ────────────────────────

    #[test]
    fn int_float_add() {
        assert_eq!(eval!(Add, int(3), float(1.5)).unwrap(), float(4.5));
    }
    #[test]
    fn int_float_sub() {
        assert_eq!(eval!(Sub, int(10), float(2.5)).unwrap(), float(7.5));
    }
    #[test]
    fn int_float_mul() {
        assert_eq!(eval!(Mul, int(4), float(2.5)).unwrap(), float(10.0));
    }
    #[test]
    fn int_float_div() {
        assert_eq!(eval!(Div, int(7), float(2.0)).unwrap(), float(3.5));
    }

    // ── String concatenation ─────────────────────────────────────────────

    #[test]
    fn str_add() {
        assert_eq!(
            eval!(Add, s("hello "), s("world")).unwrap(),
            s("hello world")
        );
    }

    // ── List / Range concatenation ───────────────────────────────────────

    #[test]
    fn list_add_list() {
        assert_eq!(
            eval!(Add, list(vec![int(1)]), list(vec![int(2)])).unwrap(),
            list(vec![int(1), int(2)])
        );
    }

    #[test]
    fn range_add_list() {
        assert_eq!(
            eval!(Add, range(1, 3), list(vec![int(10)])).unwrap(),
            list(vec![int(1), int(2), int(3), int(10)])
        );
    }

    #[test]
    fn list_add_range() {
        assert_eq!(
            eval!(Add, list(vec![int(10)]), range(1, 3)).unwrap(),
            list(vec![int(10), int(1), int(2), int(3)])
        );
    }

    #[test]
    fn range_add_range() {
        assert_eq!(
            eval!(Add, range(1, 2), range(3, 4)).unwrap(),
            list(vec![int(1), int(2), int(3), int(4)])
        );
    }

    // ── Equality / inequality (catch-all arms) ───────────────────────────

    #[test]
    fn eq_true() {
        assert_eq!(eval!(Eq, int(1), int(1)).unwrap(), boolv(true));
    }
    #[test]
    fn eq_false() {
        assert_eq!(eval!(Eq, int(1), int(2)).unwrap(), boolv(false));
    }
    #[test]
    fn neq_true() {
        assert_eq!(eval!(Neq, int(1), int(2)).unwrap(), boolv(true));
    }
    #[test]
    fn neq_false() {
        assert_eq!(eval!(Neq, int(1), int(1)).unwrap(), boolv(false));
    }

    // ── Integer comparisons ──────────────────────────────────────────────

    #[test]
    fn int_lt() {
        assert_eq!(eval!(Lt, int(1), int(2)).unwrap(), boolv(true));
    }
    #[test]
    fn int_lt_false() {
        assert_eq!(eval!(Lt, int(2), int(1)).unwrap(), boolv(false));
    }
    #[test]
    fn int_gt() {
        assert_eq!(eval!(Gt, int(5), int(3)).unwrap(), boolv(true));
    }
    #[test]
    fn int_gt_false() {
        assert_eq!(eval!(Gt, int(3), int(5)).unwrap(), boolv(false));
    }
    #[test]
    fn int_lte() {
        assert_eq!(eval!(Lte, int(3), int(3)).unwrap(), boolv(true));
    }
    #[test]
    fn int_lte_false() {
        assert_eq!(eval!(Lte, int(5), int(3)).unwrap(), boolv(false));
    }
    #[test]
    fn int_gte() {
        assert_eq!(eval!(Gte, int(5), int(5)).unwrap(), boolv(true));
    }
    #[test]
    fn int_gte_false() {
        assert_eq!(eval!(Gte, int(3), int(5)).unwrap(), boolv(false));
    }

    // ── Float comparisons ────────────────────────────────────────────────

    #[test]
    fn float_lt() {
        assert_eq!(eval!(Lt, float(1.0), float(2.0)).unwrap(), boolv(true));
    }
    #[test]
    fn float_gt() {
        assert_eq!(eval!(Gt, float(5.0), float(3.0)).unwrap(), boolv(true));
    }
    #[test]
    fn float_lte() {
        assert_eq!(eval!(Lte, float(3.0), float(3.0)).unwrap(), boolv(true));
    }
    #[test]
    fn float_gte() {
        assert_eq!(eval!(Gte, float(4.0), float(4.0)).unwrap(), boolv(true));
    }
    #[test]
    fn float_lt_false() {
        assert_eq!(eval!(Lt, float(3.0), float(1.0)).unwrap(), boolv(false));
    }

    // ── Mixed float-int comparisons ──────────────────────────────────────

    #[test]
    fn float_int_lt() {
        assert_eq!(eval!(Lt, float(1.5), int(3)).unwrap(), boolv(true));
    }
    #[test]
    fn float_int_gt() {
        assert_eq!(eval!(Gt, float(5.5), int(3)).unwrap(), boolv(true));
    }
    #[test]
    fn float_int_lte() {
        assert_eq!(eval!(Lte, float(3.0), int(3)).unwrap(), boolv(true));
    }
    #[test]
    fn float_int_gte() {
        assert_eq!(eval!(Gte, float(4.0), int(4)).unwrap(), boolv(true));
    }

    // ── Mixed int-float comparisons ──────────────────────────────────────

    #[test]
    fn int_float_lt() {
        assert_eq!(eval!(Lt, int(1), float(3.5)).unwrap(), boolv(true));
    }
    #[test]
    fn int_float_gt() {
        assert_eq!(eval!(Gt, int(5), float(1.5)).unwrap(), boolv(true));
    }
    #[test]
    fn int_float_lte() {
        assert_eq!(eval!(Lte, int(3), float(3.0)).unwrap(), boolv(true));
    }
    #[test]
    fn int_float_gte() {
        assert_eq!(eval!(Gte, int(4), float(3.9)).unwrap(), boolv(true));
    }

    // ── Boolean logic ────────────────────────────────────────────────────

    #[test]
    fn and_both_true() {
        assert_eq!(eval!(And, boolv(true), boolv(true)).unwrap(), boolv(true));
    }
    #[test]
    fn and_one_false() {
        assert_eq!(eval!(And, boolv(true), boolv(false)).unwrap(), boolv(false));
    }
    #[test]
    fn and_truthy() {
        assert_eq!(eval!(And, int(1), int(2)).unwrap(), boolv(true));
    }
    #[test]
    fn and_falsy_zero() {
        assert_eq!(eval!(And, int(0), int(1)).unwrap(), boolv(false));
    }
    #[test]
    fn or_both_false() {
        assert_eq!(eval!(Or, boolv(false), boolv(false)).unwrap(), boolv(false));
    }
    #[test]
    fn or_one_true() {
        assert_eq!(eval!(Or, boolv(false), boolv(true)).unwrap(), boolv(true));
    }
    #[test]
    fn or_falsy() {
        assert_eq!(eval!(Or, int(0), s("")).unwrap(), boolv(false));
    }

    // ── Datetime + Duration → Datetime ───────────────────────────────────

    #[test]
    fn datetime_add_duration() {
        let result = eval!(Add, s("2024-01-15T10:30:00Z"), dur(3600.0)).unwrap();
        assert!(result.to_string().contains("2024-01-15T11:30:00"));
    }

    #[test]
    fn datetime_add_duration_non_datetime() {
        let e = eval!(Add, s("not a date"), dur(60.0)).unwrap_err();
        assert!(e.to_string().contains("non-datetime string"));
    }

    // ── Datetime - Duration → Datetime ───────────────────────────────────

    #[test]
    fn datetime_sub_duration() {
        let result = eval!(Sub, s("2024-01-15T10:30:00Z"), dur(3600.0)).unwrap();
        assert!(result.to_string().contains("2024-01-15T09:30:00"));
    }

    #[test]
    fn datetime_sub_duration_non_datetime() {
        let e = eval!(Sub, s("garbage"), dur(60.0)).unwrap_err();
        assert!(e.to_string().contains("non-datetime string"));
    }

    // ── Datetime - Datetime → Duration ───────────────────────────────────

    #[test]
    fn datetime_sub_datetime() {
        assert_eq!(
            eval!(Sub, s("2024-01-15T11:00:00Z"), s("2024-01-15T10:00:00Z")).unwrap(),
            dur(3600.0)
        );
    }

    #[test]
    fn datetime_sub_datetime_non_datetime_left() {
        let e = eval!(Sub, s("bad"), s("2024-01-15T10:00:00Z")).unwrap_err();
        assert!(e.to_string().contains("not a datetime string"));
    }

    #[test]
    fn datetime_sub_datetime_non_datetime_right() {
        let e = eval!(Sub, s("2024-01-15T10:00:00Z"), s("bad")).unwrap_err();
        assert!(e.to_string().contains("not a datetime string"));
    }

    // ── Datetime comparisons ─────────────────────────────────────────────

    #[test]
    fn datetime_lt() {
        assert_eq!(
            eval!(Lt, s("2024-01-15T10:00:00Z"), s("2024-01-15T11:00:00Z")).unwrap(),
            boolv(true)
        );
    }

    #[test]
    fn datetime_gt() {
        assert_eq!(
            eval!(Gt, s("2024-01-15T12:00:00Z"), s("2024-01-15T10:00:00Z")).unwrap(),
            boolv(true)
        );
    }

    #[test]
    fn datetime_lte_equal() {
        assert_eq!(
            eval!(Lte, s("2024-01-15T10:00:00Z"), s("2024-01-15T10:00:00Z")).unwrap(),
            boolv(true)
        );
    }

    #[test]
    fn datetime_gte() {
        assert_eq!(
            eval!(Gte, s("2024-01-15T15:00:00Z"), s("2024-01-15T10:00:00Z")).unwrap(),
            boolv(true)
        );
    }

    #[test]
    fn datetime_lt_non_datetime() {
        let e = eval!(Lt, s("hello"), s("world")).unwrap_err();
        assert!(e.to_string().contains("cannot compare strings"));
    }

    #[test]
    fn datetime_gt_non_datetime() {
        let e = eval!(Gt, s("a"), s("b")).unwrap_err();
        assert!(e.to_string().contains("cannot compare strings"));
    }

    #[test]
    fn datetime_lte_non_datetime() {
        let e = eval!(Lte, s("x"), s("y")).unwrap_err();
        assert!(e.to_string().contains("cannot compare strings"));
    }

    #[test]
    fn datetime_gte_non_datetime() {
        let e = eval!(Gte, s("p"), s("q")).unwrap_err();
        assert!(e.to_string().contains("cannot compare strings"));
    }

    // ── Fallback error (type mismatch) ───────────────────────────────────

    #[test]
    fn type_mismatch_add_bool_int() {
        let e = eval!(Add, boolv(true), int(1)).unwrap_err();
        assert!(e.to_string().contains("Cannot apply"));
    }

    #[test]
    fn type_mismatch_sub_bool() {
        let e = eval!(Sub, boolv(true), boolv(false)).unwrap_err();
        assert!(e.to_string().contains("Cannot apply"));
    }

    #[test]
    fn type_mismatch_mul_bool() {
        let e = eval!(Mul, boolv(true), boolv(false)).unwrap_err();
        assert!(e.to_string().contains("Cannot apply"));
    }

    // ── is_pascal_case ───────────────────────────────────────────────────

    #[test]
    fn pascal_case_valid() {
        assert!(is_pascal_case("HelloWorld"));
        assert!(is_pascal_case("A"));
        assert!(is_pascal_case("XMLParser"));
    }

    #[test]
    fn pascal_case_non_ascii_uppercase_is_false() {
        // is_pascal_case uses is_ascii_uppercase — non-ASCII like É returns false
        assert!(!is_pascal_case("Élève"));
    }

    #[test]
    fn pascal_case_invalid() {
        assert!(!is_pascal_case("hello"));
        assert!(!is_pascal_case("helloWorld"));
        assert!(!is_pascal_case(""));
        assert!(!is_pascal_case("_Hello"));
    }

    // ── parse_dt ─────────────────────────────────────────────────────────

    #[test]
    fn parse_dt_rfc3339() {
        assert!(parse_dt("2024-01-15T10:30:00Z").is_some());
        assert!(parse_dt("2024-01-15T10:30:00+02:00").is_some());
    }

    #[test]
    fn parse_dt_formats() {
        assert!(parse_dt("2024-01-15T10:30:00").is_some());
        assert!(parse_dt("2024-01-15 10:30:00").is_some());
        assert!(parse_dt("2024-01-15T10:30").is_some());
        assert!(parse_dt("2024-01-15").is_some());
    }

    #[test]
    fn parse_dt_invalid() {
        assert!(parse_dt("not a date").is_none());
        assert!(parse_dt("").is_none());
        assert!(parse_dt("2024-13-01").is_none());
    }
}
