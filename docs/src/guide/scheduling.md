# Stdlib: `Schedule`

> **Alpha (v0.1).** Breaking changes expected.

The `Schedule` namespace provides recurring, delayed, and one-shot scheduling. It's a library, not a keyword — which means you can use dynamic intervals, cron expressions, and user-defined event sources without language changes.

Under the hood, `Schedule` sits on top of the runtime's timer primitives (`__runtime.sleep`, `__runtime.deadline`) and the agent mailbox, and emits events into whichever agent registered the schedule.

## `Schedule.every` — recurring execution

```keel
Schedule.every(5.minutes, () => {
  check_inbox()
})

Schedule.every(1.hour, () => {
  sync_data()
})
```

Dynamic intervals work because it's just a function call:

```keel
interval = if load_is_high() { 10.minutes } else { 5.minutes }
Schedule.every(interval, () => { heartbeat() })
```

## `Schedule.every` with calendar alignment

```keel
Schedule.every(1.day, at: @9am, () => {
  send_weekly_report()
})

Schedule.every(monday, at: @9am, () => {
  start_of_week_checklist()
})
```

## `Schedule.after` — delayed one-shot

```keel
Schedule.after(30.minutes, () => {
  follow_up(ticket)
})

Schedule.after(2.hours, () => {
  Io.notify("Check on deployment")
})
```

## `Schedule.at` — absolute time

```keel
Schedule.at(@2026-04-20_10am, () => {
  launch_campaign()
})
```

## `Schedule.cron` — full cron expressions

```keel
Schedule.cron("0 */15 9-17 * * MON-FRI", () => {
  poll_status()
})
```

## Inside an agent

A schedule typically lives in an `@on_start` lifecycle attribute so it's set up once when the agent starts:

```keel
agent DailyDigest {
  @role "Produce a daily digest of important emails"

  @on_start {
    Schedule.every(1.day, at: @8am, () => {
      summary = produce_digest()
      Email.send(summary, to: Env.require("DIGEST_TO"))
    })
  }
}
```

## Cancelling a schedule

`Schedule.every`, `after`, `at`, and `cron` return a handle you can cancel:

```keel
heartbeat = Schedule.every(30.seconds, () => { ping() })

# Later
heartbeat.cancel()
```

## Duration literals

Durations use the `.unit` suffix and are arithmetic-compatible:

```keel
5.seconds    30.minutes    2.hours    1.day    7.days
extended = 30.seconds * 2     # 60 seconds
timeout  = 5.minutes + 30.seconds
```

Both singular and plural forms work (`1.day`, `2.days`).

## Why a library, not keywords

`Schedule.every`, `Schedule.after`, and `Schedule.at` are prelude functions rather than hard-coded keywords. This matters because common patterns — dynamic intervals like `every N.minutes` where `N` depends on state, cron expressions, pause/resume logic, user-defined event sources (webhooks, subscriptions) — all fight fixed keyword syntax. Keeping `Schedule.*` a library sidesteps every one of them. See [The Prelude & Interfaces](./prelude.md).
