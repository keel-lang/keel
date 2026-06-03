use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{find_arg, ns};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "Random",
        name: "float",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return a random float in the range [0, 1).",
    },
    BuiltinMethod {
        namespace: "Random",
        name: "int",
        params: &[
            BuiltinParam {
                name: "min",
                ty: TySpec::Int,
                optional: false,
            },
            BuiltinParam {
                name: "max",
                ty: TySpec::Int,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Int),
        doc: "Return a random integer in the inclusive range [min, max].",
    },
    BuiltinMethod {
        namespace: "Random",
        name: "bool",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Return a random boolean.",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("Random", {
        "float" => |_interp, _args| Box::pin(async move {
            Ok(Value::Float(fastrand::f64()))
        }),
        "int" => |_interp, args| Box::pin(async move {
            let min = find_arg(&args, "min")
                .and_then(Value::as_int)
                .ok_or_else(|| miette::miette!("Random.int: missing integer `min:` argument"))?;
            let max = find_arg(&args, "max")
                .and_then(Value::as_int)
                .ok_or_else(|| miette::miette!("Random.int: missing integer `max:` argument"))?;
            if min > max {
                return Err(miette::miette!(
                    "Random.int: `min:` must be <= `max:`, got {min} > {max}"
                ));
            }
            Ok(Value::Integer(fastrand::i64(min..=max)))
        }),
        "bool" => |_interp, _args| Box::pin(async move {
            Ok(Value::Bool(fastrand::bool()))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::interpreter::{CallArgValue, Interpreter};

    fn named(name: &str, value: Value) -> CallArgValue {
        CallArgValue {
            name: Some(name.to_string()),
            value,
        }
    }

    #[test]
    fn namespace_has_random_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "Random");
        assert!(ns.methods.contains_key("float"));
        assert!(ns.methods.contains_key("int"));
        assert!(ns.methods.contains_key("bool"));
    }

    #[tokio::test]
    async fn float_is_in_half_open_unit_range() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("float").expect("float method");
        let value = method(&mut interp, vec![]).await.expect("random float");
        let Value::Float(n) = value else {
            panic!("expected float");
        };
        assert!((0.0..1.0).contains(&n), "random float out of range: {n}");
    }

    #[tokio::test]
    async fn int_respects_inclusive_bounds() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("int").expect("int method");
        for _ in 0..32 {
            let value = method(
                &mut interp,
                vec![
                    named("min", Value::Integer(-2)),
                    named("max", Value::Integer(3)),
                ],
            )
            .await
            .expect("random int");
            let Value::Integer(n) = value else {
                panic!("expected int");
            };
            assert!((-2..=3).contains(&n), "random int out of range: {n}");
        }
    }

    #[tokio::test]
    async fn int_rejects_inverted_bounds() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("int").expect("int method");
        let result = method(
            &mut interp,
            vec![
                named("min", Value::Integer(5)),
                named("max", Value::Integer(1)),
            ],
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn bool_returns_bool() {
        let ns = namespace();
        let mut interp = Interpreter::new();
        let method = ns.methods.get("bool").expect("bool method");
        let value = method(&mut interp, vec![]).await.expect("random bool");
        assert!(matches!(value, Value::Bool(_)));
    }
}
