use super::*;
use crate::Address;
use crate::calendar::{Guest, Moment};

const ME: &str = "robin.vale@example.org";
const LARS: &str = "lars@example.org";

fn event(uid: &str, summary: &str, guests: &[(&str, Said)]) -> Event {
    let guest =
        |(email, said): &(&str, Said)| Guest { who: Address { name: None, email: email.to_string() }, said: *said };
    Event {
        uid: uid.into(),
        sequence: 0,
        instead_of: None,
        summary: summary.into(),
        start: Moment::Utc(0),
        end: None,
        location: None,
        description: None,
        organizer: None,
        guests: guests.iter().map(guest).collect(),
        cancelled: false,
        links: Vec::new(),
        repeat: None,
        excluded: Vec::new(),
        alarms: Vec::new(),
        more: Vec::new(),
    }
}

fn said(event: &Event, email: &str) -> Option<Said> {
    event.guests.iter().find(|guest| guest.who.email == email).map(|guest| guest.said)
}

/// What the provider's copy says of the reader, what the local one says,
/// what is then shown, and why.
type Row = (Option<Said>, Option<Said>, Option<Said>, &'static str);

/// ADR-055: the provider's copy supplies the event and the local file the
/// answer given here, until the provider's copy shows an answer of its
/// own.
#[test]
fn the_same_uid_held_twice_is_one_event_with_the_answer_given_here_until_the_provider_has_one() {
    let table: [Row; 7] = [
        (Some(Said::Nothing), Some(Said::Accepted), Some(Said::Accepted), "answered here, not yet there"),
        (Some(Said::Nothing), Some(Said::Declined), Some(Said::Declined), "declined here, not yet there"),
        (Some(Said::Tentative), Some(Said::Accepted), Some(Said::Tentative), "the provider's answer is its own"),
        (Some(Said::Declined), Some(Said::Accepted), Some(Said::Declined), "and stands whatever was said here"),
        (Some(Said::Nothing), Some(Said::Nothing), Some(Said::Nothing), "nothing said anywhere"),
        (None, Some(Said::Accepted), Some(Said::Accepted), "a feed that leaves the guests out"),
        (Some(Said::Accepted), None, Some(Said::Accepted), "a local copy that names no reader"),
    ];
    for (there, here, shown, why) in table {
        let guests = |me: Option<Said>| {
            let mut guests = vec![(LARS, Said::Accepted)];
            guests.extend(me.map(|said| (ME, said)));
            guests
        };
        let mut own = vec![event("standup", "Standup, moved by its organizer", &guests(there))];
        // The reader's address is matched whatever its case.
        let left = joined(&mut own, vec![event("standup", "Standup", &guests(here))], &["Robin.Vale@example.org"]);
        assert_eq!(left, Vec::new(), "{why}: it is not drawn a second time");
        assert_eq!(own.len(), 1, "{why}");
        assert_eq!(own[0].summary, "Standup, moved by its organizer", "{why}: the provider's copy is the event");
        assert_eq!(said(&own[0], ME), shown, "{why}");
        assert_eq!(said(&own[0], LARS), Some(Said::Accepted), "{why}: another guest's answer is the provider's");
    }
}

#[test]
fn only_the_reader_s_own_answer_is_taken_from_the_local_copy() {
    let mut own = vec![event("standup", "Standup", &[(LARS, Said::Nothing), (ME, Said::Nothing)])];
    let local = vec![event("standup", "Standup", &[(LARS, Said::Declined), (ME, Said::Accepted)])];
    assert_eq!(joined(&mut own, local, &[ME]), Vec::new());
    assert_eq!((said(&own[0], LARS), said(&own[0], ME)), (Some(Said::Nothing), Some(Said::Accepted)));
    // Another account's address is not "you" (ADR-057).
    let mut own = vec![event("standup", "Standup", &[(ME, Said::Nothing)])];
    let local = vec![event("standup", "Standup", &[(ME, Said::Accepted)])];
    assert_eq!(joined(&mut own, local, &["robin@example.com"]), Vec::new());
    assert_eq!(said(&own[0], ME), Some(Said::Nothing));
}

#[test]
fn an_event_only_held_here_stays_local_and_the_provider_s_events_are_untouched() {
    let mut own = vec![event("standup", "Standup", &[])];
    let before = own.clone();
    let dentist = event("dentist", "Dentist", &[]);
    assert_eq!(joined(&mut own, vec![dentist.clone()], &[ME]), vec![dentist.clone()]);
    assert_eq!(own, before);
    assert_eq!(joined(&mut Vec::new(), vec![dentist.clone()], &[ME]), vec![dentist]);
}

/// One occurrence answered here, of a series the provider holds: it is
/// matched to the provider's copy of that occurrence, and where there is
/// none it stands in for the occurrence in the provider's series.
#[test]
fn an_answered_occurrence_joins_the_series_the_provider_holds() {
    let occurrence = |at: i64, me: Said| Event {
        instead_of: Some(Moment::Utc(at)),
        ..event("weekly", "Weekly, this once", &[(ME, me)])
    };
    let series = event("weekly", "Weekly", &[(ME, Said::Nothing)]);

    let mut own = vec![series.clone(), occurrence(7, Said::Nothing)];
    assert_eq!(joined(&mut own, vec![occurrence(7, Said::Declined)], &[ME]), Vec::new());
    assert_eq!(own.len(), 2);
    assert_eq!(said(&own[0], ME), Some(Said::Nothing), "the series itself was not answered");
    assert_eq!(said(&own[1], ME), Some(Said::Declined));

    let mut own = vec![series.clone()];
    assert_eq!(joined(&mut own, vec![occurrence(14, Said::Accepted)], &[ME]), Vec::new());
    assert_eq!(own, vec![series, occurrence(14, Said::Accepted)]);
}
