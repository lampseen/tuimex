//! What the pointer does (ADR-065). A press is looked up in what the last
//! frame drew and then does what a key does there, through the same
//! binding, so the pointer can do nothing the keys cannot.

use super::read::Panel;
use super::{App, Mode};
use crate::compose::{FIELDS, Stage};
use crate::keymap;
use crate::places::PLACES;
use crate::spot::{self, Mouse, Target};

/// The lines one notch of the wheel moves by.
const NOTCH: usize = 3;

impl App {
    /// `ui.mouse`: whether the pointer is read from the start.
    pub fn set_mouse(&mut self, on: bool) {
        self.pointing = on;
    }

    /// Whether whoever owns the terminal is to have it report the pointer.
    pub fn mouse_on(&self) -> bool {
        self.pointing
    }

    pub fn mouse(&mut self, mouse: Mouse) {
        // A press is not the next key of a sequence that was begun.
        self.pending.clear();
        // A report that was not asked for is not a terminal's.
        if !self.pointing {
            return;
        }
        let (col, row, wheel) = match mouse {
            Mouse::Click { col, row } => (col, row, None),
            Mouse::Wheel { col, row, down } => (col, row, Some(down)),
        };
        let on = spot::at(&self.spots, col, row);
        // A question takes a press on one of its choices as that choice's
        // key, and nothing else: unlike any other key, a press beside it
        // is as likely aimed at the terminal and does not cancel it.
        if self.asking() {
            if let (Some(Target::Answer(key)), None) = (on, wheel) {
                self.key(key);
            }
            return;
        }
        // Both scrolling bindings name the key that goes down first.
        match (&self.mode, wheel) {
            (Mode::Normal, None) => self.clicked(on),
            (Mode::Normal, Some(down)) => self.wheeled(on, down),
            // Help's key moves half a page, so a notch is one of them.
            (Mode::Help(..), Some(down)) => self.press("help.scroll", usize::from(!down)),
            (Mode::Messages(_), Some(down)) => (0..NOTCH).for_each(|_| self.press("list.move", usize::from(!down))),
            // Where text is being typed the pointer is not read.
            _ => {}
        }
    }

    /// Whether a question is open, which only its keys answer.
    fn asking(&self) -> bool {
        let staged = self.compose.as_ref().is_some_and(|c| matches!(c.stage, Stage::Closing | Stage::Summary));
        let invite = self.reading.panel() == Some(&Panel::Invite);
        matches!(self.mode, Mode::Confirm(..)) || (matches!(self.mode, Mode::Normal) && (staged || invite))
    }

    /// The `i`th key a binding names, pressed.
    fn press(&mut self, id: &str, i: usize) {
        let key = keymap::hint(id).0.split(' ').nth(i).unwrap_or("");
        keymap::presses(key).into_iter().for_each(|key| self.key(key));
    }

    fn clicked(&mut self, on: Option<Target>) {
        match on {
            Some(Target::Place(place)) => self.place_clicked(place),
            Some(target) if self.listing() => self.list_clicked(target),
            Some(_) if self.cal.open => {}
            Some(target) if self.compose.is_some() => self.field_clicked(target),
            Some(target) => self.reader_clicked(target),
            None => {}
        }
    }

    fn wheeled(&mut self, on: Option<Target>, down: bool) {
        let i = usize::from(!down);
        if self.listing() {
            let id = if on == Some(Target::Pane) { "preview.scroll" } else { "list.move" };
            (0..NOTCH).for_each(|_| self.run(id, i));
        } else if !self.cal.open && self.compose.is_none() {
            (0..NOTCH).for_each(|_| self.reader_run("reader.scroll", i));
        }
    }

    /// Whether the list is the screen, with nothing open over it.
    fn listing(&self) -> bool {
        !self.cal.open && self.compose.is_none() && self.reading.id().is_none()
    }

    /// The bar writes the key that goes to each place, and a press on it
    /// is that key.
    fn place_clicked(&mut self, place: crate::places::Place) {
        let Some(named) = PLACES.iter().find(|named| named.place == place) else { return };
        let key = keymap::hint(named.key).0.rsplit(' ').next().unwrap_or("");
        keymap::presses(key).into_iter().for_each(|key| self.key(key));
    }

    /// A press moves the cursor to what it is on, and on what the cursor
    /// already has it is Enter: one that only brought the terminal to the
    /// front opens nothing.
    fn list_clicked(&mut self, target: Target) {
        (self.said, self.fix) = (None, None);
        match target {
            Target::Tab(tab) if tab != self.tab() || self.search.is_some() => self.switch(tab),
            Target::Row(row) if row == self.selected && !self.focus => self.run("list.open", 0),
            Target::Row(row) => (self.selected, self.focus) = (row, false),
            Target::Pane => self.run("list.focus-preview", 0),
            _ => {}
        }
    }

    /// In the reader the same: a `[n]` lists the links with that one
    /// chosen, which opens nothing, and the chosen entry of a list is
    /// Enter there.
    fn reader_clicked(&mut self, target: Target) {
        (self.said, self.fix) = (None, None);
        let (links, files) = (self.reading.links(), self.files().len());
        let numbered = |n: usize| (1..=links).contains(&n);
        match (target, self.reading.panel()) {
            (Target::Reference(n), Some(Panel::None)) if numbered(n) => {
                self.reader_run("reader.links", 0);
                self.reading.choose(Panel::Links(n - 1));
            }
            (Target::Reference(n), Some(Panel::Links(_))) if numbered(n) => self.reading.choose(Panel::Links(n - 1)),
            (Target::Link(at), Some(Panel::Links(on))) if at == *on => self.reader_run("links.choose", 2),
            (Target::Link(at), Some(Panel::Links(_))) if at < links => self.reading.choose(Panel::Links(at)),
            (Target::File(at), Some(Panel::Files(on))) if at == *on => self.reader_run("attachments.save", 0),
            (Target::File(at), Some(Panel::Files(_))) if at < files => self.reading.choose(Panel::Files(at)),
            _ => {}
        }
    }

    /// A press on a field takes the focus there, by the steps Tab takes.
    /// On the field that has it, it does nothing: Enter there moves on or
    /// starts `$EDITOR`, neither of which a press aimed at the field means.
    fn field_clicked(&mut self, target: Target) {
        let Target::Field(field) = target else { return };
        for _ in FIELDS {
            match &self.compose {
                Some(c) if c.stage == Stage::Fields && c.focus != field => self.press("compose.field", 0),
                _ => return,
            }
        }
    }
}

#[cfg(test)]
mod tests;
