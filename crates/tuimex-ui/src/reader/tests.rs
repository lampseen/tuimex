use tuimex_core::{Address, Attachment, Body, EventTime, Invite, Link, Picture, Timestamp};

use super::text::{invite, rows};
use super::{files, links, look};
use crate::palette::Tok;
use crate::span::Line;

fn text(line: &Line) -> String {
    line.iter().map(|s| s.text.as_str()).collect()
}

fn drawn(source: &str, measure: usize) -> Vec<String> {
    rows(source, measure, None).0.iter().map(text).collect()
}

fn body(links: &[(&str, &str)]) -> Body {
    Body {
        text: String::new(),
        links: links
            .iter()
            .enumerate()
            .map(|(i, (url, shown))| Link { n: i + 1, url: url.to_string(), shown: shown.to_string() })
            .collect(),
        attachments: Vec::new(),
        invite: None,
        blocked_remote: Vec::new(),
        remote_images: 0,
        image_hosts: Vec::new(),
        loadable_images: 0,
        trackers: Vec::new(),
        unsubscribe: None,
        pictures: Vec::new(),
    }
}

fn file(index: usize, name: &str) -> Attachment {
    Attachment { index, name: name.to_string(), mime: "image/png".to_string(), size: 2048, inline: true }
}

#[test]
fn a_line_is_drawn_by_how_it_begins() {
    let got =
        drawn("# Title\n\n> quoted words here\n* one\n* two\n####### seven\n#not a heading\n[image: a chart]", 12);
    let want = [
        "Title",
        "",
        "│ quoted",
        "│ words here",
        "· one",
        "· two",
        "#######",
        "seven",
        "#not a",
        "heading",
        "[image: a",
        "chart]",
    ];
    assert_eq!(got, want);
    let styled = rows("# Title\n[image: a chart]\nplain", 40, None).0;
    assert!(styled[0][0].bold && !styled[2][0].bold);
    assert_eq!(styled[1][0].fg, crate::palette::Tok::Dim);
}

#[test]
fn tabs_make_a_table_with_its_first_row_dim_and_one_tab_makes_none() {
    let table = rows("Route\tLength\nRidge loop\t12 km\nMill\t7", 40, None).0;
    assert_eq!(
        table.iter().map(text).collect::<Vec<_>>(),
        ["Route        Length   ", "Ridge loop   12 km    ", "Mill         7        "]
    );
    assert_eq!((table[0][0].fg, table[1][0].fg), (crate::palette::Tok::Dim, crate::palette::Tok::Fg));
    assert_eq!(drawn("name\tvalue", 40), ["name    value"], "a tab alone is space, never nothing");
}

#[test]
fn what_follows_the_last_rule_is_small_print_and_the_rule_is_not_drawn() {
    let got = rows("above\n\n---\n\nmiddle\n\n────────\n\nbelow\n-- \nsigned", 40, None).0;
    assert_eq!(got.iter().map(text).collect::<Vec<_>>(), ["above", "", "middle", "", "below", "-- ", "signed"]);
    let tones: Vec<_> = got.iter().filter(|l| !l.is_empty()).map(|l| l[0].fg).collect();
    use crate::palette::Tok::{Dim, Fg};
    assert_eq!(tones, [Fg, Fg, Dim, Dim, Dim], "a signature's dashes are not a rule");
}

