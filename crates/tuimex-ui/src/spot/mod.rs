//! Where the last frame put what a pointer can be on. A screen is lines of
//! styled text that do not say what they are, so whatever draws something
//! a click can mean says here which cells it took (ADR-065).

use std::ops::Range;

use crate::places::Place;
use crate::span::{Span, cells};

/// What a pointer on a cell is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Place(Place),
    /// A section's tab, by its place among them.
    Tab(usize),
    /// A row of the list, counted as the cursor counts them.
    Row(usize),
    /// The preview.
    Pane,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
    pub row: usize,
    pub cols: Range<usize>,
    pub target: Target,
}

impl Spot {
    /// The cells `span` takes in a row that has `before` ahead of it.
    pub fn over(row: usize, before: &[Span], span: &Span, target: Target) -> Spot {
        let from = cells(before);
        Spot { row, cols: from..from + cells(std::slice::from_ref(span)), target }
    }

    /// The same cells, `rows` lower and `cols` to the right, as they are
    /// once what was drawn by itself is set into a screen.
    pub fn moved(mut self, rows: usize, cols: usize) -> Spot {
        self.row += rows;
        self.cols = self.cols.start + cols..self.cols.end + cols;
        self
    }
}

/// What is at a cell: the last spot said for it, as what is drawn later
/// is drawn over what came before.
pub fn at(spots: &[Spot], col: usize, row: usize) -> Option<Target> {
    spots.iter().rev().find(|spot| spot.row == row && spot.cols.contains(&col)).map(|spot| spot.target)
}

/// What the pointer did, in cells from the top left of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse {
    Click { col: usize, row: usize },
    Wheel { col: usize, row: usize, down: bool },
}

#[cfg(test)]
mod tests;
