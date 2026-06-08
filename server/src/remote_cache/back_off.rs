use serde::{Deserialize, Serialize};

/// The initial back-off time when the minimum offline time was reached.
const BACK_OFF_BASE_TIME_S: i64 = 10;
/// The maximum back-off time between retries.
const BACK_OFF_MAX_TIME_S: i64 = 3 * 60;
/// Minimum time a connection must fail before back-off kicks in.
const BACK_OFF_MIN_TIME_S: i64 = 10 * 60;

/// Holds the current state for backing off an offline remote
#[derive(Clone, Deserialize, Serialize)]
pub struct BackOffState {
    /// How often the back-off time is doubled since the start, up to a maximum of
    /// [BACK_OFF_MAX_TIME_S]
    back_off_doubling_count: u32,
    /// The last error message we got.
    last_error: String,
    /// The first time it failed.
    first_fail_time: i64,
    /// The next time a connection can be retried.
    next_try: i64,
}

impl BackOffState {
    /// Create a new back-off config when a host/remote is not reachable for the first time
    pub fn new(time: i64, error: String) -> Self {
        Self {
            first_fail_time: time,
            next_try: time,
            back_off_doubling_count: 0,
            last_error: error,
        }
    }

    /// Increases the back-off state when the remote is still unreachable.
    /// Returns the next timestamp when it's allowed to retry if set.
    pub fn retried(&mut self, time: i64, error: String) -> Option<i64> {
        self.last_error = error;

        if self.time_to_next_try(time) > 0 {
            // still in the back off period
            return None;
        }

        if self.back_off_doubling_count == 0 && (time - self.first_fail_time) < BACK_OFF_MIN_TIME_S
        {
            // failing, but did not reach the minimum fail timer yet
            return None;
        }

        // we're outside the back-off period, and later than the minimum
        let back_off_time = (BACK_OFF_BASE_TIME_S * 2i64.pow(self.back_off_doubling_count))
            .min(BACK_OFF_MAX_TIME_S);

        if back_off_time < BACK_OFF_MAX_TIME_S {
            self.back_off_doubling_count += 1;
        }

        self.next_try = time + back_off_time;
        Some(self.next_try)
    }

    /// Returns the remaining time to try again in seconds.
    /// If it is 0, we're free to try again.
    pub fn time_to_next_try(&self, current_time: i64) -> u64 {
        self.next_try.saturating_sub(current_time).max(0) as u64
    }

    /// Returns the last error that was set.
    pub fn last_error(&self) -> String {
        self.last_error.clone()
    }
}

#[cfg(test)]
mod test {
    use super::{BACK_OFF_MAX_TIME_S, BACK_OFF_MIN_TIME_S, BackOffState};

    #[test]
    fn test_back_off_calculation() {
        let mut time = 0;
        let mut back_off = BackOffState::new(time, String::new());
        // timeout should be @ 0 seconds

        assert_eq!(back_off.time_to_next_try(time + 1), 0);

        time += BACK_OFF_MIN_TIME_S;
        back_off.retried(time, String::new());
        let mut back_off_time = 10;

        assert_eq!(back_off.time_to_next_try(time + 5), 5);
        assert_eq!(back_off.time_to_next_try(time + 10), 0);
        assert_eq!(back_off.time_to_next_try(time + 20), 0);

        back_off.retried(time + back_off_time, String::new());
        time += back_off_time;
        back_off_time *= 2;
        back_off.retried(time, String::new());

        assert_eq!(back_off.time_to_next_try(time), back_off_time as u64);

        time += back_off_time;
        back_off_time *= 2;
        back_off.retried(time, String::new());
        time += back_off_time;
        back_off_time *= 2;
        back_off.retried(time, String::new());
        time += back_off_time;
        back_off_time *= 2;
        back_off.retried(time, String::new());
        time += back_off_time;
        back_off.retried(time, String::new());

        // retried 7 times, back-off should have reached the max time by now
        assert_eq!(back_off.time_to_next_try(time), BACK_OFF_MAX_TIME_S as u64);

        // simulate 12 hour outage

        let iterations = 12 * 60 * 60 / BACK_OFF_MAX_TIME_S;
        for _ in 0..iterations {
            time += BACK_OFF_MAX_TIME_S;
            back_off.retried(time, String::new());
        }

        // timeout should still be at the maximum
        assert_eq!(back_off.time_to_next_try(time), BACK_OFF_MAX_TIME_S as u64);
    }
}
