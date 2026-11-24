//! Lines of spans onto a ratatui buffer: the one place a token becomes a
//! colour and a glyph becomes the active tier's.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::glyphs::Glyphs;
use crate::palette::Palette;
use crate::span::{Line, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub palette: Palette,
    pub glyphs: Glyphs,
}

impl Theme {
    fn style(&self, span: &Span) -> Style {
        let mut style = Style::default()
            .fg(self.palette.color(span.fg).unwrap_or(Color::Reset))
            .bg(span.bg.and_then(|bg| self.palette.color(bg)).unwrap_or(Color::Reset));
        if span.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if span.underline {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        style
    }
}

/// Paints `lines` from the top of `area`, one to a row, clipped to it. A
/// short line is padded with plain spaces, as the design pads one, and rows
/// below the last line are left as they were.
pub fn paint(lines: &[Line], buf: &mut Buffer, area: Rect, theme: &Theme) {
    let right = area.right();
    let pad = theme.style(&crate::span::gap(0));
    for (line, y) in lines.iter().zip(area.y..area.bottom()) {
        let mut x = area.x;
        for span in line {
            let text: String = span.text.chars().map(|c| theme.glyphs.map(c)).collect();
            // `set_stringn` leaves out control characters and gives a wide
            // one both of its cells, so text from a message can neither
            // reach the terminal as an escape nor shift what follows it.
            x = buf.set_stringn(x, y, text, usize::from(right.saturating_sub(x)), theme.style(span)).0;
        }
        for x in x..right {
            buf[(x, y)].set_char(' ').set_style(pad);
        }
    }
}
