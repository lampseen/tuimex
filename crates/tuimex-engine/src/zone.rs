//! The system's zone files, read by name and handed to the pure calendar
//! code as plain periods (ADR-061, step 3), and the reader's own zone.
//!
//! A zone's name comes from a calendar, so it is hostile text that would
//! otherwise be a path, and a zone file is bytes this module checks before
//! trusting any count in it. Every failure here means the same thing to the
//! caller, that the zone is not there to use, so errors become `None`.
//! This module only reads (ADR-011).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use tuimex_core::calendar::{DAY, Period, Zone, civil_from_days, nth_weekday};

#[cfg(test)]
mod tests;

/// The largest zone file read. The tz database's largest is a few KiB;
/// the cap keeps a planted file from filling memory.
const LIMIT: u64 = 1 << 20;

const LONGEST_NAME: usize = 64;

/// Transition times beyond this are refused, so that adding an offset to
/// one, and the calendar code's day arithmetic on the result, cannot
/// overflow. The tz database's earliest, -2^59, is inside it.
const FARTHEST: i64 = 1 << 60;

/// UTC offsets beyond this are refused; RFC 8536 suggests a range inside
/// it, and every real zone is within fifteen hours.
const WIDEST: i64 = 26 * 3_600;

/// A footer rule is applied from the first year it would change the clock
/// after the file's last transition; a file with none has always kept it.
const EARLIEST_YEAR: i64 = 1800;

pub fn zone_dirs() -> Vec<PathBuf> {
    vec![PathBuf::from("/usr/share/zoneinfo"), PathBuf::from("/var/db/timezone/zoneinfo")]
}

/// The zone a calendar names, from the first of `dirs` that has it.
pub fn system_zone(name: &str, dirs: &[PathBuf]) -> Option<Zone> {
    if !plausible(name) {
        return None;
    }
    dirs.iter().find_map(|dir| {
        let dir = dir.canonicalize().ok()?;
        let path = dir.join(name).canonicalize().ok()?;
        // A symlink may lead anywhere; only what is still inside counts.
        if !path.starts_with(&dir) {
            return None;
        }
        parse(&read(&path)?)
    })
}

/// The reader's zone, from the file the system keeps it in, or a fixed
/// offset when that cannot be read.
pub fn local_zone(localtime: &Path, fallback_offset: i64) -> Zone {
    let zone = localtime.canonicalize().ok().and_then(|path| read(&path)).and_then(|bytes| parse(&bytes));
    zone.unwrap_or_else(|| Zone::fixed(fallback_offset))
}

/// Adds the system's zone for each name the calendar did not define. A
/// calendar's own definition is what its author's program meant, so it is
/// never replaced.
pub fn fill(zones: &mut BTreeMap<String, Zone>, names: impl IntoIterator<Item = String>, dirs: &[PathBuf]) {
    for name in names {
        if zones.contains_key(&name) {
            continue;
        }
        if let Some(zone) = system_zone(&name, dirs) {
            zones.insert(name, zone);
        }
    }
}

/// Whether a name is made only of what zone names are made of. No `.` is
/// allowed at all, so no segment can climb out of the directory.
fn plausible(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= LONGEST_NAME
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-' | b'/'))
        && name.split('/').all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn read(path: &Path) -> Option<Vec<u8>> {
    // Checked before opening: opening a FIFO or a device can block.
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > LIMIT {
        return None;
    }
    let file = File::open(path).ok()?;
    let mut bytes = Vec::new();
    // Capped again while reading, as the file may have grown since.
    file.take(LIMIT + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= LIMIT).then_some(bytes)
}

/// Bytes taken from the front, each take checked against what is left.
struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.0.len() {
            return None;
        }
        let (taken, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(taken)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn byte(&mut self) -> Option<u8> {
        Some(self.array::<1>()?[0])
    }

    fn count(&mut self) -> Option<usize> {
        usize::try_from(u32::from_be_bytes(self.array()?)).ok()
    }

    fn i32(&mut self) -> Option<i64> {
        Some(i64::from(i32::from_be_bytes(self.array()?)))
    }

    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_be_bytes(self.array()?))
    }
}

struct Header {
    version: u8,
    utc_count: usize,
    std_count: usize,
    leap_count: usize,
    time_count: usize,
    type_count: usize,
    char_count: usize,
}

