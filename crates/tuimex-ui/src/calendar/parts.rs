//! What every view of the calendar is drawn from: the design's
//! `calendarTabs`, `calendarAlerts`, `dayStatus`, `calendarHint`,
//! `eventWords`, `eventRow`, `agendaRows`, `eventFields`, `calendarEntries`
//! and `eventLinks`, its words for a day, and the frame they all sit in. A
//! view is finished without changing this file.

use tuimex_core::AccountId;
use tuimex_core::calendar::{DAY, Said, Standing, civil_from_days, links_of, weekday_of};

use super::{Calendar, Days, Entry, Picked};
use crate::glyphs::{CURSOR, DOT, NOTICE};
use crate::keymap;
use crate::layout::{self, INDENT, MARGIN};
use crate::list::{self, Preview, account_label, heading, status_row};
use crate::palette::Tok;
use crate::reader;
use crate::span::{Line, Span, cells, fill, fit, gap, highlight, left_right, run};
use crate::when::{DAYS, MONTHS, span_of};

pub const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// How many days ahead a list of days looks for something to show.
pub const AHEAD: i64 = 92;

/// The three views of the same events at the same date.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Month,
    Week,
    Day,
}

impl View {
    /// In the order of the tabs, which is the order `1 2 3` pick them in.
    pub const ALL: [View; 3] = [View::Month, View::Week, View::Day];

    pub fn name(self) -> &'static str {
        match self {
            View::Month => "Month",
            View::Week => "Week",
            View::Day => "Day",
        }
    }

    pub fn from_name(name: &str) -> Option<View> {
        View::ALL.into_iter().find(|view| view.name().eq_ignore_ascii_case(name))
    }
}

/// Everything a view is drawn from besides the size of the screen: the
/// options the design's calendar screens take.
#[derive(Clone, Debug)]
pub struct Scene<'a> {
    pub calendar: &'a Calendar,
    pub view: View,
    /// The day the cursor is on.
    pub cursor: i64,
    /// The selected event, in the views that select one.
    pub picked: Option<&'a Picked>,
    /// Whether the focus is in the month's list of events and not in its
    /// grid: the list then selects an event, as the day view does.
    pub focus: bool,
    /// The accounts, in the config's order, and the colour of each.
    pub accounts: &'a [AccountId],
    pub colours: &'a [Tok],
    /// The colour of the words at the right of an event's row.
    pub words: Tok,
    /// The one account the calendar is narrowed to.
    pub narrowed: Option<&'a AccountId>,
    /// What is being looked for, in lower case: it is underlined where it
    /// matches, and an event it does not match is faint.
    pub needle: Option<&'a str>,
    /// The search being shown, as it was typed: only what matches `needle`
    /// is listed, under a Search tab.
    pub search: Option<&'a str>,
    /// Where the selected event is asked to be shown beside the list of
    /// days: `ui.preview`, as for mail.
    pub preview: Preview,
    /// What `list::panel` made, which takes the rows over the status line.
    pub panel: Vec<Line>,
    /// The left of the status line, in place of the day and its count.
    pub left: Option<Line>,
    /// The right of the status line: `alerts`.
    pub right: Line,
    /// What the command line holds; the view's own hint when it is idle.
    pub command: Option<Line>,
}

impl<'a> Scene<'a> {
    pub fn colour_of(&self, account: &AccountId) -> Tok {
        let at = self.accounts.iter().position(|a| a == account);
        at.and_then(|at| self.colours.get(at)).copied().unwrap_or(Tok::Fg)
    }

    /// The rows a view has between the tabs and the panel.
    pub fn room(&self, height: usize) -> usize {
        height.saturating_sub(2 + 3 + self.panel.len())
    }

