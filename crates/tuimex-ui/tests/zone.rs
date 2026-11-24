//! Times are drawn in the zone the binary names. This is a test binary of
//! its own because the zone is the process's and is set once: every other
//! test of this crate runs without one, in UTC, as the goldens are written.

use tuimex_core::Timestamp;
use tuimex_ui::when::{clock, group_of, long_date, set_zone, short_date, stamp};

/// 2026-11-23 14:20 UTC, the design's `NOW`.
const NOW: Timestamp = Timestamp(1_795_443_600);
/// 2026-07-01 00:00 UTC.
const JULY: i64 = 1_782_864_000;

/// A zone nine hours east in winter and ten in summer, which no place
/// keeps: the test must not pass by reading this machine's own.
fn east(at: i64) -> i32 {
    if at < 1_790_000_000 { 10 * 3600 } else { 9 * 3600 }
}

#[test]
fn a_time_is_drawn_as_the_wall_clock_in_the_readers_zone_stood_at_that_instant() {
    set_zone(east);
    assert_eq!(clock(NOW), "23:20");
    assert_eq!(stamp(Timestamp(NOW.0 + 3600)), "24 Nov 00:20", "the day turns when it does there");
    assert_eq!(long_date(Timestamp(NOW.0 + 3600)), "Tue 24 Nov 2026");
    assert_eq!(stamp(Timestamp(JULY)), "1 Jul 10:00", "a summer message keeps its summer hour");
    // 16:00 UTC is tomorrow there, and 14:00 UTC of the day before is
    // already this morning's small hours.
    let (late, early) = (Timestamp(NOW.0 + 6000), Timestamp(NOW.0 - 86_400 + 2400));
    assert_eq!((group_of(late, NOW), group_of(NOW, late)), (0, 1));
    assert_eq!((group_of(early, NOW), short_date(early, 0, NOW)), (0, "00:00".to_string()));
    set_zone(|_| 0);
    assert_eq!(clock(NOW), "23:20", "the zone is set once");
}
