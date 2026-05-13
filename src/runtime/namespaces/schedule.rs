use crate::interpreter::value::Value;
use crate::interpreter::{CallArgValue, Interpreter, Namespace};
use crate::runtime::namespace::{ns, positional};

pub(crate) fn namespace() -> Namespace {
    ns!("Schedule", {
        // `Schedule.every(duration, () => { ... })` fires the closure
        // once immediately, then again every `duration` for the life
        // of the enclosing agent. Must be called from an @on_start or
        // an agent task — outside an agent there's no context to bind
        // the closure to.
        "every" => |interp, args| Box::pin(async move {
            schedule_fire(interp, args, /* recurring */ true).await
        }),
        // `Schedule.after(duration, () => { ... })` fires the closure
        // once after `duration`.
        "after" => |interp, args| Box::pin(async move {
            schedule_fire(interp, args, /* recurring */ false).await
        }),
        // `Schedule.at(datetime_str, () => { ... })` fires the closure
        // once at the given absolute time. Accepts:
        //   - RFC 3339 / ISO 8601: `"2026-04-20T10:00:00Z"` or
        //     `"2026-04-20T10:00:00+02:00"`
        //   - Naive local datetime: `"2026-04-20T10:00:00"` (treated as UTC)
        // If the target is already in the past, fires immediately.
        "at" => |interp, args| Box::pin(async move {
            schedule_at(interp, args).await
        }),
        // `Schedule.cron(expr, () => { ... })` schedules a recurring closure
        // using a standard 5-field cron expression (minute hour day month weekday).
        "cron" => |interp, args| Box::pin(async move {
            schedule_cron(interp, args).await
        }),
        "sleep" => |_i, args| Box::pin(async move {
            if let Some(Value::Duration(secs)) = positional(&args, 0) {
                tokio::time::sleep(std::time::Duration::from_secs_f64(*secs)).await;
            }
            Ok(Value::None)
        }),
    })
}

async fn schedule_at(interp: &mut Interpreter, args: Vec<CallArgValue>) -> miette::Result<Value> {
    let when_str = positional(&args, 0)
        .map(|v| v.to_display_string())
        .ok_or_else(|| miette::miette!("Schedule.at: missing datetime argument"))?;

    let target = parse_datetime(&when_str).ok_or_else(|| {
        miette::miette!("Schedule.at: cannot parse `{when_str}` as an ISO 8601 datetime")
    })?;
    let now = interp.runtime.clock.now_utc();
    let delay_secs = (target - now).num_seconds().max(0) as f64;

    let (params, body) = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule.at: missing closure argument"))?;

    let agent_name = interp
        .current_agent
        .as_ref()
        .ok_or_else(|| miette::miette!("Schedule.at must be called from within an agent"))?
        .lock()
        .def
        .name
        .clone();

    let closure_id = interp.register_closure(agent_name.clone(), params, body);
    let tx = interp.event_tx.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs_f64(delay_secs)).await;
        let _ = tx.send(crate::interpreter::Event::FireClosure {
            agent_name,
            closure_id,
        });
    });
    Ok(Value::None)
}