    /// What is on each day from `from` up to `to`, as this scene shows it:
    /// narrowed to its account, and in a search only what matches.
    pub fn days(&self, from: i64, to: i64) -> Days<'a> {
        let mut days = self.calendar.days(from, to, self.narrowed);
        if let Some(only) = self.needle.filter(|_| self.search.is_some()) {
            days.days.iter_mut().for_each(|day| day.retain(|entry| entry.matches(only)));
        }
        days
    }

    /// The selected event, when it is still there to draw.
    pub fn entry(&self) -> Option<Entry<'a>> {
        self.calendar.find(self.picked?, self.narrowed)
    }

    /// Whether an entry is over: on a day before today, or today and done
    /// by the clock.
    pub fn ended(&self, entry: &Entry) -> bool {
        let today = self.calendar.today();
        entry.day < today || (entry.day == today && !entry.all_day && entry.end <= self.calendar.wall())
    }
}

/// `Thu 26 Nov`.
pub fn day_short(day: i64) -> String {
    let (_, month, dom) = civil_from_days(day);
    format!("{} {dom} {}", DAYS[weekday_of(day) as usize], MONTHS[(month - 1) as usize])
}

/// `Thu 26 November`.
pub fn day_long(day: i64) -> String {
    let (_, month, dom) = civil_from_days(day);
    format!("{} {dom} {}", DAYS[weekday_of(day) as usize], MONTH_NAMES[(month - 1) as usize])
}

/// A day's label in a list of days: the two nearest are named.
pub fn day_label(day: i64, today: i64) -> String {
    let near = match day - today {
        0 => format!("Today {DOT} "),
        1 => format!("Tomorrow {DOT} "),
        _ => String::new(),
    };
    format!("{near}{}", day_long(day))
}

/// The time of day of a wall time, `09:30`.
pub fn clock(wall: i64) -> String {
    let minutes = wall.rem_euclid(DAY) / 60;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// The views as tabs, and at the right what the page shows. A search
/// takes the place of the active tab, after the others.
pub fn tabs(o: &Scene, width: usize, title: &str) -> Line {
    let mut tabs = Line::new();
    for view in View::ALL {
        let on = o.search.is_none() && view == o.view;
        tabs.extend([Span { bold: on, ..run(view.name(), if on { Tok::Acc } else { Tok::Dim }) }, gap(3)]);
    }
    if let Some(query) = o.search {
        tabs.extend([run("Search", Tok::Acc).bold(), gap(1), run(query, Tok::Fg)]);
    }
    left_right(&heading(tabs), &[gap(2), run(title, Tok::Fg).bold(), gap(MARGIN)], width)
}

/// The right of the status line in the calendar: a reminder that is due,
/// what mail says there, each account whose calendar could not be read,
/// and reminders being off. `reminder` is what is about to begin and how
/// soon.
pub fn alerts(calendar: &Calendar, mail: Line, reminder: Option<(&str, String)>, reminders_off: bool) -> Line {
    let mut parts: Vec<Line> = reminder.map(|(what, lead)| self::reminder(what, &lead)).into_iter().collect();
    parts.push(mail);
    for (account, read) in calendar.unread() {
        let mut part = vec![run(format!("{}'s calendar not read", account.0), Tok::Err)];
        part.extend(read.map(|at| run(format!(" {DOT} last read {} ago", age(calendar, at)), Tok::Dim)));
        parts.push(part);
    }
    if reminders_off {
        parts.push(vec![run("reminders off", Tok::Dim)]);
    }
    parts.retain(|part| !part.is_empty());
    parts.join(&run(format!(" {DOT} "), Tok::Dim))
}

/// What is about to begin and how soon, with the key that lists its links.
pub fn reminder(what: &str, lead: &str) -> Line {
    let link = format!(" {DOT} {} for its link", keymap::hint("calendar.links").0);
    vec![run(format!("{NOTICE} {what} in {lead}"), Tok::Warn), run(link, Tok::Dim)]
}

/// How long until something `seconds` away begins, as a reminder says it:
/// `10 min`, `2 h`, `1 h 30 min`. A minute begun counts whole, so it never
/// says less time than there is.
pub fn lead(seconds: i64) -> String {
    let minutes = (seconds.max(0) + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, minutes) => format!("{minutes} min"),
        (hours, 0) => format!("{hours} h"),
        (hours, minutes) => format!("{hours} h {minutes} min"),
    }
}

/// How long ago `at` was, in its largest whole unit: `2m`, `20s`.
pub fn age(calendar: &Calendar, at: tuimex_core::Timestamp) -> String {
    span_of(u32::try_from((calendar.now.0 - at.0).max(0)).unwrap_or(u32::MAX))
}

