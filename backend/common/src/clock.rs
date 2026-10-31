//! Injectable clock so domain logic never reads wall time directly.

use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug)]
pub struct FakeClock {
    now: Mutex<DateTime<Utc>>,
}

impl FakeClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            now: Mutex::new(now),
        }
    }

    pub fn advance(&self, duration: Duration) {
        if let Ok(mut now) = self.now.lock() {
            *now += duration;
        }
    }

    pub fn set(&self, now: DateTime<Utc>) {
        if let Ok(mut slot) = self.now.lock() {
            *slot = now;
        }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
            .lock()
            .map(|now| *now)
            .unwrap_or_else(|poisoned| *poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 31, 8, 0, 0)
            .single()
            .unwrap_or_default()
    }

    #[test]
    fn fake_clock_advances_deterministically() {
        let clock = FakeClock::new(fixed());
        assert_eq!(clock.now(), fixed());
        clock.advance(Duration::seconds(90));
        assert_eq!(clock.now(), fixed() + Duration::seconds(90));
    }

    #[test]
    fn fake_clock_can_be_repositioned() {
        let clock = FakeClock::new(fixed());
        clock.set(fixed() + Duration::days(1));
        assert_eq!(clock.now(), fixed() + Duration::days(1));
    }
}