/// A picture is drawn where the text names its file, and after the text
/// when nothing names it. Its cells never exceed the design's 48 by 12.
#[test]
fn a_picture_takes_cells_by_its_shape_and_is_placed_once() {
    let mut b = body(&[]);
    b.attachments = vec![file(0, "wide.png"), file(1, "tall.png"), file(2, "notes.txt")];
    b.pictures = vec![Picture { index: 0, width: 4000, height: 500 }, Picture { index: 1, width: 10, height: 9000 }];
    let (lines, placed) = rows("before\n[image: tall.png]\nafter", 76, Some(&b));
    let at: Vec<_> = placed.iter().map(|p| (p.index, p.row, p.width, p.height)).collect();
    assert_eq!(at, [(1, 1, 48, 12), (0, 15, 48, 3)]);
    assert_eq!(text(&lines[13]), "after");
    assert_eq!(
        text(&lines[16]),
        format!("{0}{0}{0}{0}{0}{0}{0}{0}{0}{0}{0}░░░ wide.png  4000x500 {0}{0}{0}{0}{0}{0}{0}{0}{0}{0}{0}░░░", "░")
    );
    let narrow = rows("[image: wide.png]", 10, Some(&b)).1;
    assert_eq!((narrow[0].width, narrow[0].height), (10, 1), "no wider than the text, never no rows");
    let zero = Picture { index: 0, width: 0, height: 0 };
    b.pictures = vec![zero];
    assert_eq!(rows("", 76, Some(&b)).1[0].height, 1, "a picture claiming no size divides nothing by zero");
}

/// ADR-018, over the corpus's shapes of link: the host shown is the one a
/// browser would go to, and what is wrong is said beside it.
#[test]
fn a_link_s_row_shows_its_real_host_and_what_is_wrong_with_it() {
    let mut b = body(&[
        ("https://login.example.net/session", "https://www.example.com/login"),
        ("https://www.example.com@login.example.net/session", "Sign in"),
        ("http://192.0.2.7/session", "Sign in"),
        ("file:///etc/passwd", "open the report"),
        ("https://www.example.com/offer?id=7&utm_source=mail", "See the offer"),
        ("https://t.example.net/c/1", "Read more"),
        ("https://www.example.com/{dim|x}", "{err|braces}"),
    ]);
    b.trackers = vec!["t.example.net".to_string()];
    let got: Vec<String> = links(&b).iter().map(text).collect();
    assert_eq!(
        got,
        [
            " 1  login.example.net   https://www.example.com/login   ▲ text says www.example.com",
            " 2  login.example.net   Sign in   ▲ address carries a sign-in name",
            " 3  192.0.2.7           Sign in   ▲ host is a bare IP address",
            " 4  file:               open the report   ▲ will not open",
            " 5  www.example.com     See the offer   tracking removed",
            " 6  t.example.net       Read more   tracking service",
            " 7  www.example.com     {err|braces}",
        ]
    );
    let fifth = look(&b.links[4], &b.trackers);
    assert_eq!(fifth.url, "https://www.example.com/offer?id=7");
    assert!(look(&b.links[3], &b.trackers).blocked && !fifth.blocked);
}

#[test]
fn a_file_s_row_has_its_number_size_and_type() {
    let got: Vec<String> = files(&[file(0, "a.png"), file(4, "longer-name.png")], &[4]).iter().map(text).collect();
    assert_eq!(
        got,
        [" 1  a.png                2 KB   image/png", " 2  longer-name.png      2 KB   image/png   shown above"]
    );
}

#[test]
fn an_invitation_s_card_says_when_and_who_in_words() {
    let person = |name: &str, email: &str| Address { name: Some(name.to_string()), email: email.to_string() };
    let day = 1_795_651_200;
    let mut card = Invite {
        uid: "1".into(),
        summary: "Review".into(),
        start: EventTime::At(Timestamp(day + 36_000)),
        end: Some(EventTime::At(Timestamp(day + 86_400 + 3600))),
        location: None,
        organizer: Some(person("Ada", "ada@example.com")),
        attendees: vec![person("Ada", "ada@example.com"), person("Bo", "bo@example.com")],
        recurrence: Some("every week".into()),
    };
    let me = ["ME@example.com".to_string()];
    let work = ("work", Tok::Head);
    let got: Vec<String> = invite(&card, &me, work).iter().map(text).collect();
    assert_eq!(
        got,
        [
            "What     Review",
            "When     Thu 26 Nov 2026, 10:00 to Fri 27 Nov 2026, 01:00",
            "Who      Ada and 1 other",
            "Repeats  every week",
            "Calendar work · the account it came to",
            "",
        ]
    );
    card.attendees.push(person("Me", "me@example.com"));
    (card.start, card.end, card.organizer) = (EventTime::Day(Timestamp(day)), None, None);
    let got: Vec<String> = invite(&card, &me, work).iter().map(text).collect();
    assert_eq!(got[1..3], ["When     Thu 26 Nov 2026", "Who      you and 2 others"]);
    card.start = EventTime::Local { wall: Timestamp(day + 36_000), zone: Some("Mars/Olympus".into()) };
    assert_eq!(text(&invite(&card, &me, work)[1]), "When     Thu 26 Nov 2026, 10:00 Mars/Olympus");
}

