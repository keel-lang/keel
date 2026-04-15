# Scheduling

## every — recurring execution

```keel
every 5.minutes { check_inbox() }
every 1.hour { sync_data() }
every 30.seconds { heartbeat() }
```

The agent stays alive and repeats the block on schedule. Press `Ctrl+C` to stop.

Duration units: `seconds`/`sec`/`s`, `minutes`/`min`/`m`, `hours`/`hr`/`h`, `days`/`day`/`d`

## after — delayed one-time execution

```keel
after 30.minutes { follow_up(ticket) }
after 2.hours { remind user "Check on deployment" }
```

The block executes once after the delay.

## wait — pause execution

```keel
# Wait a fixed duration
wait 5.seconds

# Wait until a condition is true (polls every second)
wait until is_ready
```

## Scheduling in agents

Agents can have multiple `every` blocks:

```keel
agent Monitor {
  role "System monitor"

  every 30.seconds {
    check_health()
  }

  every 1.hour {
    send_report()
  }
}
```

The first tick executes immediately, then repeats on schedule.
