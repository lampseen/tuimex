use std::fs;

use tuimex_core::calendar::days_from_civil;

use super::*;

const HOUR: i64 = 3_600;

fn at(year: i64, month: i64, day: i64, hour: i64) -> i64 {
    days_from_civil(year, month, day) * DAY + hour * HOUR
}

/// A directory of its own under the workspace's target, never the system's
/// temporary directory.
fn scratch(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("engine-zone-{}-{name}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    root
}

fn put(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn header(version: u8, times: usize, types: usize, chars: usize) -> Vec<u8> {
    let mut out = b"TZif".to_vec();
    out.push(version);
    out.extend([0; 15]);
    for count in [0, 0, 0, times, types, chars] {
        out.extend(u32::try_from(count).unwrap().to_be_bytes());
    }
    out
}

fn block(width: usize, transitions: &[(i64, u8)], offsets: &[i64]) -> Vec<u8> {
    let mut out = Vec::new();
    for &(at, _) in transitions {
        if width == 4 {
            out.extend(i32::try_from(at).unwrap().to_be_bytes());
        } else {
            out.extend(at.to_be_bytes());
        }
    }
    out.extend(transitions.iter().map(|&(_, index)| index));
    for &offset in offsets {
        out.extend(i32::try_from(offset).unwrap().to_be_bytes());
        out.extend([0, 0]);
    }
    out.extend(b"ZZZ\0");
    out
}

/// A zone file: `version` 0 is a version 1 file, with 32-bit times and no
/// footer; any other has a placeholder first block, as `zic -b slim` writes,
/// then the 64-bit block and `footer`.
fn tzif(version: u8, transitions: &[(i64, u8)], offsets: &[i64], footer: &str) -> Vec<u8> {
    if version == 0 {
        let mut out = header(0, transitions.len(), offsets.len(), 4);
        out.extend(block(4, transitions, offsets));
        return out;
    }
    let mut out = header(version, 0, 1, 4);
    out.extend(block(4, &[], &[0]));
    out.extend(header(version, transitions.len(), offsets.len(), 4));
    out.extend(block(8, transitions, offsets));
    out.extend(format!("\n{footer}\n").bytes());
    out
}

/// Central European time from 1980, then the EU rule from the footer.
fn europe() -> Vec<u8> {
    let transitions = [(at(1980, 1, 1, 0), 1), (at(1980, 3, 30, 1), 2), (at(1980, 9, 28, 1), 1)];
    tzif(b'2', &transitions, &[1_800, HOUR, 2 * HOUR], "CET-1CEST,M3.5.0,M10.5.0/3")
}

/// Eastern Australia: daylight time from October to April, across the new year.
fn australia() -> Vec<u8> {
    tzif(b'2', &[(at(1999, 10, 30, 16), 1)], &[10 * HOUR, 11 * HOUR], "AEST-10AEDT,M10.1.0,M4.1.0/3")
}

fn india() -> Vec<u8> {
    tzif(b'2', &[(at(1945, 10, 14, 18), 1)], &[6 * HOUR + 1_800, 5 * HOUR + 1_800], "IST-5:30")
}

#[test]
fn explicit_transitions_are_read_on_both_sides() {
    let zone = parse(&europe()).unwrap();
    let cases = [
        (at(1900, 1, 1, 0), 1_800),
        (at(1980, 1, 1, 0) - 1, 1_800),
        (at(1980, 1, 1, 0), HOUR),
        (at(1980, 3, 30, 1) - 1, HOUR),
        (at(1980, 3, 30, 1), 2 * HOUR),
        (at(1980, 9, 28, 1) - 1, 2 * HOUR),
        (at(1980, 9, 28, 1), HOUR),
    ];
    for (instant, offset) in cases {
        assert_eq!(zone.offset_at(instant), Some(offset), "at {instant}");
    }
    assert_eq!(zone.offset_at_wall(at(1979, 6, 1, 12)), Some(1_800));
    assert_eq!(zone.offset_at_wall(at(1980, 6, 1, 12)), Some(2 * HOUR));
    assert_eq!(zone.offset_at_wall(at(1980, 9, 28, 2)), Some(2 * HOUR), "still summer, on the summer clock");
    assert_eq!(zone.offset_at_wall(at(1980, 9, 28, 4)), Some(HOUR));
}

#[test]
fn the_footer_rule_carries_the_zone_past_the_last_transition() {
    let zone = parse(&europe()).unwrap();
    let yearly: Vec<_> = zone.periods.iter().filter_map(|p| p.yearly).collect();
    assert_eq!(yearly, [(3, -1, 0), (10, -1, 0)]);
    for year in [1981, 2026, 2030, 2100] {
        // Both changes happen at 01:00 UTC.
        let spring = nth_weekday(year, 3, -1, 0) * DAY;
        let autumn = nth_weekday(year, 10, -1, 0) * DAY;
        assert_eq!(zone.offset_at(spring + HOUR - 1), Some(HOUR), "{year}");
        assert_eq!(zone.offset_at(spring + HOUR), Some(2 * HOUR), "{year}");
        assert_eq!(zone.offset_at(autumn + HOUR - 1), Some(2 * HOUR), "{year}");
        assert_eq!(zone.offset_at(autumn + HOUR), Some(HOUR), "{year}");
        assert_eq!(zone.offset_at(at(year, 1, 15, 12)), Some(HOUR), "{year}");
        assert_eq!(zone.offset_at(at(year, 7, 15, 12)), Some(2 * HOUR), "{year}");
        assert_eq!(zone.offset_at_wall(spring + HOUR + 1_800), Some(HOUR), "{year}");
        assert_eq!(zone.offset_at_wall(spring + 3 * HOUR + 1_800), Some(2 * HOUR), "{year}");
        assert_eq!(zone.offset_at_wall(autumn + HOUR), Some(2 * HOUR), "{year}");
        assert_eq!(zone.offset_at_wall(autumn + 4 * HOUR), Some(HOUR), "{year}");
    }
}

#[test]
fn a_southern_rule_spans_the_new_year() {
    let zone = parse(&australia()).unwrap();
    assert_eq!(zone.offset_at(at(1999, 10, 30, 16) - 1), Some(10 * HOUR));
    assert_eq!(zone.offset_at(at(1999, 10, 30, 16)), Some(11 * HOUR));
    assert_eq!(zone.offset_at(at(2000, 1, 15, 0)), Some(11 * HOUR), "explicit, then the footer's April");
    for year in [2000, 2001, 2031, 2045] {
        // Daylight time ends at 03:00 on its own clock and begins at 02:00
        // on standard time: 16:00 UTC the day before, both.
        let ends = nth_weekday(year, 4, 1, 0) * DAY + 3 * HOUR - 11 * HOUR;
        let begins = nth_weekday(year, 10, 1, 0) * DAY + 2 * HOUR - 10 * HOUR;
        assert_eq!(zone.offset_at(ends - 1), Some(11 * HOUR), "{year}");
        assert_eq!(zone.offset_at(ends), Some(10 * HOUR), "{year}");
        assert_eq!(zone.offset_at(begins - 1), Some(10 * HOUR), "{year}");
        assert_eq!(zone.offset_at(begins), Some(11 * HOUR), "{year}");
        assert_eq!(zone.offset_at(at(year, 1, 15, 0)), Some(11 * HOUR), "{year}");
        assert_eq!(zone.offset_at(at(year, 7, 15, 0)), Some(10 * HOUR), "{year}");
        assert_eq!(zone.offset_at(at(year, 12, 31, 20)), Some(11 * HOUR), "{year}");
        assert_eq!(zone.offset_at_wall(at(year, 1, 1, 0) + 1_800), Some(11 * HOUR), "{year}");
        assert_eq!(zone.offset_at_wall(at(year, 6, 30, 12)), Some(10 * HOUR), "{year}");
    }
}

#[test]
fn a_fixed_footer_adds_nothing() {
    for footer in ["IST-5:30", "<+0530>-5:30"] {
        let zone = parse(&tzif(b'2', &[(at(1945, 10, 14, 18), 1)], &[23_400, 19_800], footer)).unwrap();
        assert_eq!(zone.periods.len(), 2, "{footer}");
        assert!(zone.periods.iter().all(|p| p.yearly.is_none()), "{footer}");
        assert_eq!(zone.offset_at(at(2035, 7, 1, 0)), Some(19_800), "{footer}");
        assert_eq!(zone.offset_at(at(1940, 7, 1, 0)), Some(23_400), "{footer}");
    }
}

#[test]
fn a_footer_in_another_form_leaves_the_explicit_transitions() {
    for footer in
        ["EST5EDT,J60/2,J300/2", "EST5EDT,60,300", "<-02>2<-01>,M3.5.0/-1,M10.5.0/0", "EST5EDT", ":Europe/X", ""]
    {
        let zone = parse(&tzif(b'2', &[(at(2000, 1, 1, 0), 1)], &[-4 * HOUR, -5 * HOUR], footer)).unwrap();
        assert!(zone.periods.iter().all(|p| p.yearly.is_none()), "{footer:?}");
        assert_eq!(zone.offset_at(at(2035, 7, 1, 0)), Some(-5 * HOUR), "{footer:?}");
    }
}

#[test]
fn a_posix_rule_is_read_with_its_sign_turned() {
    let Some(Footer::Yearly { std, dst, begins, ends }) = posix(b"EST5EDT,M3.2.0,M11.1.0") else { panic!() };
    assert_eq!((std, dst), (-5 * HOUR, -4 * HOUR));
    assert_eq!((begins.yearly(), begins.time), ((3, 2, 0), 2 * HOUR));
    assert_eq!((ends.yearly(), ends.time), ((11, 1, 0), 2 * HOUR));

    let Some(Footer::Yearly { std, dst, begins, ends }) = posix(b"<+1030>-10:30<+11>-11,M10.1.0,M4.1.0/1:30:15") else {
        panic!()
    };
    assert_eq!((std, dst), (10 * HOUR + 1_800, 11 * HOUR));
    assert_eq!(begins.yearly(), (10, 1, 0));
    assert_eq!((ends.yearly(), ends.time), ((4, 1, 0), HOUR + 1_815));

    assert!(matches!(posix(b"CET-1"), Some(Footer::Fixed)));
    assert!(matches!(posix(b""), Some(Footer::Unread)));
    for bad in
        ["C-1", "CET", "CET-25", "CET-1CEST,M13.1.0,M10.5.0", "CET-1CEST,M3.0.0,M10.5.0", "CET-1CEST,M3.5.7,M10.5.0"]
    {
        assert!(posix(bad.as_bytes()).is_none(), "{bad}");
    }
    for bad in ["CET-1CEST,M3.5.0,M10.5.0,", "CET-1CEST,M3.5.0/24,M10.5.0", "<CE T>-1", "CET-1CEST,M3.5.0"] {
        assert!(posix(bad.as_bytes()).is_none(), "{bad}");
    }
}

#[test]
fn every_version_is_read() {
    let transitions = [(at(1980, 1, 1, 0), 1)];
    for version in [0, b'2', b'3', b'4'] {
        let zone = parse(&tzif(version, &transitions, &[0, HOUR], "CET-1")).unwrap();
        assert_eq!(zone.offset_at(at(1979, 1, 1, 0)), Some(0), "{version}");
        assert_eq!(zone.offset_at(at(1981, 1, 1, 0)), Some(HOUR), "{version}");
    }
    for version in [b'1', b'5', 2] {
        assert_eq!(parse(&tzif(version, &transitions, &[0, HOUR], "CET-1")), None, "{version}");
    }
}

#[test]
fn a_malformed_file_is_refused() {
    let offsets = [0, HOUR];
    assert_eq!(parse(&tzif(b'2', &[(0, 2)], &offsets, "")), None, "a type that does not exist");
    assert_eq!(parse(&tzif(b'2', &[(10, 1), (10, 0)], &offsets, "")), None, "times out of order");
    assert_eq!(parse(&tzif(b'2', &[], &[], "")), None, "no types");
    assert_eq!(parse(&tzif(b'2', &[(i64::MIN, 1)], &offsets, "")), None, "a time beyond any calendar");
    assert_eq!(parse(&tzif(b'2', &[], &[i64::from(i32::MAX)], "")), None, "an offset beyond any zone");
    let mut no_newline = tzif(b'2', &[], &[0], "UTC0");
    no_newline.pop();
    assert_eq!(parse(&no_newline), None);
    let mut trailing = tzif(b'2', &[], &[0], "UTC0");
    trailing.push(b'x');
    assert_eq!(parse(&trailing), None);
    let two_lines = tzif(b'2', &[], &[0], "UTC0\nUTC0");
    assert_eq!(parse(&two_lines), None);
}

#[test]
fn a_file_cut_short_anywhere_is_refused_without_a_panic() {
    for file in [europe(), australia(), tzif(0, &[(at(1980, 1, 1, 0), 1)], &[0, HOUR], "")] {
        assert!(parse(&file).is_some());
        for length in 0..file.len() {
            assert_eq!(parse(&file[..length]), None, "cut at {length}");
        }
    }
}

#[test]
fn garbage_never_panics() {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let valid = europe();
    for _ in 0..5_000 {
        let length = usize::try_from(next() % 400).unwrap();
        let mut bytes: Vec<u8> = (0..length).map(|_| next().to_le_bytes()[0]).collect();
        if bytes.len() >= 5 && next() % 2 == 0 {
            bytes[..5].copy_from_slice(b"TZif2");
        }
        parse(&bytes);
        // The real thing with one byte changed: counts that lie are where
        // a reader goes wrong.
        let mut altered = valid.clone();
        let place = usize::try_from(next() % valid.len() as u64).unwrap();
        altered[place] = next().to_le_bytes()[0];
        parse(&altered);
    }
}

#[test]
fn a_hostile_name_never_reaches_the_filesystem() {
    let root = scratch("hostile");
    put(&root.join("Europe/Berlin"), &europe());
    let dirs = [root.clone()];
    assert!(system_zone("Europe/Berlin", &dirs).is_some());
    let long = "A".repeat(65);
    let hostile = [
        "../../etc/passwd",
        "/etc/passwd",
        "Europe/../../etc/passwd",
        "",
        long.as_str(),
        "Europe/Ber lin",
        "Europe/Berlin\0",
        "Europe//Berlin",
        "Europe/Berlin/",
        "./Europe/Berlin",
        "Europe/./Berlin",
        "Europe\\Berlin",
        "Europe/Berlín",
    ];
    for name in hostile {
        assert!(!plausible(name), "{name:?}");
        assert_eq!(system_zone(name, &dirs), None, "{name:?}");
    }
    assert!(plausible(&"A".repeat(64)));
    for name in ["Etc/GMT+5", "Etc/GMT-14", "America/Argentina/Buenos_Aires", "UTC"] {
        assert!(plausible(name), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn a_link_is_followed_only_while_it_stays_inside() {
    let root = scratch("links");
    let zones = root.join("zones");
    put(&zones.join("Europe/Berlin"), &europe());
    put(&root.join("outside/Real"), &europe());
    std::os::unix::fs::symlink(root.join("outside/Real"), zones.join("Escape")).unwrap();
    std::os::unix::fs::symlink("Europe/Berlin", zones.join("Link")).unwrap();
    std::os::unix::fs::symlink("../outside", zones.join("Out")).unwrap();
    let dirs = [zones.clone()];
    assert_eq!(system_zone("Escape", &dirs), None);
    assert_eq!(system_zone("Out/Real", &dirs), None);
    assert_eq!(system_zone("Link", &dirs), parse(&europe()));
    // The local zone's file is a link wherever the system put it.
    assert_eq!(local_zone(&zones.join("Escape"), 0), parse(&europe()).unwrap());
}

#[test]
fn only_a_regular_file_of_a_sane_size_is_read() {
    let root = scratch("files");
    put(&root.join("Europe/Berlin"), &europe());
    let mut large = europe();
    large.resize(usize::try_from(LIMIT).unwrap() + 1, 0);
    put(&root.join("Large"), &large);
    put(&root.join("Garbage"), b"not a zone at all");
    let dirs = [root.clone()];
    assert_eq!(system_zone("Europe", &dirs), None, "a directory");
    assert_eq!(system_zone("Large", &dirs), None);
    assert_eq!(system_zone("Garbage", &dirs), None);
    assert_eq!(system_zone("Missing", &dirs), None);
}

#[test]
fn the_directories_are_tried_in_turn() {
    let root = scratch("dirs");
    put(&root.join("second/Europe/Berlin"), &europe());
    put(&root.join("first/Europe/Berlin"), b"broken");
    put(&root.join("first/Asia/Kolkata"), &india());
    let dirs = [root.join("absent"), root.join("first"), root.join("second")];
    assert_eq!(system_zone("Europe/Berlin", &dirs), parse(&europe()));
    assert_eq!(system_zone("Asia/Kolkata", &dirs), parse(&india()));
    assert_eq!(system_zone("Asia/Tokyo", &dirs), None);
    assert_eq!(zone_dirs(), [PathBuf::from("/usr/share/zoneinfo"), PathBuf::from("/var/db/timezone/zoneinfo")]);
}

#[test]
fn the_local_zone_falls_back_to_a_fixed_offset() {
    let root = scratch("local");
    put(&root.join("localtime"), &australia());
    put(&root.join("broken"), b"TZif2");
    assert_eq!(local_zone(&root.join("localtime"), 0), parse(&australia()).unwrap());
    assert_eq!(local_zone(&root.join("broken"), -7 * HOUR), Zone::fixed(-7 * HOUR));
    assert_eq!(local_zone(&root.join("missing"), 2 * HOUR), Zone::fixed(2 * HOUR));
    assert_eq!(local_zone(&root, HOUR), Zone::fixed(HOUR), "a directory");
}

#[test]
fn fill_never_replaces_a_zone_the_calendar_defined() {
    let root = scratch("fill");
    put(&root.join("Europe/Berlin"), &europe());
    put(&root.join("Asia/Kolkata"), &india());
    let mut zones = BTreeMap::from([("Europe/Berlin".to_string(), Zone::fixed(123))]);
    let names = ["Europe/Berlin", "Nowhere/Zone", "Asia/Kolkata", "../Asia/Kolkata"].map(String::from);
    fill(&mut zones, names, &[root]);
    assert_eq!(zones.get("Europe/Berlin"), Some(&Zone::fixed(123)));
    assert_eq!(zones.get("Asia/Kolkata"), parse(&india()).as_ref());
    assert_eq!(zones.len(), 2, "{:?}", zones.keys().collect::<Vec<_>>());
}

/// The machine's own zone files against the offsets those zones are known
/// to have had and to be going to have.
#[test]
fn the_system_s_zones_give_the_known_offsets() {
    let dirs = zone_dirs();
    assert!(
        dirs.iter().any(|d| d.is_dir()),
        "none of {dirs:?} exists; Linux and macOS both ship zone files, so this machine cannot prove the reader"
    );
    let berlin_spring = |y| nth_weekday(y, 3, -1, 0) * DAY + HOUR;
    let berlin_autumn = |y| nth_weekday(y, 10, -1, 0) * DAY + HOUR;
    let berlin = vec![
        (at(1990, 1, 15, 12), 1),
        (at(1990, 7, 15, 12), 2),
        // West Germany's summer ended in September until 1996.
        (at(1990, 10, 15, 12), 1),
        (at(2010, 1, 15, 12), 1),
        (at(2010, 7, 15, 12), 2),
        (at(2026, 1, 15, 12), 1),
        (at(2026, 3, 29, 1) - 1, 1),
        (at(2026, 3, 29, 1), 2),
        (at(2026, 10, 25, 1) - 1, 2),
        (at(2026, 10, 25, 1), 1),
        (at(2035, 1, 15, 12), 1),
        (berlin_spring(2035) - 1, 1),
        (berlin_spring(2035), 2),
        (at(2035, 7, 15, 12), 2),
        (berlin_autumn(2035) - 1, 2),
        (berlin_autumn(2035), 1),
    ];
    let new_york = vec![
        (at(1990, 1, 15, 12), -5),
        // Until 2007, from the first Sunday of April to the last of October.
        (at(1990, 3, 20, 12), -5),
        (at(1990, 4, 15, 12), -4),
        (at(1990, 10, 20, 12), -4),
        (at(1990, 11, 15, 12), -5),
        (at(2010, 3, 20, 12), -4),
        (at(2010, 11, 15, 12), -5),
        (at(2026, 3, 8, 7) - 1, -5),
        (at(2026, 3, 8, 7), -4),
        (at(2026, 11, 1, 6) - 1, -4),
        (at(2026, 11, 1, 6), -5),
        (at(2035, 1, 15, 12), -5),
        (at(2035, 3, 20, 12), -4),
        (at(2035, 7, 15, 12), -4),
        (nth_weekday(2035, 11, 1, 0) * DAY + 6 * HOUR - 1, -4),
        (nth_weekday(2035, 11, 1, 0) * DAY + 6 * HOUR, -5),
        (at(2035, 12, 1, 12), -5),
    ];
    let sydney = vec![
        (at(1990, 1, 15, 0), 11),
        (at(1990, 6, 15, 0), 10),
        (at(1990, 12, 15, 0), 11),
        (at(2010, 1, 15, 0), 11),
        (at(2010, 7, 15, 0), 10),
        (at(2026, 1, 15, 0), 11),
        (at(2026, 4, 4, 16) - 1, 11),
        (at(2026, 4, 4, 16), 10),
        (at(2026, 7, 15, 0), 10),
        (at(2026, 10, 3, 16) - 1, 10),
        (at(2026, 10, 3, 16), 11),
        (at(2035, 1, 15, 0), 11),
        (nth_weekday(2035, 4, 1, 0) * DAY - 8 * HOUR - 1, 11),
        (nth_weekday(2035, 4, 1, 0) * DAY - 8 * HOUR, 10),
        (at(2035, 7, 15, 0), 10),
        (at(2035, 12, 15, 0), 11),
    ];
    let kolkata: Vec<_> =
        [1990, 2010, 2026, 2035].iter().flat_map(|&y| [(at(y, 1, 15, 0), 0), (at(y, 7, 15, 0), 0)]).collect();

    let mut checked = 0;
    for (name, table, extra) in [
        ("Europe/Berlin", berlin, 0),
        ("America/New_York", new_york, 0),
        ("Asia/Kolkata", kolkata, 5 * HOUR + 1_800),
        ("Australia/Sydney", sydney, 0),
    ] {
        if !dirs.iter().any(|d| d.join(name).is_file()) {
            eprintln!("{name} is not on this machine; not checked");
            continue;
        }
        let zone = system_zone(name, &dirs).unwrap_or_else(|| panic!("{name} is on this machine and was not read"));
        for (instant, hours) in table {
            let offset = hours * HOUR + extra;
            assert_eq!(zone.offset_at(instant), Some(offset), "{name} at {instant}");
            // The same moment, read back from its wall time, an hour clear
            // of the changes so that no wall time is skipped or repeated.
            let wall = instant + offset;
            if (instant % DAY) == 12 * HOUR || (instant % DAY) == 0 {
                assert_eq!(zone.offset_at_wall(wall), Some(offset), "{name} at wall {wall}");
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "none of the four zones is under {dirs:?}");
}
