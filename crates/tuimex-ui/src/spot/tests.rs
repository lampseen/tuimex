use super::{Spot, Target, at};
use crate::palette::Tok;
use crate::span::{gap, run};

#[test]
fn a_spot_is_the_cells_of_its_span_counted_as_a_terminal_counts_them() {
    let line = [gap(2), run("日本", Tok::Fg), gap(3), run("Unread", Tok::Dim)];
    let spot = Spot::over(0, &line[..3], &line[3], Target::Tab(1));
    assert_eq!(spot.cols, 9..15, "a wide character before it is two cells");
    assert_eq!(spot.moved(1, 4), Spot { row: 1, cols: 13..19, target: Target::Tab(1) });
}

#[test]
fn a_cell_is_what_was_last_drawn_on_it_and_nothing_where_nothing_was() {
    let spots = [
        Spot { row: 2, cols: 0..10, target: Target::Row(0) },
        Spot { row: 2, cols: 6..10, target: Target::Pane },
        Spot { row: 3, cols: 0..10, target: Target::Row(1) },
    ];
    assert_eq!(at(&spots, 5, 2), Some(Target::Row(0)));
    assert_eq!(at(&spots, 6, 2), Some(Target::Pane));
    assert_eq!(at(&spots, 9, 3), Some(Target::Row(1)));
    assert_eq!((at(&spots, 10, 2), at(&spots, 0, 4), at(&[], 0, 0)), (None, None, None));
}
