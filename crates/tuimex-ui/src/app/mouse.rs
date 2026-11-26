//! What the pointer does (ADR-065). A press is looked up in what the last
//! frame drew and then does what a key does there, through the same
//! binding, so the pointer can do nothing the keys cannot.

use super::{App, Mode};
use crate::keymap;
use crate::places::PLACES;
use crate::spot::{self, Mouse, Target};

/// The lines one notch of the wheel moves by.
const NOTCH: usize = 3;

impl App {
    pub fn mouse(&mut self, mouse: Mouse) {
        // A press is not the next key of a sequence that was begun.
        self.pending.clear();
        // Where a question is open or text is being typed the pointer is
        // not read: a press there is as likely aimed at the terminal.
        if !matches!(self.mode, Mode::Normal) {
            return;
        }
        match mouse {
            Mouse::Click { col, row } => match spot::at(&self.spots, col, row) {
                Some(Target::Place(place)) => self.place_clicked(place),
                Some(target) if self.listing() => self.list_clicked(target),
                _ => {}
            },
            Mouse::Wheel { col, row, down } if self.listing() => {
                let over_pane = spot::at(&self.spots, col, row) == Some(Target::Pane);
                // Both bindings name the key that goes down first.
                let id = if over_pane { "preview.scroll" } else { "list.move" };
                (0..NOTCH).for_each(|_| self.run(id, usize::from(!down)));
            }
            Mouse::Wheel { .. } => {}
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
            Target::Tab(_) | Target::Place(_) => {}
        }
    }
}
#[cfg(test)]
mod tests;
