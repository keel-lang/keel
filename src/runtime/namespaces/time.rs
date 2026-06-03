use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::args::{expect_str, expect_str_named};
use crate::runtime::namespace::ns;

pub(crate) const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "Time",
        name: "now",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Datetime),
        doc: "Return the current UTC datetime.",
    },
    BuiltinMethod {
        namespace: "Time",
        name: "epoch_ms",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Int),
        doc: "Return the current Unix timestamp in milliseconds.",
    },
    BuiltinMethod {
        namespace: "Time",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableDatetime),
        doc: "Parse a datetime string, returning none on failure.",
    },
];

pub(crate) fn namespace() -> Namespace {
    ns!("Time", {
        "now" => |host, args| Box::pin(async move {
            use chrono::SecondsFormat;
            if let Some(tz_str) = expect_str_named(&args, "tz", "Time.now")? {
                let tz: chrono_tz::Tz = tz_str.parse().map_err(|_| {
                    miette::miette!("Time.now: unknown timezone {tz_str:?}. Use an IANA name like \"America/New_York\".")
                })?;
                let now = host.runtime().clock.now_utc().with_timezone(&tz);
                Ok(Value::String(now.to_rfc3339_opts(SecondsFormat::Millis, false)))
            } else {
                Ok(Value::String(host.runtime().clock.now_utc().to_rfc3339_opts(SecondsFormat::Millis, true)))
            }
        }),
        "epoch_ms" => |host, _args| Box::pin(async move {
            let ms = host.runtime().clock.now_utc().timestamp_millis();
            Ok(Value::Integer(ms))
        }),
        "parse" => |_i, args| Box::pin(async move {
            use chrono::SecondsFormat;
            let s = expect_str(&args, 0, "Time.parse")?;

            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
                return Ok(Value::String(dt.to_rfc3339_opts(SecondsFormat::Millis, false)));
            }

            if let Some(tz_str) = expect_str_named(&args, "tz", "Time.parse")? {
                let Ok(tz) = tz_str.parse::<chrono_tz::Tz>() else {
                    return Ok(Value::None);
                };
                for fmt in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S",
                            "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d"] {
                    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(s, fmt)
                        && let chrono::LocalResult::Single(dt) = ndt.and_local_timezone(tz)
                    {
                        return Ok(Value::String(dt.to_rfc3339_opts(SecondsFormat::Millis, false)));
                    }
                    if let Ok(nd) = chrono::NaiveDate::parse_from_str(s, fmt)
                        && let Some(ndt) = nd.and_hms_opt(0, 0, 0)
                        && let chrono::LocalResult::Single(dt) = ndt.and_local_timezone(tz)
                    {
                        return Ok(Value::String(dt.to_rfc3339_opts(SecondsFormat::Millis, false)));
                    }
                }
            }

            Ok(Value::None)
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    use chrono::{DateTime, Utc};

    use crate::interpreter::{CallArgValue, Interpreter};
    use crate::runtime::context::{Clock, MapEnv, NativeFileSystem, RuntimeContext};

    struct FrozenClock {
        utc: DateTime<Utc>,
        instant: Instant,
    }

    impl FrozenClock {
        fn frozen_at(utc: DateTime<Utc>) -> Self {
            Self {
                utc,
                instant: Instant::now(),
            }
        }
    }

    impl Clock for FrozenClock {
        fn now_utc(&self) -> DateTime<Utc> {
            self.utc
        }

        fn now_instant(&self) -> Instant {
            self.instant
        }
    }

    fn interp_at(iso: &str) -> Interpreter {
        let dt: DateTime<Utc> = iso.parse().expect("valid RFC 3339");
        let ctx = RuntimeContext::test_context(
            Arc::new(MapEnv::with(&[])),
            Arc::new(FrozenClock::frozen_at(dt)),
            Arc::new(NativeFileSystem),
        );
        Interpreter::with_runtime(ctx)
    }

    fn arg(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    #[test]
    fn namespace_has_now_parse_and_epoch_ms() {
        let ns = namespace();
        assert_eq!(ns.name, "Time");
        assert!(ns.methods.contains_key("now"));
        assert!(ns.methods.contains_key("parse"));
        assert!(ns.methods.contains_key("epoch_ms"));
    }

    #[tokio::test]
    async fn epoch_ms_returns_unix_millis() {
        let ns = namespace();
        let mut interp = interp_at("2024-01-15T10:30:00.500Z");
        let method = ns.methods.get("epoch_ms").unwrap();
        let result = method(&mut interp, vec![]).await;
        match result.unwrap() {
            Value::Integer(ms) => {
                // 2024-01-15T10:30:00.500Z = 1705314600500 ms since epoch
                assert_eq!(ms, 1705314600500);
            }
            other => panic!("expected int, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn now_returns_frozen_time() {
        let ns = namespace();
        let mut interp = interp_at("2024-01-15T10:30:00Z");
        let method = ns.methods.get("now").unwrap();
        let result = method(&mut interp, vec![]).await;
        let s = match result.unwrap() {
            Value::String(s) => s,
            other => panic!("expected string, got {other:?}"),
        };
        assert!(s.starts_with("2024-01-15T10:30:00"), "got {s}");
    }

    #[tokio::test]
    async fn parse_rfc3339_returns_datetime() {
        let ns = namespace();
        let mut interp = interp_at("2024-01-01T00:00:00Z");
        let method = ns.methods.get("parse").unwrap();
        let result = method(
            &mut interp,
            vec![arg(Value::String("2024-06-15T12:00:00+02:00".into()))],
        )
        .await;
        let s = match result.unwrap() {
            Value::String(s) => s,
            other => panic!("expected string, got {other:?}"),
        };
        assert!(s.contains("2024-06-15"), "got {s}");
    }

    #[tokio::test]
    async fn parse_returns_none_for_invalid() {
        let ns = namespace();
        let mut interp = interp_at("2024-01-01T00:00:00Z");
        let method = ns.methods.get("parse").unwrap();
        let result = method(&mut interp, vec![arg(Value::String("not-a-date".into()))]).await;
        assert_eq!(result.unwrap(), Value::None);
    }
}
