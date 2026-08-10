use crate::interpreter::value::Value;
use crate::interpreter::{Namespace, RuntimeErrorKind, call_fn_value, find_fn_value, is_fn_value};
use crate::runtime::args::{expect_duration, expect_str};
use crate::runtime::namespace::{make_typed_report, ns, positional};
use crate::runtime::namespaces::schedule::parse_datetime;

pub(crate) fn namespace() -> Namespace {
    ns!("control", {
        // Control.retry(n, fn) — invoke fn up to n times until it succeeds.
        // The last error is surfaced if every attempt fails.
        "retry" => |host, args| Box::pin(async move {
            let attempts = match positional(&args, 0) {
                Some(Value::Integer(n)) if *n > 0 => *n as usize,
                _ => return Err(miette::miette!("control.retry: first argument must be a positive integer")),
            };
            let f = find_fn_value(&args)
                .ok_or_else(|| miette::miette!("control.retry: missing closure argument"))?;

            let mut last_err: Option<miette::Report> = None;
            for _ in 0..attempts {
                match call_fn_value(host, &f, vec![]).await {
                    Ok(v) => return Ok(v),
                    Err(e) => last_err = Some(e),
                }
            }
            Err(last_err.unwrap_or_else(|| miette::miette!("control.retry: all attempts failed")))
        }),
        // Control.with_timeout(duration, fn) — abort fn if it doesn't
        // complete within `duration`. Raises TimeoutError on expiry.
        "with_timeout" => |host, args| Box::pin(async move {
            let pos0_is_fn = positional(&args, 0).map(is_fn_value).unwrap_or(false);
            if positional(&args, 0).is_none() || pos0_is_fn {
                return Err(miette::miette!(
                    "control.with_timeout: missing duration argument"
                ));
            }
            let duration = expect_duration(&args, 0, "control.with_timeout")?;
            let f = find_fn_value(&args)
                .ok_or_else(|| miette::miette!("control.with_timeout: missing closure argument"))?;

            let dur = std::time::Duration::from_secs_f64(duration);
            let fut = call_fn_value(host, &f, vec![]);
            match tokio::time::timeout(dur, fut).await {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(make_typed_report(RuntimeErrorKind::Timeout, format!("control.with_timeout exceeded {duration}s"))),
            }
        }),
        // Control.with_deadline(datetime_str, fn) — abort fn if the
        // absolute deadline (RFC 3339 / ISO 8601) passes before fn returns.
        "with_deadline" => |host, args| Box::pin(async move {
            let when_str = expect_str(&args, 0, "control.with_deadline")?;
            let target = parse_datetime(when_str)
                .ok_or_else(|| miette::miette!("control.with_deadline: cannot parse `{when_str}` as an ISO 8601 datetime"))?;
            let now = host.runtime().clock.now_utc();
            let remaining = (target - now).num_milliseconds().max(0) as u64;
            let f = find_fn_value(&args)
                .ok_or_else(|| miette::miette!("control.with_deadline: missing closure argument"))?;

            let dur = std::time::Duration::from_millis(remaining);
            let fut = call_fn_value(host, &f, vec![]);
            match tokio::time::timeout(dur, fut).await {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(make_typed_report(RuntimeErrorKind::Deadline, format!("control.with_deadline exceeded `{when_str}`"))),
            }
        }),
    })
}