/// The design's rule for the narrow preview: no word of a message is cut.
/// Each kind of line at a 56-cell measure.
#[test]
fn at_a_narrow_measure_every_kind_of_line_wraps_and_none_is_cut() {
    let long = "one two three four five six seven eight nine ten eleven twelve thirteen";
    let all = |source: &str| drawn(source, 56);
    assert_eq!(
        all(&format!("# {long}")),
        ["one two three four five six seven eight nine ten eleven", "twelve thirteen"],
        "a heading"
    );
    assert!(rows(&format!("# {long}"), 56, None).0.iter().all(|row| row[0].bold), "bold on every row of it");
    assert_eq!(
        all(&format!("* {long}")),
        ["· one two three four five six seven eight nine ten", "  eleven twelve thirteen"],
        "a list item, under its own text"
    );
    assert_eq!(
        all(&format!("> {long}")),
        ["│ one two three four five six seven eight nine ten", "│ eleven twelve thirteen"],
        "a quote, behind its bar"
    );
    assert_eq!(
        all(&format!("[image: {long}]")),
        ["[image: one two three four five six seven eight nine ten", "eleven twelve thirteen]"],
        "an image's alt text"
    );
    assert_eq!(
        all(&format!("-- \n{long}\nRobin")),
        ["-- ", "one two three four five six seven eight nine ten eleven", "twelve thirteen", "Robin"],
        "a signature's lines"
    );
    assert!(all(long).iter().chain(&all(&format!("* {long}"))).all(|row| row.chars().count() <= 56));
}

#[test]
fn a_table_too_wide_for_the_measure_is_each_row_s_cells_in_a_line_parted_by_dots() {
    let source = "Route\tLength\tStatus\nRidge loop above the saddle hut\t12 km\tgroomed since the weekend\nMill creek\t7 km\tclosed";
    assert_eq!(drawn(source, 76).len(), 3, "columns while a row fits");
    assert!(drawn(source, 76)[1].starts_with("Ridge loop above the saddle hut   12 km    groomed"));
    let narrow = rows(source, 30, None).0;
    let drawn: Vec<String> = narrow.iter().map(text).collect();
    let want = [
        "Route · Length · Status",
        "Ridge loop above the saddle",
        "  hut · 12 km · groomed since",
        "  the weekend",
        "Mill creek · 7 km · closed",
    ];
    assert_eq!(drawn, want);
    assert_eq!((narrow[0][1].fg, narrow[1][1].fg), (crate::palette::Tok::Dim, crate::palette::Tok::Fg));
}

#[test]
fn a_picture_s_placeholder_is_no_wider_than_the_measure_and_keeps_what_fits_of_its_label() {
    let mut read = body(&[]);
    read.attachments = vec![file(0, "a-photograph-with-a-long-name.png")];
    read.pictures = vec![Picture { index: 0, width: 1600, height: 1200 }];
    let (drawn, placed) = rows("[image: a-photograph-with-a-long-name.png]", 20, Some(&read));
    assert_eq!((placed[0].width, drawn.len()), (20, placed[0].height));
    assert!(drawn.iter().all(|row| text(row).chars().count() == 20), "{drawn:#?}");
    assert!(
        drawn.iter().any(|row| text(row) == " a-photograph-with-a"),
        "{:#?}",
        drawn.iter().map(text).collect::<Vec<_>>()
    );
}

