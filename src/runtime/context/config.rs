use super::env::EnvProvider;

/// Pre-computed rank for the default log level `"info"`, matching `log_level_rank("info")`.
/// Avoids an unnecessary `.unwrap()` at every `RuntimeConfig::from_env` call.
const DEFAULT_LOG_RANK: u8 = 1;

/// Default interpreter event queue depth. Overridable via `KEEL_EVENT_QUEUE_CAPACITY`.
pub const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 1024;

pub fn log_level_rank(name: &str) -> Option<u8> {
    match name.to_ascii_lowercase().as_str() {
        "debug" => Some(0),
        "info" => Some(1),
        "warn" | "warning" => Some(2),
        "error" => Some(3),
        _ => None,
    }
}

pub fn log_level_name(rank: u8) -> &'static str {
    match rank {
        0 => "debug",
        1 => "info",
        2 => "warn",
        _ => "error",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    trace: bool,
    log_threshold: u8,
    event_queue_capacity: usize,
}

impl RuntimeConfig {
    pub fn from_env(env: &dyn EnvProvider) -> Self {
        let trace = env.var("KEEL_TRACE").as_deref() == Some("1");
        let log_threshold = env
            .var("KEEL_LOG_LEVEL")
            .and_then(|s| log_level_rank(&s))
            .unwrap_or(DEFAULT_LOG_RANK);
        let event_queue_capacity = env
            .var("KEEL_EVENT_QUEUE_CAPACITY")
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_EVENT_QUEUE_CAPACITY);

        Self {
            trace,
            log_threshold,
            event_queue_capacity,
        }
    }

    pub fn trace_enabled(&self) -> bool {
        self.trace
    }

    pub fn set_trace(&mut self, on: bool) {
        self.trace = on;
    }

    pub fn log_threshold(&self) -> u8 {
        self.log_threshold
    }

    pub fn set_log_threshold(&mut self, name: &str) -> bool {
        match log_level_rank(name) {
            Some(rank) => {
                self.log_threshold = rank;
                true
            }
            None => false,
        }
    }

    pub fn event_queue_capacity(&self) -> usize {
        self.event_queue_capacity
    }
}
