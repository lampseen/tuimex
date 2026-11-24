//! One event held twice (ADR-055): by the account's provider, and as a
//! file here because it was answered here. They are one event on screen.

use super::Event;
use super::Said;

#[cfg(test)]
mod tests;

/// Joins what is held here of an account's events to its provider's copy
/// of them, and returns the local events the provider has no copy of,
/// which stay `local`. `mine` are the addresses of that account alone, as
/// `standing` takes them.
///
/// A local event whose UID the provider carries is not drawn a second
/// time. The provider's copy supplies the event; the local one supplies
/// what the reader answered, for as long as the provider's copy shows no
/// answer of theirs: the guest has said nothing there, or is not listed,
/// as in a feed that leaves its guests out. An answered occurrence the
/// provider holds only as part of its series joins the provider's events
/// as that occurrence, so the series draws it once and not beside itself.
pub fn joined(own: &mut Vec<Event>, local: Vec<Event>, mine: &[&str]) -> Vec<Event> {
    let mut left = Vec::new();
    for event in local {
        if !own.iter().any(|held| held.uid == event.uid) {
            left.push(event);
            continue;
        }
        let same = |held: &&mut Event| held.uid == event.uid && held.instead_of == event.instead_of;
        match own.iter_mut().find(same) {
            Some(held) => answer(held, &event, mine),
            None => own.push(event),
        }
    }
    left
}

/// Gives `held` what the reader answered in `local`, where `held` shows
/// no answer of theirs.
fn answer(held: &mut Event, local: &Event, mine: &[&str]) {
    let me = |email: &str| mine.iter().any(|address| address.eq_ignore_ascii_case(email));
    for given in local.guests.iter().filter(|guest| me(&guest.who.email) && guest.said != Said::Nothing) {
        match held.guests.iter_mut().find(|guest| guest.who.email.eq_ignore_ascii_case(&given.who.email)) {
            Some(guest) if guest.said == Said::Nothing => guest.said = given.said,
            // The provider's copy has an answer of its own.
            Some(_) => {}
            None => held.guests.push(given.clone()),
        }
    }
}
