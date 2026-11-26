//! The bar of places: the design's `placesRow` and `LAYOUT.places`. The
//! first row of every screen names the places, each by the function key
//! that goes to it, and marks the one in view (ADR-054).

use crate::keymap;
use crate::layout::MARGIN;
use crate::list::heading;
use crate::palette::Tok;
use crate::span::{Line, cells, fit, gap, run};
use crate::spot::{Spot, Target};

/// The cells between places.
pub const GAP: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Mail,
    Calendar,
}

impl Place {
    pub fn name(self) -> &'static str {
        match self {
            Place::Mail => "mail",
            Place::Calendar => "calendar",
        }
    }
}

/// A place as the bar names it.
pub struct Named {
    pub place: Place,
    /// The binding whose last key goes to it.
    pub key: &'static str,
    pub name: &'static str,
    /// What it is called while it is not in view and the row is too narrow
    /// for its name.
    pub short: &'static str,
}

pub const PLACES: [Named; 2] = [
    Named { place: Place::Mail, key: "calendar.mail", name: "Mail", short: "Mail" },
    Named { place: Place::Calendar, key: "global.calendar", name: "Calendar", short: "Cal" },
];

/// How the place that is not in view is written.
#[derive(Clone, Copy)]
enum Form {
    Name,
    Short,
    Key,
}

fn row(here: Place, keys: bool, form: Form) -> Line {
    let mut line = Vec::new();
    for (at, named) in PLACES.iter().enumerate() {
        let key = keymap::hint(named.key).0.rsplit(' ').next().filter(|_| keys).unwrap_or("");
        let on = named.place == here;
        let name = match form {
            Form::Name => named.name,
            _ if on => named.name,
            Form::Short => named.short,
            Form::Key => "",
        };
        let apart = if key.is_empty() || name.is_empty() { "" } else { " " };
        if at > 0 {
            line.push(gap(GAP));
        }
        // The bar is quiet so that the tabs under it are the only accent:
        // the place in view is bold in the ordinary foreground, which tells
        // it without colour, and the other is dim.
        let said = format!("{key}{apart}{name}");
        match on {
            true => line.push(run(said, Tok::Fg).bold()),
            false => line.push(run(said, Tok::Dim)),
        }
    }
    line.retain(|span| !span.text.is_empty());
    heading(line)
}

/// The bar with `here` in view, in `width` cells. `keys` is whether the
/// function keys are read on the screen under it: where letters are typed
/// they are not, and the bar then names no key. Too narrow for all of it,
/// the place not in view takes its short name, and narrower still is its
/// key alone.
pub fn bar(here: Place, keys: bool, width: usize) -> Line {
    let forms: &[Form] = if keys { &[Form::Name, Form::Short, Form::Key] } else { &[Form::Name, Form::Short] };
    let mut made = forms.iter().map(|form| row(here, keys, *form));
    let mut line = made.next().unwrap_or_default();
    while cells(&line) + MARGIN > width {
        match made.next() {
            Some(narrower) => line = narrower,
            None => break,
        }
    }
    fit(&line, width)
}

/// The cells of the bar that go to a place when the pointer is pressed on
/// them, which is what the function key written there does: where `keys`
/// says those are not read, nothing on the bar is pressed either.
pub fn spots(here: Place, keys: bool, width: usize) -> Vec<Spot> {
    if !keys {
        return Vec::new();
    }
    let line = bar(here, keys, width);
    let said = line.iter().enumerate().filter(|(_, span)| !span.text.trim().is_empty());
    said.zip(&PLACES).map(|((at, span), named)| Spot::over(0, &line[..at], span, Target::Place(named.place))).collect()
}

#[cfg(test)]
mod tests;
