//! How long to wait before trying a lost connection again (PLAN §4.2).

use std::time::Duration;

const FIRST: Duration = Duration::from_secs(1);
const LONGEST: Duration = Duration::from_secs(60);

/// One second, then twice the last wait, never more than a minute.
#[derive(Debug)]
pub struct Backoff(Duration);

impl Default for Backoff {
    fn default() -> Backoff {
        Backoff(FIRST)
    }
}

impl Backoff {
    /// The wait before the next attempt.
    pub fn wait(&mut self) -> Duration {
        let wait = self.0;
        self.0 = (wait * 2).min(LONGEST);
        wait
    }

    /// After an attempt that worked: the next failure starts over.
    pub fn reset(&mut self) {
        self.0 = FIRST;
    }
}

#[cfg(test)]
mod tests;