impl Header {
    fn read(bytes: &mut Bytes) -> Option<Header> {
        if bytes.take(4)? != b"TZif" {
            return None;
        }
        let version = bytes.byte()?;
        if !matches!(version, 0 | b'2' | b'3' | b'4') {
            return None;
        }
        bytes.take(15)?;
        Some(Header {
            version,
            utc_count: bytes.count()?,
            std_count: bytes.count()?,
            leap_count: bytes.count()?,
            time_count: bytes.count()?,
            type_count: bytes.count()?,
            char_count: bytes.count()?,
        })
    }

    /// The length of the data block, whose times are `width` bytes.
    fn block_len(&self, width: usize) -> Option<usize> {
        let parts = [
            self.time_count.checked_mul(width + 1)?,
            self.type_count.checked_mul(6)?,
            self.char_count,
            self.leap_count.checked_mul(width + 4)?,
            self.std_count,
            self.utc_count,
        ];
        parts.into_iter().try_fold(0usize, usize::checked_add)
    }
}

/// A zone file (RFC 8536) as periods. Version 1 has only 32-bit times;
/// later versions follow them with a 64-bit block and a footer rule for
/// the years after the last transition, which is what is read.
fn parse(bytes: &[u8]) -> Option<Zone> {
    let mut bytes = Bytes(bytes);
    let first = Header::read(&mut bytes)?;
    let (head, width) = if first.version == 0 {
        (first, 4)
    } else {
        bytes.take(first.block_len(4)?)?;
        (Header::read(&mut bytes)?, 8)
    };
    let mut block = Bytes(bytes.take(head.block_len(width)?)?);
    let footer = if head.version == 0 {
        Footer::Unread
    } else {
        // The footer is a newline, the rule, a newline, and the end.
        let text = bytes.0.strip_prefix(b"\n")?.strip_suffix(b"\n")?;
        if text.contains(&b'\n') {
            return None;
        }
        // A rule this cannot read is a zone this cannot place a future
        // date in, and a zone half known would be a guess (ADR-061): the
        // time is then shown as written.
        posix(text)?
    };

    if head.type_count == 0
        || (head.std_count != 0 && head.std_count != head.type_count)
        || (head.utc_count != 0 && head.utc_count != head.type_count)
    {
        return None;
    }
    let mut times = Vec::with_capacity(head.time_count);
    for _ in 0..head.time_count {
        let at = if width == 4 { block.i32()? } else { block.i64()? };
        if !(-FARTHEST..=FARTHEST).contains(&at) || times.last().is_some_and(|&before| before >= at) {
            return None;
        }
        times.push(at);
    }
    let indices = block.take(head.time_count)?;
    let mut offsets = Vec::with_capacity(head.type_count);
    for _ in 0..head.type_count {
        let offset = block.i32()?;
        let _is_dst = block.byte()?;
        let name_at = usize::from(block.byte()?);
        if !(-WIDEST..=WIDEST).contains(&offset) || name_at >= head.char_count {
            return None;
        }
        offsets.push(offset);
    }

    let mut offset = *offsets.first()?;
    let mut periods = vec![Period { start: i64::MIN / 2, offset, yearly: None }];
    for (&at, &index) in times.iter().zip(indices) {
        let next = *offsets.get(usize::from(index))?;
        // A change of name alone leaves the clock where it was.
        if next != offset {
            periods.push(Period { start: at + offset, offset: next, yearly: None });
            offset = next;
        }
    }
    if let Footer::Yearly { std, dst, begins, ends } = footer {
        let last = times.last().copied();
        periods.push(Period { start: first_onset(last, &begins, std), offset: dst, yearly: Some(begins.yearly()) });
        periods.push(Period { start: first_onset(last, &ends, dst), offset: std, yearly: Some(ends.yearly()) });
    }
    Some(Zone { periods })
}

/// The wall time, on the clock in force before it, of the first change by
/// `change` after the instant `after`.
fn first_onset(after: Option<i64>, change: &Change, before: i64) -> i64 {
    let Some(after) = after else { return change.onset(EARLIEST_YEAR) };
    let mut year = civil_from_days((after + before).div_euclid(DAY)).0 - 1;
    loop {
        let wall = change.onset(year);
        if wall - before > after {
            return wall;
        }
        year += 1;
    }
}

