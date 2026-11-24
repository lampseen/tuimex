use std::time::Duration;

use super::Backoff;

#[test]
fn waits_double_from_one_second_and_stop_at_a_minute() {
    let mut backoff = Backoff::default();
    let waits: Vec<u64> = (0..9).map(|_| backoff.wait().as_secs()).collect();
    assert_eq!(waits, [1, 2, 4, 8, 16, 32, 60, 60, 60]);
}

#[test]
fn a_success_starts_the_waits_over() {
    let mut backoff = Backoff::default();
    backoff.wait();
    backoff.wait();
    backoff.reset();
    assert_eq!(backoff.wait(), Duration::from_secs(1));
}
