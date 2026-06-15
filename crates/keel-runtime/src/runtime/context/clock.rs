use std::time::Instant;

use chrono::{DateTime, Utc};

pub trait Clock: Send + Sync {
    fn now_utc(&self) -> DateTime<Utc>;
    fn now_instant(&self) -> Instant;
}

#[derive(Default)]
pub struct NativeClock;

impl Clock for NativeClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn now_instant(&self) -> Instant {
        Instant::now()
    }
}

#[cfg(test)]
pub struct FixedClock {
    utc: parking_lot::Mutex<DateTime<Utc>>,
    instant: parking_lot::Mutex<Instant>,
}

#[cfg(test)]
impl FixedClock {
    pub fn new(utc: DateTime<Utc>) -> Self {
        Self {
            utc: parking_lot::Mutex::new(utc),
            instant: parking_lot::Mutex::new(Instant::now()),
        }
    }
}

#[cfg(test)]
impl Clock for FixedClock {
    fn now_utc(&self) -> DateTime<Utc> {
        *self.utc.lock()
    }
    fn now_instant(&self) -> Instant {
        *self.instant.lock()
    }
}
