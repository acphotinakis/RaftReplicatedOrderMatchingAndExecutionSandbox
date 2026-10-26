use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    interval: Duration,
    last_request: Mutex<Option<Instant>>,
}

impl RateLimiter {
    /// Creates a new RateLimiter.
    ///
    /// # Panics
    /// Panics if `requests_per_minute` is 0.
    pub fn new(requests_per_minute: u32) -> Self {
        assert!(
            requests_per_minute > 0,
            "requests_per_minute must be greater than 0"
        );

        let interval = Duration::from_secs_f64(60.0 / requests_per_minute as f64);

        Self {
            interval,
            last_request: Mutex::new(None),
        }
    }

    /// Blocks until the required rate-limit interval has passed.
    pub fn wait(&self) {
        let mut last_request = self.last_request.lock().unwrap();

        if let Some(last_time) = *last_request {
            let elapsed = last_time.elapsed();
            if elapsed < self.interval {
                thread::sleep(self.interval - elapsed);
            }
        }

        *last_request = Some(Instant::now());
    }
}