/// What a footer says about the years after the last transition.
enum Footer {
    /// No rule, or one in a form not read; the last transition holds.
    Unread,
    /// One offset for ever, which the last transition already gives.
    Fixed,
    Yearly {
        std: i64,
        dst: i64,
        begins: Change,
        ends: Change,
    },
}

/// A yearly change of the clock: the `nth` `weekday` of `month` (`-1` for
/// the last), at `time` on the clock in force before it.
struct Change {
    month: i64,
    nth: i64,
    weekday: i64,
    time: i64,
}

impl Change {
    fn yearly(&self) -> (i64, i64, i64) {
        (self.month, self.nth, self.weekday)
    }

    fn onset(&self, year: i64) -> i64 {
        nth_weekday(year, self.month, self.nth, self.weekday) * DAY + self.time
    }
}

/// A POSIX TZ string as a footer carries it, such as
/// `CET-1CEST,M3.5.0,M10.5.0/3`. Its offsets are west of UTC, the opposite
/// sign to a UTC offset. `None` is a form not read: Julian-day rules, and
/// a change whose time is outside its own day, which the calendar code's
/// weekday rules cannot say.
fn posix(text: &[u8]) -> Option<Footer> {
    if text.is_empty() {
        return Some(Footer::Unread);
    }
    let mut s = Text(text);
    s.name()?;
    let std = -s.offset(24)?;
    if s.0.is_empty() {
        return Some(Footer::Fixed);
    }
    s.name()?;
    let dst = match s.0.first() {
        Some(b'+' | b'-' | b'0'..=b'9') => -s.offset(24)?,
        _ => std + 3_600,
    };
    s.eat(b',')?;
    let begins = s.change()?;
    s.eat(b',')?;
    let ends = s.change()?;
    s.0.is_empty().then_some(Footer::Yearly { std, dst, begins, ends })
}

struct Text<'a>(&'a [u8]);

impl Text<'_> {
    fn eat(&mut self, byte: u8) -> Option<()> {
        self.0 = self.0.strip_prefix(&[byte])?;
        Some(())
    }

    /// Up to three digits, at most `max`.
    fn number(&mut self, max: i64) -> Option<i64> {
        let digits = self.0.iter().take(3).take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        let (number, rest) = self.0.split_at(digits);
        self.0 = rest;
        let value = number.iter().fold(0, |n, &d| n * 10 + i64::from(d - b'0'));
        (value <= max).then_some(value)
    }

    /// A zone abbreviation: three or more letters, or anything of letters,
    /// digits and signs between `<` and `>`.
    fn name(&mut self) -> Option<()> {
        let quoted = self.eat(b'<').is_some();
        let fits = |b: &u8| {
            if quoted { b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-') } else { b.is_ascii_alphabetic() }
        };
        let length = self.0.iter().take_while(|&b| fits(b)).count();
        self.0 = &self.0[length..];
        if quoted {
            self.eat(b'>')?;
        }
        (length >= 3).then_some(())
    }

    /// `[+-]hh[:mm[:ss]]` in seconds, with hours at most `hours`.
    fn offset(&mut self, hours: i64) -> Option<i64> {
        let sign = match self.0.first() {
            Some(b'-') => -1,
            _ => 1,
        };
        if matches!(self.0.first(), Some(b'+' | b'-')) {
            self.0 = &self.0[1..];
        }
        let mut seconds = self.number(hours)? * 3_600;
        if self.eat(b':').is_some() {
            seconds += self.number(59)? * 60;
            if self.eat(b':').is_some() {
                seconds += self.number(59)?;
            }
        }
        Some(sign * seconds)
    }

    /// `Mm.w.d[/time]`; week 5 is the month's last.
    fn change(&mut self) -> Option<Change> {
        self.eat(b'M')?;
        let month = self.number(12)?;
        self.eat(b'.')?;
        let week = self.number(5)?;
        self.eat(b'.')?;
        let weekday = self.number(6)?;
        let time = if self.eat(b'/').is_some() { self.offset(167)? } else { 2 * 3_600 };
        if month == 0 || week == 0 || !(0..DAY).contains(&time) {
            return None;
        }
        Some(Change { month, nth: if week == 5 { -1 } else { week }, weekday, time })
    }
}