/// Parse an ISO 8601 / RFC 3339 datetime string into UTC. Falls back
/// to naive datetime (treated as UTC) when no timezone is given.
pub(crate) fn parse_datetime(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
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

async fn schedule_cron(interp: &mut Interpreter, args: Vec<CallArgValue>) -> miette::Result<Value> {
    let expr_str = positional(&args, 0)
        .map(|v| v.to_display_string())
        .ok_or_else(|| miette::miette!("Schedule.cron: missing cron expression argument"))?;

    let (params, body) = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule.cron: missing closure argument"))?;

    let agent_name = interp
        .current_agent
        .as_ref()
        .ok_or_else(|| miette::miette!("Schedule.cron must be called from within an agent"))?
        .lock()
        .def
        .name
        .clone();

    let closure_id = interp.register_closure(agent_name.clone(), params, body);
    let tx = interp.event_tx.clone();
    let clock = interp.runtime.clock.clone();

    // Parse and validate the cron expression (5 fields: minute hour day month weekday)
    let cron_spec = parse_cron_spec(&expr_str)
        .ok_or_else(|| miette::miette!("Schedule.cron: invalid cron expression `{expr_str}`"))?;

    tokio::spawn(async move {
        loop {
            let now = clock.now_utc();
            if let Some(next_run) = next_cron_execution(&cron_spec, now) {
                let delay = std::time::Duration::from_millis(
                    (next_run - now).num_milliseconds().max(0) as u64,
                );
                tokio::time::sleep(delay).await;

                if tx
                    .send(crate::interpreter::Event::FireClosure {
                        agent_name: agent_name.clone(),
                        closure_id,
                    })
                    .is_err()
                {
                    break; // receiver dropped — event loop has exited
                }
            } else {
                break;
            }
        }
    });

    Ok(Value::None)
}

/// Parse a 5-field cron expression into a structured format.
/// Format: minute hour day month weekday
/// Each field can be: number, *, comma-separated list, range, or step expression.
struct CronSpec {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days: Vec<u32>,
    months: Vec<u32>,
    weekdays: Vec<u32>,
}

fn parse_cron_spec(expr: &str) -> Option<CronSpec> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }

    Some(CronSpec {
        minutes: parse_cron_field(parts[0], 0, 59)?,
        hours: parse_cron_field(parts[1], 0, 23)?,
        days: parse_cron_field(parts[2], 1, 31)?,
        months: parse_cron_field(parts[3], 1, 12)?,
        weekdays: parse_cron_field(parts[4], 0, 6)?,
    })
}

fn parse_cron_field(field: &str, min: u32, max: u32) -> Option<Vec<u32>> {
    if field == "*" {
        return Some((min..=max).collect());
    }

    let mut values = Vec::new();

    for part in field.split(',') {
        if let Some((range_part, step)) = part.split_once('/') {
            let step_val: u32 = step.parse().ok()?;
            if step_val == 0 {
                return None;
            }

            let range_vals = if range_part == "*" {
                (min..=max).collect::<Vec<_>>()
            } else if let Some((start_str, end_str)) = range_part.split_once('-') {
                let start: u32 = start_str.parse().ok()?;
                let end: u32 = end_str.parse().ok()?;
                if start > end || start < min || end > max {
                    return None;
                }
                (start..=end).collect::<Vec<_>>()
            } else {
                let val: u32 = range_part.parse().ok()?;
                if val < min || val > max {
                    return None;
                }
                vec![val]
            };

            for v in range_vals {
                if (v - min) % step_val == 0 {
                    values.push(v);
                }
            }
        } else if let Some((start_str, end_str)) = part.split_once('-') {
            let start: u32 = start_str.parse().ok()?;
            let end: u32 = end_str.parse().ok()?;
            if start > end || start < min || end > max {
                return None;
            }
            values.extend(start..=end);
        } else {
            let val: u32 = part.parse().ok()?;
            if val < min || val > max {
                return None;
            }
            values.push(val);
        }
    }

    values.sort_unstable();
    values.dedup();
    Some(values)
}

fn next_cron_execution(
    spec: &CronSpec,
    from: chrono::DateTime<chrono::Utc>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::{Datelike, Timelike};

    // Start from the next minute
    let mut dt = from + chrono::Duration::minutes(1);
    // Clear seconds and nanoseconds
    let nanos = dt.nanosecond();
    let secs = dt.second();
    dt = dt - chrono::Duration::seconds(secs as i64) - chrono::Duration::nanoseconds(nanos as i64);

    // Try up to 4 years ahead to find a match
    for _ in 0..2_102_400 {
        let m = dt.minute();
        let h = dt.hour();
        let d = dt.day();
        let mo = dt.month();
        let wd = dt.weekday().num_days_from_sunday();

        if spec.minutes.contains(&m)
            && spec.hours.contains(&h)
            && spec.days.contains(&d)
            && spec.months.contains(&mo)
            && spec.weekdays.contains(&wd)
        {
            return Some(dt);
        }

        dt += chrono::Duration::minutes(1);
    }
    None
}