/// The left of the status line: the day and how much is on it, or which of
/// the day's events is selected, and how many of them wait for an answer.
pub fn day_status(o: &Scene, day: i64) -> Line {
    let days = o.days(day, day + 1);
    let entries = days.on(day);
    let waiting = entries.iter().filter(|entry| entry.waiting()).count();
    let at = o.picked.and_then(|picked| entries.iter().position(|entry| entry.is(picked)));
    let count = match (at, entries.len()) {
        (Some(at), n) => format!("{} of {n}", at + 1),
        (None, 1) => "1 event".to_string(),
        (None, n) => format!("{n} events"),
    };
    let mut line = vec![run(format!("{} {DOT} {count}", day_short(day)), Tok::Dim)];
    if waiting > 0 {
        line.extend([run(format!(" {DOT} "), Tok::Dim), run(format!("{waiting} to answer"), Tok::Warn)]);
    }
    line
}

/// The idle command line: a few keys for the selected event, or for the
/// day when none is. With `listed`, the focus is in the month's list and
/// the way out is back to the grid, by the Alt key that points at it: the
/// list is beside the grid at every width.
pub fn hint(entry: Option<&Entry>, listed: bool) -> Line {
    let mut keys = vec![keymap::hint("calendar.open")];
    let out = match entry {
        None => {
            keys.extend(["calendar.event", "calendar.today", "calendar.new"].map(keymap::hint));
            "calendar.mail"
        }
        Some(entry) => {
            keys.extend(entry.waiting().then(|| keymap::hint("calendar.answer")));
            keys.extend((!links_of(entry.event).is_empty()).then(|| keymap::hint("calendar.links")));
            keys.extend(["calendar.edit", "calendar.delete"].map(keymap::hint));
            "global.back"
        }
    };
    keys.push(if listed { ("Alt-h", keymap::hint("calendar.focus-grid").1) } else { keymap::hint(out) });
    keys.push(keymap::hint("global.help"));
    list::hint(&keys)
}

/// What is not normal about an event, most useful first, each with a
/// colour when it has one of its own.
pub fn event_words(entry: &Entry) -> Vec<(String, Option<Tok>)> {
    let mut words = Vec::new();
    if entry.waiting() {
        words.push(("needs answer".to_string(), Some(Tok::Warn)));
    }
    match entry.standing {
        Some(Standing::Tentative) => words.push(("tentative".to_string(), None)),
        Some(Standing::Declined) => words.push(("declined".to_string(), None)),
        _ => {}
    }
    if let Some(start) = entry.proposed {
        let weekday = DAYS[weekday_of(start.div_euclid(DAY)) as usize];
        words.push((format!("proposed {weekday} {}", clock(start)), None));
    }
    if entry.local {
        words.push(("local".to_string(), None));
    }
    words
}

/// How one row of events is drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct Look {
    pub selected: bool,
    /// What is being looked for does not match it.
    pub faint: bool,
}