fn headed(change: impl FnOnce(&mut tuimex_core::Listed), brief: bool, width: usize) -> Vec<String> {
    let mut msg = tuimex_fixtures::design::inbox().into_iter().find(|m| m.envelope.id.key == "bank").unwrap();
    change(&mut msg);
    let rows = super::header(&msg, None, None, brief, width);
    assert!(rows.iter().all(|row| crate::span::cells(row) <= width), "nothing is wider than the row");
    rows.iter().map(text).collect()
}

fn person(n: usize) -> Address {
    Address { name: Some(format!("P{n}")), email: format!("p{n}@example.net") }
}

/// The design's header block at a 56-cell measure: the address a notice
/// warns about is the part that must not be lost.
#[test]
fn a_notice_wraps_under_its_own_text_and_keeps_the_address_it_warns_about() {
    let was = "a.very.long.mailbox.name.for.accounts.payable@billing.example.net";
    let rows = headed(|m| m.sender_was = Some(was.to_string()), true, 56);
    let notice = [
        "▲ Sender differs from the thread · earlier messages came",
        "  from",
        "  a.very.long.mailbox.name.for.accounts.payable@billing.",
        "  example.net",
    ];
    assert_eq!(rows[3..], notice, "an address wider than the row is broken, and none of it is lost");
    let short = headed(|m| m.sender_was = Some("pay@billing.example.net".to_string()), true, 56);
    assert_eq!(short[4], "  from pay@billing.example.net", "one that fits a row is whole on it");
}

#[test]
fn recipients_are_packed_whole_and_stop_at_two_rows_in_the_preview_and_six_in_the_reader() {
    let many = |m: &mut tuimex_core::Listed| m.envelope.to = (1..=14).map(person).collect();
    let preview = headed(many, true, 56);
    assert_eq!(preview[1], "To       P1 <p1@example.net>, P2 <p2@example.net>,");
    assert_eq!(preview[2], "         P3 <p3@example.net>, and 11 more", "the last row makes room to say so");
    assert_eq!(preview.len(), 3, "From and two rows of To: {preview:#?}");
    let reader = headed(many, false, 56);
    let to: Vec<&String> = reader[1..].iter().take_while(|row| !row.starts_with("Date")).collect();
    assert_eq!(to.len(), 6, "{reader:#?}");
    assert_eq!(to[5], "         P11 <p11@example.net>, and 3 more");
    assert!(to[..5].iter().all(|row| row.ends_with(">,")), "a comma ends a row that another follows");
    let nine = headed(|m| m.envelope.to = (1..=9).map(person).collect(), false, 56);
    assert_eq!(nine[5], "         P9 <p9@example.net>", "a list within the cap is all there, and says no more");
    // The same for files.
    let files = |m: &mut tuimex_core::Listed| {
        m.attachments = (0..5).map(|i| file(i, &format!("a-scanned-page-number-{i}.png"))).collect()
    };
    let listed = headed(files, true, 56);
    assert_eq!(listed[2], "Files    a-scanned-page-number-0.png 2 KB,");
    assert_eq!(listed[3], "         a-scanned-page-number-1.png 2 KB, and 3 more");
}

#[test]
fn an_address_wider_than_the_row_is_broken_and_every_other_is_kept_whole() {
    let long = Address { name: None, email: format!("{}@example.net", "x".repeat(60)) };
    let rows = headed(|m| m.envelope.to = vec![person(1), long.clone(), person(2)], false, 56);
    assert_eq!(rows[1], "To       P1 <p1@example.net>,");
    assert_eq!(rows[2], format!("         {}", &long.email[..47]));
    assert_eq!(rows[3], format!("         {},", &long.email[47..]));
    assert_eq!(rows[4], "         P2 <p2@example.net>");
}