async fn schedule_fire(
    interp: &mut Interpreter,
    args: Vec<CallArgValue>,
    recurring: bool,
) -> miette::Result<Value> {
    let duration = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Duration(s) => Some(*s),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule: missing duration argument"))?;

    let (params, body) = args
        .iter()
        .find_map(|a| match &a.value {
            Value::Closure(p, b) => Some((p.clone(), (**b).clone())),
            _ => None,
        })
        .ok_or_else(|| miette::miette!("Schedule: missing closure argument"))?;

    let agent_name = interp
        .current_agent
        .as_ref()
        .ok_or_else(|| miette::miette!("Schedule must be called from within an agent"))?
        .lock()
        .def
        .name
        .clone();

    let closure_id = interp.register_closure(agent_name.clone(), params, body);
    let tx = interp.event_tx.clone();
    let dur = std::time::Duration::from_secs_f64(duration);

    if recurring {
        // Fire immediately, then on each tick.
        let _ = tx.send(crate::interpreter::Event::FireClosure {
            agent_name: agent_name.clone(),
            closure_id,
        });
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(dur);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await; // consume the immediate tick (already fired above)
            loop {
                interval.tick().await;
                if tx
                    .send(crate::interpreter::Event::FireClosure {
                        agent_name: agent_name.clone(),
                        closure_id,
                    })
                    .is_err()
                {
                    break; // receiver dropped — event loop has exited
                }
            }
        });
    } else {
        tokio::spawn(async move {
            tokio::time::sleep(dur).await;
            let _ = tx.send(crate::interpreter::Event::FireClosure {
                agent_name,
                closure_id,
            });
        });
    }

    Ok(Value::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone, Timelike};

    #[test]
    fn parse_cron_field_accepts_ranges_steps_and_lists() {
        assert_eq!(parse_cron_field("*/20", 0, 59), Some(vec![0, 20, 40]));
        assert_eq!(
            parse_cron_field("5,10-12", 0, 59),
            Some(vec![5, 10, 11, 12])
        );
        assert_eq!(parse_cron_field("10-20/5", 0, 59), Some(vec![10, 15, 20]));
    }

    #[test]
    fn parse_cron_field_rejects_invalid_values() {
        assert_eq!(parse_cron_field("60", 0, 59), None);
        assert_eq!(parse_cron_field("20-10", 0, 59), None);
        assert_eq!(parse_cron_field("*/0", 0, 59), None);
        assert_eq!(parse_cron_field("abc", 0, 59), None);
    }

    #[test]
    fn next_cron_execution_finds_next_matching_minute() {
        let spec = parse_cron_spec("*/15 * * * *").expect("valid cron spec");
        let from = chrono::Utc
            .with_ymd_and_hms(2026, 5, 8, 10, 7, 42)
            .single()
            .expect("valid datetime");
        let next = next_cron_execution(&spec, from).expect("next run");

        assert_eq!(next.hour(), 10);
        assert_eq!(next.minute(), 15);
        assert_eq!(next.second(), 0);
    }

    #[test]
    fn next_cron_execution_uses_standard_weekday_numbers() {
        let spec = parse_cron_spec("0 9 * * 1-5").expect("valid cron spec");
        let from = chrono::Utc
            .with_ymd_and_hms(2026, 5, 10, 8, 0, 0)
            .single()
            .expect("valid datetime");

        let next = next_cron_execution(&spec, from).expect("next run");

        assert_eq!(next.weekday().num_days_from_sunday(), 1);
        assert_eq!(next.day(), 11);
        assert_eq!(next.hour(), 9);
        assert_eq!(next.minute(), 0);
    }
}