/// An event as a row: the account, its hours, the title and where, then
/// the words. It needs you when it is bold; what has ended is dim. Under
/// `CAL_NARROW` cells the account shortens, the hours become the hour it
/// starts, and the words keep room for the longest alone.
pub fn event_row(o: &Scene, entry: &Entry, width: usize, look: Look) -> Line {
    let narrow = width < layout::CAL_NARROW;
    let tone = |fg: Tok| if look.faint { Tok::Faint } else { fg };
    let text = tone(if o.ended(entry) { Tok::Dim } else { Tok::Fg });
    let needle = o.needle.filter(|_| !look.faint);

    let mut row = vec![if look.selected { run(CURSOR, Tok::Acc) } else { gap(1) }, gap(INDENT - 1)];
    // As in the list of mail, an account is named when there is more than
    // one and the view is not narrowed to one.
    if o.accounts.len() > 1 && o.narrowed.is_none() {
        let full = o.accounts.iter().map(|a| account_label(o.accounts, a, layout::ACCOUNT_MOST).chars().count());
        let (letters, apart) = match narrow {
            true => (layout::ACCOUNT_SHORT, layout::ACCOUNT_SHORT_GAP),
            false => (full.max().unwrap_or(0), layout::ACCOUNT_GAP),
        };
        let name = run(account_label(o.accounts, entry.account, letters), tone(o.colour_of(entry.account)));
        row.extend(fit(&[name], letters + apart));
    }
    let hours = match (entry.all_day, narrow) {
        (true, _) => "all day".to_string(),
        (false, true) => clock(entry.start),
        (false, false) => format!("{}-{}", clock(entry.start), clock(entry.end)),
    };
    row.extend(fit(&[run(hours, tone(Tok::Dim))], if narrow { layout::CAL_TIME_NARROW } else { layout::CAL_TIME }));
    row.push(gap(2));

    let words_width = if narrow { layout::CAL_WORDS_NARROW } else { layout::CAL_WORDS };
    let mut words = Line::new();
    for (word, own) in event_words(entry) {
        let mut next = vec![run(word, tone(own.unwrap_or(o.words)))];
        if !words.is_empty() {
            next.insert(0, run(format!(" {DOT} "), tone(o.words)));
        }
        if cells(&words) + cells(&next) > words_width {
            break;
        }
        words.extend(next);
    }
    let bold = entry.waiting() && !look.faint;
    let mut title = highlight(&entry.event.summary, needle, Span { bold, ..run("", text) });
    // Where it is held is said unless it has a link, which says it better.
    if let Some(place) = entry.event.location.as_deref().filter(|_| links_of(entry.event).is_empty()) {
        title.push(gap(2));
        title.extend(highlight(place, needle, run("", tone(Tok::Dim))));
    }
    let room = width.saturating_sub(cells(&row) + MARGIN);
    if words.is_empty() {
        row.extend(fit(&title, room));
    } else {
        row.extend(fit(&title, room.saturating_sub(3 + words_width)));
        row.push(gap(3 + words_width - cells(&words)));
        row.extend(words);
    }
    row.push(gap(MARGIN));
    if look.selected { fill(row, Tok::Sel) } else { row }
}

/// Days from `from` on, each under its label, in exactly `room` rows of
/// `width` cells. A day with nothing on it is passed over, except the
/// first, which says so; a search passes over that one too. It never ends
/// on a label with no row under it. `selected` is the event drawn with the
/// cursor: one further down its day than the rows reach is the last row,
/// and the rows above it follow.
pub fn agenda(o: &Scene, from: i64, room: usize, width: usize, selected: Option<&Picked>) -> Vec<Line> {
    let days = o.days(from, from + AHEAD);
    let today = o.calendar.today();
    // Each line, with whether it is a label.
    let mut lines: Vec<(Line, bool)> = Vec::new();
    let mut at = None;
    for day in from..from + AHEAD {
        if lines.len() >= room {
            break;
        }
        let entries = days.on(day);
        if entries.is_empty() && (day != from || o.search.is_some()) {
            continue;
        }
        if !lines.is_empty() {
            lines.push((Line::new(), false));
        }
        lines.push((heading(vec![run(day_label(day, today), Tok::Dim).bold()]), true));
        if entries.is_empty() {
            lines.push((vec![gap(INDENT), run("Nothing on this day.", Tok::Dim)], false));
        }
        for entry in entries {
            let look = Look {
                selected: selected.is_some_and(|picked| entry.is(picked)),
                faint: o.needle.is_some_and(|needle| !entry.matches(needle)),
            };
            if look.selected {
                at = Some(lines.len());
            }
            lines.push((event_row(o, entry, width, look), false));
        }
    }
    lines.drain(..at.map_or(0, |at| (at + 1).saturating_sub(room)));
    lines.truncate(room);
    while lines.last().is_some_and(|(line, label)| *label || cells(line) == 0) {
        lines.pop();
    }
    let mut out: Vec<Line> = lines.into_iter().map(|(line, _)| fit(&line, width)).collect();
    out.resize(room, fit(&[], width));
    out
}

