use tuimex_core::AccountId;
use tuimex_core::calendar::Event;
use tuimex_fixtures::design as fx;

use super::{Item, Patch};
use crate::calendar::Calendar;
use crate::calendar::form::Target;
use crate::calendar::tests::design;

fn work(local: bool) -> Target {
    Target { account: AccountId("work".to_string()), local }
}

fn titles(calendar: &Calendar) -> Vec<String> {
    calendar.days(fx::THU, fx::THU + 1, None).on(fx::THU).iter().map(|e| e.event.summary.clone()).collect()
}

fn vendor(calendar: &Calendar) -> Event {
    calendar.shelves[2].events.iter().find(|event| event.summary == "Vendor call").unwrap().clone()
}

fn held(at: Target, event: &Event, held: bool, unsaved: bool) -> Item {
    Item::Event { at, event: Box::new(event.clone()), held, unsaved }
}

#[test]
fn a_patch_gives_back_the_patch_that_undoes_it() {
    let mut calendar = design();
    let was = calendar.clone();
    let gone = Patch(vec![
        held(work(false), &vendor(&calendar), false, false),
        Item::Proposed { account: work(false).account, uid: "roadmap@design.example.net".to_string(), start: Some(7) },
        Item::Ignored { account: work(false).account, uid: "standup@design.example.net".to_string(), ignored: true },
    ]);
    let back = calendar.patch(&gone, false);
    assert_eq!(titles(&calendar), ["Q4 roadmap review", "Staging migration window"]);
    assert_eq!(calendar.proposed.len(), 1);
    let again = calendar.patch(&back, false);
    assert_eq!(titles(&calendar), titles(&was));
    assert!(calendar.proposed.is_empty() && calendar.ignored.is_empty());
    calendar.patch(&again, false);
    assert_eq!(titles(&calendar), ["Q4 roadmap review", "Staging migration window"], "and what undoes that");
}

#[test]
fn a_patch_done_twice_is_done_once() {
    let mut calendar = design();
    let mut changed = vendor(&calendar);
    changed.summary = "Vendor call, moved".to_string();
    let patch = Patch(vec![held(work(false), &changed, true, false)]);
    calendar.patch(&patch, false);
    let once = calendar.clone();
    let back = calendar.patch(&patch, true);
    assert_eq!(calendar, once);
    assert_eq!(back, patch, "what it would put back is what is already there");
}

#[test]
fn an_event_is_put_in_a_calendar_the_account_did_not_have() {
    let mut calendar = design();
    let event = vendor(&calendar);
    let other = Target { account: AccountId("third".to_string()), local: true };
    let back = calendar.patch(&Patch(vec![held(other.clone(), &event, true, false)]), false);
    let shelf = calendar.shelves.last().unwrap();
    assert_eq!((shelf.account.0.as_str(), shelf.local, shelf.events.len()), ("third", true, 1));
    calendar.patch(&back, false);
    assert!(calendar.shelves.last().unwrap().events.is_empty());
}

/// An event made here has no UID until its keeper gives it one, so the
/// copy read back is another UID's: it is known by its title and time.
#[test]
fn an_unsaved_event_read_back_under_its_keeper_s_uid_is_not_put_twice() {
    let mut read = design();
    let mut made = vendor(&read);
    (made.uid, made.summary) = ("unsaved 1".to_string(), "Offsite planning".to_string());
    let stored = Event { uid: "k7@tuimex".to_string(), ..made.clone() };
    read.shelves[3].events.push(stored.clone());
    let put = Patch(vec![held(work(true), &made, true, true)]);
    let mut again = read.clone();
    again.patch(&put, true);
    assert_eq!(again, read, "over a calendar that holds it, it is left as read");
    let mut first = read.clone();
    first.patch(&put, false);
    assert_eq!(first.shelves[3].events.len(), 3, "the first time it is a second event of that title, as asked");
    // Taking it back takes the keeper's copy.
    let gone = Patch(vec![held(work(true), &made, false, true)]);
    let back = read.patch(&gone, false);
    assert_eq!(read.shelves[3].events.len(), 1);
    assert_eq!(back, Patch(vec![held(work(true), &stored, true, false)]));
}
