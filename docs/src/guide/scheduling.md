# Stdlib: `schedule`

The `schedule` module (`use std/schedule`) provides recurring, delayed, and one-shot scheduling. It's a library, not a keyword — which means you can use dynamic intervals, cron expressions, and user-defined event sources without language changes.

Under the hood, `schedule` sits on top of the runtime's timer primitives and the agent mailbox, and emits events into whichever agent registered the schedule.

> `schedule.every`, `schedule.after`, `schedule.at`, and `schedule.cron` all accept a
> named task in place of the lambda literal shown below: `schedule.every(5.minutes, check_inbox)`.

## `schedule.every` — recurring execution

```keel
schedule.every(5.minutes, () => {
  check_inbox()
})

schedule.every(1.hour, () => {
  sync_data()
})
```

Dynamic intervals work because it's just a function call:

```keel
interval = if load_is_high() { 10.minutes } else { 5.minutes }
schedule.every(interval, () => { heartbeat() })
```

Intervals fire relative to when the schedule is registered. To anchor recurring work to a clock time (every weekday at 9am, say), use [`schedule.cron`](#schedulecron--cron-expressions).

## `schedule.after` — delayed one-shot

```keel
schedule.after(30.minutes, () => {
  follow_up(ticket)
})

schedule.after(2.hours, () => {
  io.notify("Check on deployment")
})
```

## `schedule.at` — absolute time

`schedule.at` takes an RFC 3339 / ISO 8601 datetime string. A naive datetime (no offset) is treated as UTC; a target already in the past fires immediately.

```keel
schedule.at("2026-04-20T10:00:00Z", () => {
  launch_campaign()
})
```

## `schedule.cron` — cron expressions

```keel
schedule.cron("0 9-17 * * 1-5", () => {
  poll_status()
})
```

> **Status:** `schedule.cron` is wired for standard 5-field cron expressions with numeric fields and runs from inside an agent lifecycle or handler context.

## `schedule.sleep` — pause execution

`schedule.sleep` suspends the current agent (or task) for a given duration without blocking the runtime event loop:

```keel
schedule.sleep(2.seconds)
schedule.sleep(500.ms)
```

Use `schedule.sleep` inside an agent body to introduce deliberate delays between steps — for example, a polling loop with backoff:

```keel
use std/schedule

agent Poller {
  @on_start {
    for i in 1..10 {
      result = fetch_status()
      if result.ready { stop(self) }
      schedule.sleep(5.seconds)
    }
  }
}
```

## Inside an agent

A schedule typically lives in an `@on_start` lifecycle attribute so it's set up once when the agent starts:

```keel
use std/email
use std/env
use std/schedule

agent DailyDigest {
  @tools [email, env]
  @role "Produce a daily digest of important emails"

  @on_start {
    schedule.cron("0 8 * * *", () => {
      summary = produce_digest()
      email.send(summary, to: env.require("DIGEST_TO"))
    })
  }
}
```

Scheduling calls return `none` — there is no cancellation handle. A schedule lives for the lifetime of the enclosing agent; stopping the agent tears its schedules down.

## Duration literals

Durations use the `.unit` suffix and are arithmetic-compatible:

```keel
short    = 5.seconds
pause    = 30.minutes
week     = 7.days
extended = 30.seconds * 2     # 60 seconds
timeout  = 5.minutes + 30.seconds
```

Both singular and plural forms work (`1.day`, `2.days`).

## Overflow behavior

Scheduled closures are delivered through the interpreter's bounded event queue (default 1024; `KEEL_EVENT_QUEUE_CAPACITY`). The overflow policy differs by schedule type:

**Recurring** (`schedule.every` tick loop, `schedule.cron`): when the queue is full at the moment a tick fires, the tick is **dropped** and the schedule continues normally. This is a coalescing drop — a slow handler that misses one heartbeat does not accumulate a backlog of missed ticks, and the next tick fires on time.

**One-shot** (`schedule.after`, `schedule.at`) and the **initial fire** of `schedule.every`: these **wait for queue space** rather than dropping. Delivery is guaranteed as long as the event loop is still running — if the queue is momentarily full, the callback fires as soon as a slot opens. There is no `RuntimeBusy` error for schedule overflow — scheduler waits are silent and transparent to user code.

## Why a library, not keywords

`schedule.every`, `schedule.after`, and `schedule.at` are ordinary stdlib functions (imported with `use std/schedule`) rather than hard-coded keywords. This matters because common patterns — dynamic intervals like `every N.minutes` where `N` depends on state, cron expressions, pause/resume logic, user-defined event sources (webhooks, subscriptions) — all fight fixed keyword syntax. Keeping `schedule.*` a library sidesteps every one of them. See [The Standard Library](./stdlib.md).