/// When an event is, in words, and after a dot how far off its day is:
/// `Thu 26 Nov 2026, 10:00 to 11:30 · in 3 days`.
fn when(o: &Scene, entry: &Entry) -> Line {
    let (first, year) = (entry.start.div_euclid(DAY), civil_from_days(entry.day).0);
    let last = (entry.end - 1).max(entry.start).div_euclid(DAY);
    let said = match (entry.all_day, last > first) {
        (false, _) => format!("{} {year}, {} to {}", day_short(first), clock(entry.start), clock(entry.end)),
        (true, true) => format!("{} to {} {year}, all day", day_short(first), day_short(last)),
        (true, false) => format!("{} {year}, all day", day_short(first)),
    };
    let off = match entry.day - o.calendar.today() {
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        -1 => "yesterday".to_string(),
        days if days > 0 => format!("in {days} days"),
        days => format!("{} days ago", -days),
    };
    vec![run(said, Tok::Fg), run(format!(" {DOT} {off}"), Tok::Dim)]
}

/// Everyone asked, each a whole item: the organizer, you with what you
/// have said, then the others with what each has said.
fn guests(o: &Scene, entry: &Entry) -> Vec<Line> {
    let name = |who: &tuimex_core::Address| who.name.clone().filter(|n| !n.is_empty()).unwrap_or(who.email.clone());
    let mine = |email: &str| {
        let own = o.calendar.identities.iter().filter(|identity| identity.account == *entry.account);
        own.into_iter().any(|identity| identity.address.email.eq_ignore_ascii_case(email))
    };
    let mut all: Vec<Line> = Vec::new();
    let organizer = entry.event.organizer.as_ref();
    all.extend(organizer.map(|who| vec![run(format!("{} ", name(who)), Tok::Fg), run("organizer", Tok::Dim)]));
    all.extend(match (entry.proposed, entry.standing) {
        (Some(_), _) => Some(vec![run("you ", Tok::Fg), run("proposed another time", Tok::Dim)]),
        (None, Some(Standing::Needs)) => Some(vec![run("you ", Tok::Fg), run("no answer", Tok::Warn)]),
        (None, Some(Standing::Accepted)) => Some(vec![run("you ", Tok::Fg), run("yes", Tok::Dim)]),
        (None, Some(Standing::Tentative)) => Some(vec![run("you ", Tok::Fg), run("maybe", Tok::Dim)]),
        (None, Some(Standing::Declined)) => Some(vec![run("you ", Tok::Fg), run("no", Tok::Dim)]),
        (None, None) => None,
    });
    let asked = |who: &&tuimex_core::calendar::Guest| {
        !mine(&who.who.email) && organizer.is_none_or(|by| !by.email.eq_ignore_ascii_case(&who.who.email))
    };
    all.extend(entry.event.guests.iter().filter(asked).map(|guest| {
        let said = match guest.said {
            Said::Nothing => "no answer",
            Said::Accepted => "yes",
            Said::Tentative => "maybe",
            Said::Declined => "no",
        };
        vec![run(format!("{} ", name(&guest.who)), Tok::Fg), run(said, Tok::Dim)]
    }));
    all
}

/// The fields every event has, as a header block of `width` cells: when,
/// where, the host its link really opens with the address after it, the
/// calendar that holds it, and everyone asked. `brief` is the form beside
/// a list, which keeps the list of people short; the event's own screen
/// puts its further fields after these.
pub fn event_fields(o: &Scene, entry: &Entry, brief: bool, width: usize) -> Vec<Line> {
    let mut rows = reader::field("When", vec![when(o, entry)]);
    if let Some(place) = &entry.event.location {
        rows.extend(reader::field("Where", vec![vec![run(place.as_str(), Tok::Fg)]]));
    }
    if let Some(link) = links_of(entry.event).first() {
        let look = reader::look(link, &[]);
        rows.extend(reader::field(
            "Join",
            vec![vec![run(format!("{} ", look.host), Tok::Fg), run(look.url, Tok::Dim)]],
        ));
    }
    let own = o.calendar.sources().into_iter().find(|source| source.account == entry.account && !source.local);
    let kept = match (entry.local, own.and_then(|source| source.read)) {
        (true, _) => "local, only in tuimex".to_string(),
        (false, Some(read)) => format!("its own calendar, read {} ago", age(o.calendar, read)),
        (false, None) => "its own calendar".to_string(),
    };
    let account = run(entry.account.0.as_str(), o.colour_of(entry.account));
    rows.extend(reader::field("Calendar", vec![vec![account, run(format!(" {DOT} {kept}"), Tok::Dim)]]));
    if entry.event.organizer.is_some() {
        let most = if brief { reader::ROWS.0 } else { reader::ROWS.1 };
        rows.extend(reader::field("Who", reader::packed(guests(o, entry), width.saturating_sub(reader::LABEL), most)));
    }
    rows.iter().map(|row| fit(row, width)).collect()
}

