# Stdlib: `Schedule`

> **Alpha (v0.1).** Breaking changes expected.

The `Schedule` namespace provides recurring, delayed, and one-shot scheduling. It's a library, not a keyword — which means you can use dynamic intervals, cron expressions, and user-defined event sources without language changes.

Under the hood, `Schedule` sits on top of the runtime's timer primitives (`__runtime.sleep`, `__runtime.deadline`) and the agent mailbox, and emits events into whichever agent registered the schedule.

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

## `schedule.every` with calendar alignment <span class="badge badge-soon">Coming soon</span>

```keel
schedule.every(1.day, at: @9am, () => {
  send_weekly_report()
})

schedule.every(monday, at: @9am, () => {
  start_of_week_checklist()
})
```

> **Status:** the `at:` alignment argument is parsed but ignored in v0.1 — intervals fire relative to when the schedule is registered, not anchored to a clock time.

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

```keel
schedule.at(@2026-04-20_10am, () => {
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
  @role "Produce a daily digest of important emails"

  @on_start {
    schedule.every(1.day, at: @8am, () => {
      summary = produce_digest()
      email.send(summary, to: env.require("DIGEST_TO"))
    })
  }
}
```

## Cancelling a schedule <span class="badge badge-soon">Coming soon</span>

`schedule.every`, `after`, `at`, and `cron` return a handle you can cancel:

```keel
heartbeat = schedule.every(30.seconds, () => { ping() })

# Later
heartbeat.cancel()
```

> **Status:** v0.1 scheduling calls return `none` — cancellation handles are not yet plumbed through. Schedules live for the lifetime of the enclosing agent.

## Duration literals

Durations use the `.unit` suffix and are arithmetic-compatible:

```keel
5.seconds    30.minutes    2.hours    1.day    7.days
extended = 30.seconds * 2     # 60 seconds
timeout  = 5.minutes + 30.seconds
```

Both singular and plural forms work (`1.day`, `2.days`).

## Overflow behavior

Scheduled closures are delivered through the interpreter's bounded event queue (default 1024; `KEEL_EVENT_QUEUE_CAPACITY`). The overflow policy differs by schedule type:

**Recurring** (`schedule.every` tick loop, `schedule.cron`): when the queue is full at the moment a tick fires, the tick is **dropped** and the schedule continues normally. This is a coalescing drop — a slow handler that misses one heartbeat does not accumulate a backlog of missed ticks, and the next tick fires on time.

**One-shot** (`schedule.after`, `schedule.at`) and the **initial fire** of `schedule.every`: these **wait for queue space** rather than dropping. Delivery is guaranteed as long as the event loop is still running — if the queue is momentarily full, the callback fires as soon as a slot opens. There is no `RuntimeBusy` error for schedule overflow — scheduler waits are silent and transparent to user code.

## Why a library, not keywords

`schedule.every`, `schedule.after`, and `schedule.at` are prelude functions rather than hard-coded keywords. This matters because common patterns — dynamic intervals like `every N.minutes` where `N` depends on state, cron expressions, pause/resume logic, user-defined event sources (webhooks, subscriptions) — all fight fixed keyword syntax. Keeping `schedule.*` a library sidesteps every one of them. See [The Prelude & Interfaces](./stdlib.md).
