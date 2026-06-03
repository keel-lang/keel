use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{ns, positional};

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "Math",
        name: "PI",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "The mathematical constant π (3.14159…).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "E",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "The mathematical constant e (2.71828…).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "sqrt",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the square root of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "pow",
        params: &[
            BuiltinParam {
                name: "x",
                ty: TySpec::Float,
                optional: false,
            },
            BuiltinParam {
                name: "y",
                ty: TySpec::Float,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return x raised to the power y.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "exp",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return e raised to the power x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "log",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the natural logarithm of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "log2",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the base-2 logarithm of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "log10",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the base-10 logarithm of x.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "sin",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the sine of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "cos",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the cosine of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "tan",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the tangent of x (x in radians).",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "asin",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arcsine of x in radians.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "acos",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arccosine of x in radians.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "atan",
        params: &[BuiltinParam {
            name: "x",
            ty: TySpec::Float,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the arctangent of x in radians.",
    },
    BuiltinMethod {
        namespace: "Math",
        name: "atan2",
        params: &[
            BuiltinParam {
                name: "y",
                ty: TySpec::Float,
                optional: false,
            },
            BuiltinParam {
                name: "x",
                ty: TySpec::Float,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return the four-quadrant arctangent of y and x in radians.",
    },
];

fn as_num(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

fn num_arg(
    args: &[crate::interpreter::CallArgValue],
    idx: usize,
    fn_name: &str,
) -> miette::Result<f64> {
    match positional(args, idx) {
        None => Err(miette::miette!(
            "Math.{fn_name}: expected a numeric argument at position {idx}, got nothing"
        )),
        Some(v) => as_num(v).ok_or_else(|| {
            miette::miette!(
                "Math.{fn_name}: expected int or float at position {idx}, got {}",
                v.type_name()
            )
        }),
    }
}

pub(crate) fn namespace() -> Namespace {
    ns!("Math", {
        "PI" => |_interp, _args| Box::pin(async move {
            Ok(Value::Float(std::f64::consts::PI))
        }),
        "E" => |_interp, _args| Box::pin(async move {
            Ok(Value::Float(std::f64::consts::E))
        }),
        "sqrt" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "sqrt")?;
            if x < 0.0 {
                return Err(miette::miette!("Math.sqrt: argument must be non-negative, got {x}"));
            }
            Ok(Value::Float(x.sqrt()))
        }),
        "pow" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "pow")?;
            let y = num_arg(&args, 1, "pow")?;
            Ok(Value::Float(x.powf(y)))
        }),
        "exp" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "exp")?;
            Ok(Value::Float(x.exp()))
        }),
        "log" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "log")?;
            if x <= 0.0 {
                return Err(miette::miette!("Math.log: argument must be positive, got {x}"));
            }
            Ok(Value::Float(x.ln()))
        }),
        "log2" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "log2")?;
            if x <= 0.0 {
                return Err(miette::miette!("Math.log2: argument must be positive, got {x}"));
            }
            Ok(Value::Float(x.log2()))
        }),
        "log10" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "log10")?;
            if x <= 0.0 {
                return Err(miette::miette!("Math.log10: argument must be positive, got {x}"));
            }
            Ok(Value::Float(x.log10()))
        }),
        "sin" => |_interp, args| Box::pin(async move {
            Ok(Value::Float(num_arg(&args, 0, "sin")?.sin()))
        }),
        "cos" => |_interp, args| Box::pin(async move {
            Ok(Value::Float(num_arg(&args, 0, "cos")?.cos()))
        }),
        "tan" => |_interp, args| Box::pin(async move {
            Ok(Value::Float(num_arg(&args, 0, "tan")?.tan()))
        }),
        "asin" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "asin")?;
            if !(-1.0..=1.0).contains(&x) {
                return Err(miette::miette!("Math.asin: argument must be in [-1, 1], got {x}"));
            }
            Ok(Value::Float(x.asin()))
        }),
        "acos" => |_interp, args| Box::pin(async move {
            let x = num_arg(&args, 0, "acos")?;
            if !(-1.0..=1.0).contains(&x) {
                return Err(miette::miette!("Math.acos: argument must be in [-1, 1], got {x}"));
            }
            Ok(Value::Float(x.acos()))
        }),
        "atan" => |_interp, args| Box::pin(async move {
            Ok(Value::Float(num_arg(&args, 0, "atan")?.atan()))
        }),
        "atan2" => |_interp, args| Box::pin(async move {
            let y = num_arg(&args, 0, "atan2")?;
            let x = num_arg(&args, 1, "atan2")?;
            Ok(Value::Float(y.atan2(x)))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{CallArgValue, Interpreter};

    fn pos(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    fn float(f: f64) -> CallArgValue {
        pos(Value::Float(f))
    }
    fn int(i: i64) -> CallArgValue {
        pos(Value::Integer(i))
    }

    fn unpack(v: Value) -> f64 {
        match v {
            Value::Float(f) => f,
            other => panic!("expected float, got {other:?}"),
        }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-10
    }

    #[test]
    fn namespace_has_all_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "Math");
        for name in [
            "PI", "E", "sqrt", "pow", "exp", "log", "log2", "log10", "sin", "cos", "tan", "asin",
            "acos", "atan", "atan2",
        ] {
            assert!(ns.methods.contains_key(name), "missing: {name}");
        }
    }

    #[tokio::test]
    async fn pi_and_e_are_correct() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let pi = unpack(ns.methods["PI"](&mut i, vec![]).await.unwrap());
        let e = unpack(ns.methods["E"](&mut i, vec![]).await.unwrap());
        assert_eq!(pi, std::f64::consts::PI);
        assert_eq!(e, std::f64::consts::E);
    }

    #[tokio::test]
    async fn sqrt_of_four_is_two() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(ns.methods["sqrt"](&mut i, vec![int(4)]).await.unwrap());
        assert!(approx(v, 2.0), "got {v}");
    }

    #[tokio::test]
    async fn sqrt_accepts_float_input() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(ns.methods["sqrt"](&mut i, vec![float(9.0)]).await.unwrap());
        assert!(approx(v, 3.0));
    }

    #[tokio::test]
    async fn sqrt_rejects_negative() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let r = ns.methods["sqrt"](&mut i, vec![float(-1.0)]).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn pow_two_to_ten() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(
            ns.methods["pow"](&mut i, vec![int(2), int(10)])
                .await
                .unwrap(),
        );
        assert!(approx(v, 1024.0));
    }

    #[tokio::test]
    async fn log_of_e_is_one() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(
            ns.methods["log"](&mut i, vec![float(std::f64::consts::E)])
                .await
                .unwrap(),
        );
        assert!(approx(v, 1.0));
    }

    #[tokio::test]
    async fn log_rejects_non_positive() {
        let ns = namespace();
        let mut i = Interpreter::new();
        assert!(ns.methods["log"](&mut i, vec![float(0.0)]).await.is_err());
        assert!(ns.methods["log"](&mut i, vec![float(-1.0)]).await.is_err());
    }

    #[tokio::test]
    async fn log2_of_eight_is_three() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(ns.methods["log2"](&mut i, vec![int(8)]).await.unwrap());
        assert!(approx(v, 3.0));
    }

    #[tokio::test]
    async fn log10_of_hundred_is_two() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(ns.methods["log10"](&mut i, vec![int(100)]).await.unwrap());
        assert!(approx(v, 2.0));
    }

    #[tokio::test]
    async fn sin_cos_at_zero() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let sin = unpack(ns.methods["sin"](&mut i, vec![float(0.0)]).await.unwrap());
        let cos = unpack(ns.methods["cos"](&mut i, vec![float(0.0)]).await.unwrap());
        assert!(approx(sin, 0.0));
        assert!(approx(cos, 1.0));
    }

    #[tokio::test]
    async fn tan_at_pi_over_four() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(
            ns.methods["tan"](&mut i, vec![float(std::f64::consts::PI / 4.0)])
                .await
                .unwrap(),
        );
        assert!(approx(v, 1.0));
    }

    #[tokio::test]
    async fn asin_acos_at_one() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let asin = unpack(ns.methods["asin"](&mut i, vec![float(1.0)]).await.unwrap());
        let acos = unpack(ns.methods["acos"](&mut i, vec![float(1.0)]).await.unwrap());
        assert!(approx(asin, std::f64::consts::PI / 2.0));
        assert!(approx(acos, 0.0));
    }

    #[tokio::test]
    async fn asin_rejects_out_of_range() {
        let ns = namespace();
        let mut i = Interpreter::new();
        assert!(ns.methods["asin"](&mut i, vec![float(1.5)]).await.is_err());
        assert!(ns.methods["acos"](&mut i, vec![float(-2.0)]).await.is_err());
    }

    #[tokio::test]
    async fn atan_at_one_is_pi_over_four() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(ns.methods["atan"](&mut i, vec![float(1.0)]).await.unwrap());
        assert!(approx(v, std::f64::consts::PI / 4.0));
    }

    #[tokio::test]
    async fn atan2_y1_x1_is_pi_over_four() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(
            ns.methods["atan2"](&mut i, vec![float(1.0), float(1.0)])
                .await
                .unwrap(),
        );
        assert!(approx(v, std::f64::consts::PI / 4.0));
    }

    #[tokio::test]
    async fn exp_of_zero_is_one() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(ns.methods["exp"](&mut i, vec![float(0.0)]).await.unwrap());
        assert!(approx(v, 1.0));
    }

    #[tokio::test]
    async fn exp_overflow_returns_infinity() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let v = unpack(ns.methods["exp"](&mut i, vec![float(710.0)]).await.unwrap());
        assert!(v.is_infinite() && v > 0.0, "expected +inf, got {v}");
    }

    #[tokio::test]
    async fn num_arg_wrong_type_gives_descriptive_error() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let err = ns.methods["sqrt"](&mut i, vec![pos(Value::String("oops".into()))])
            .await
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("str"), "expected type name in error: {msg}");
    }

    // ── MockHost tests (AC#3: Math testable without Interpreter) ──────────

    #[tokio::test]
    async fn sqrt_with_mock_host() {
        use crate::interpreter::MockHost;
        use crate::runtime::context::{MapEnv, NativeClock, NativeFileSystem, RuntimeContext};
        use std::sync::Arc;

        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(&[])),
            Arc::new(NativeClock),
            Arc::new(NativeFileSystem),
        );
        let mut host = MockHost::new(ctx);
        let ns = namespace();
        let result = ns.methods["sqrt"](&mut host, vec![float(4.0)])
            .await
            .unwrap();
        assert_eq!(result, Value::Float(2.0));
    }

    #[tokio::test]
    async fn pow_with_mock_host() {
        use crate::interpreter::MockHost;
        use crate::runtime::context::{MapEnv, NativeClock, NativeFileSystem, RuntimeContext};
        use std::sync::Arc;

        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(&[])),
            Arc::new(NativeClock),
            Arc::new(NativeFileSystem),
        );
        let mut host = MockHost::new(ctx);
        let ns = namespace();
        let result = ns.methods["pow"](&mut host, vec![float(2.0), float(8.0)])
            .await
            .unwrap();
        assert_eq!(result, Value::Float(256.0));
    }
}