/// The whole screen round a view's `content`, which is `o.room(height)`
/// rows: the tabs with `title` at their right, the content, the panel, a
/// blank row, the status line and the command line. The status line says
/// `left`, after the account the calendar is narrowed to, and the command
/// line `idle` when nothing else holds it.
pub fn frame(o: &Scene, width: usize, title: &str, content: Vec<Line>, left: Line, idle: Line) -> Vec<Line> {
    let mut lines = vec![tabs(o, width, title), Line::new()];
    lines.extend(content);
    lines.extend(o.panel.iter().cloned());
    let mut status = Line::new();
    if let Some(account) = o.narrowed {
        let only = run(format!("only {DOT}"), Tok::Dim);
        status.extend([run(account.0.as_str(), o.colour_of(account)), gap(1), only, gap(1)]);
    }
    status.extend(o.left.clone().unwrap_or(left));
    lines.extend([Line::new(), status_row(width, &status, &o.right), o.command.clone().unwrap_or(idle)]);
    lines
}

/// The calendars, each one a row to show or hide: the design's
/// `calendarEntries`.
pub fn sources(o: &Scene) -> Vec<Line> {
    let longest = o.accounts.iter().map(|a| account_label(o.accounts, a, layout::ACCOUNT_MOST).chars().count());
    let name_width = longest.max().unwrap_or(0) + layout::ACCOUNT_GAP;
    let entry = |source: &super::Source| {
        let tone = |fg: Tok| if source.hidden { Tok::Faint } else { fg };
        let name = account_label(o.accounts, source.account, layout::ACCOUNT_MOST);
        let events = if source.events == 1 { "1 event".to_string() } else { format!("{} events", source.events) };
        let kept = match (source.local, source.read) {
            (true, _) => "only in tuimex".to_string(),
            (false, Some(read)) => format!("read {} ago", age(o.calendar, read)),
            (false, None) => String::new(),
        };
        let mut row = vec![
            run(format!("{name:<name_width$}"), tone(o.colour_of(source.account))),
            run(format!("{:<10}", if source.local { "local" } else { "its own" }), tone(Tok::Fg)),
            run(format!("{events:<12}"), tone(Tok::Dim)),
            run(kept, tone(Tok::Dim)),
        ];
        if source.hidden {
            row.extend([gap(3), run("hidden", Tok::Dim)]);
        }
        row
    };
    o.calendar.sources().iter().map(entry).collect()
}

/// An event's links as the links panel lists them, the meeting's first,
/// each with the host it really opens (ADR-018), and beside each list the
/// address each opens. The meeting's link is shown as that, unless what
/// the event says of its place names another host, which is then what it
/// is shown as.
pub fn links(entry: &Entry) -> (Vec<Line>, Vec<String>) {
    let links = links_of(entry.event);
    let looks: Vec<reader::Look> = links.iter().map(|link| reader::look(link, &[])).collect();
    let shown = |(link, look): (&tuimex_core::Link, &reader::Look)| match (&look.claims, link.n) {
        (Some(claims), _) => claims.clone(),
        (None, 1) => "the meeting".to_string(),
        (None, _) => link.shown.clone(),
    };
    let shown: Vec<String> = links.iter().zip(&looks).map(shown).collect();
    let listed: Vec<(usize, &str, &reader::Look)> =
        links.iter().zip(&shown).zip(&looks).map(|((link, shown), look)| (link.n, shown.as_str(), look)).collect();
    (reader::link_rows(&listed), looks.iter().map(|look| look.url.clone()).collect())
}
