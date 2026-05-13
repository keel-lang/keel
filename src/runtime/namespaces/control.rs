use crate::interpreter::Namespace;
use crate::interpreter::value::Value;
use crate::runtime::namespace::{ns, positional};
use crate::runtime::namespaces::schedule::parse_datetime;

pub(crate) fn namespace() -> Namespace {
    ns!("Control", {
        // Control.retry(n, fn) — invoke fn up to n times until it succeeds.
        // The last error is surfaced if every attempt fails.
        "retry" => |interp, args| Box::pin(async move {
            let attempts = match positional(&args, 0) {
                Some(Value::Integer(n)) if *n > 0 => *n as usize,
                _ => return Err(miette::miette!("Control.retry: first argument must be a positive integer")),
            };
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.retry: missing closure argument"))?;

            let mut last_err: Option<miette::Report> = None;
            for _ in 0..attempts {
                match interp.call_closure(&params, &body, vec![]).await {
                    Ok(v) => return Ok(v),
                    Err(e) => last_err = Some(e),
                }
            }
            Err(last_err.unwrap_or_else(|| miette::miette!("Control.retry: all attempts failed")))
        }),
        // Control.with_timeout(duration, fn) — abort fn if it doesn't
        // complete within `duration`. Raises TimeoutError on expiry.
        "with_timeout" => |interp, args| Box::pin(async move {
            let duration = args.iter().find_map(|a| match &a.value {
                Value::Duration(s) => Some(*s),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.with_timeout: missing duration argument"))?;
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.with_timeout: missing closure argument"))?;

            let dur = std::time::Duration::from_secs_f64(duration);
            let fut = interp.call_closure(&params, &body, vec![]);
            match tokio::time::timeout(dur, fut).await {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(miette::miette!("TimeoutError: Control.with_timeout exceeded {duration}s")),
            }
        }),
        // Control.with_deadline(datetime_str, fn) — abort fn if the
        // absolute deadline (RFC 3339 / ISO 8601) passes before fn returns.
        "with_deadline" => |interp, args| Box::pin(async move {
            let when_str = positional(&args, 0)
                .map(|v| v.to_display_string())
                .ok_or_else(|| miette::miette!("Control.with_deadline: missing datetime argument"))?;
            let target = parse_datetime(&when_str)
                .ok_or_else(|| miette::miette!("Control.with_deadline: cannot parse `{when_str}` as an ISO 8601 datetime"))?;
            let now = interp.runtime.clock.now_utc();
            let remaining = (target - now).num_milliseconds().max(0) as u64;
            let (params, body) = args.iter().find_map(|a| match &a.value {
                Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
                _ => None,
            }).ok_or_else(|| miette::miette!("Control.with_deadline: missing closure argument"))?;

            let dur = std::time::Duration::from_millis(remaining);
            let fut = interp.call_closure(&params, &body, vec![]);
            match tokio::time::timeout(dur, fut).await {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(miette::miette!("DeadlineError: Control.with_deadline exceeded `{when_str}`")),
            }
        }),
    })
}
