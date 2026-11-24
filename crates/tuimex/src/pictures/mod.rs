//! Pictures attached to a message, drawn in the reader through the
//! terminal's image protocol where it has one. They are bytes already on
//! disk: nothing here fetches (ADR-008). The UI draws a placeholder of the
//! picture's size in cells and says where it is; this draws over it, and
//! where it cannot the placeholder is what stays.
//!
//! The bytes are a stranger's. Only the four formats PLAN §1 allows are
//! read, a picture larger than `MAX_SIDE` on either side is not decoded
//! at all, and a decoder may allocate no more than `MAX_ALLOC`.

use std::collections::HashMap;
use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, Limits};
use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};
use ratatui::widgets::Widget;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};
use tuimex_core::MessageId;
use tuimex_ui::reader::Placed;

/// The most pixels a picture may have on a side and still be shown.
pub const MAX_SIDE: u32 = 8192;
/// The most a decoder may allocate for one picture, in bytes.
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

const FORMATS: [ImageFormat; 4] = [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Gif, ImageFormat::WebP];

/// A reader over `bytes` that has told one of the four formats from their
/// first bytes; what the message called the file is not asked.
fn reader(bytes: &[u8]) -> Option<ImageReader<Cursor<&[u8]>>> {
    // An in-memory reader cannot fail to be read.
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    reader.format().is_some_and(|format| FORMATS.contains(&format)).then_some(reader)
}

/// The picture's size in pixels, read from its header alone. `None` when
/// it is not a picture this draws: another format, no size, or too large.
pub fn size(bytes: &[u8]) -> Option<(u32, u32)> {
    // A header that does not parse is not a picture; the file is still
    // listed, and saving it says nothing of what is in it.
    let (width, height) = reader(bytes)?.into_dimensions().ok()?;
    ((1..=MAX_SIDE).contains(&width) && (1..=MAX_SIDE).contains(&height)).then_some((width, height))
}

/// The picture, or the first frame of a moving one.
fn decode(bytes: &[u8]) -> Option<DynamicImage> {
    size(bytes)?;
    let mut reader = reader(bytes)?;
    let mut limits = Limits::default();
    (limits.max_image_width, limits.max_image_height, limits.max_alloc) =
        (Some(MAX_SIDE), Some(MAX_SIDE), Some(MAX_ALLOC));
    reader.limits(limits);
    // Bytes that do not decode leave the placeholder where it is.
    reader.decode().ok()
}

type Drawn = (usize, u16, u16);

/// Draws the reader's pictures over their placeholders, and keeps what it
/// made of each so a frame costs no decoding.
#[derive(Default)]
pub struct Painter {
    /// What the terminal can do, once it has been asked. `Some(None)` is a
    /// terminal with no image protocol.
    picker: Option<Option<Picker>>,
    of: Option<MessageId>,
    /// By the picture and the cells it was fitted to. `None` is a picture
    /// that could not be drawn, which is not tried again.
    drawn: HashMap<Drawn, Option<Protocol>>,
}

impl Painter {
    #[cfg(test)]
    pub(crate) fn with(picker: Picker) -> Self {
        Painter { picker: Some(Some(picker)), ..Painter::default() }
    }

    /// The terminal is asked what it can draw once, and only when there is
    /// first a picture to draw: one that never shows a picture is never
    /// asked. Blocks standing in for pixels are not a picture, so a
    /// terminal offering no more keeps the design's placeholder.
    fn picker(&mut self) -> Option<&Picker> {
        let asked = self.picker.get_or_insert_with(|| {
            // A terminal that does not answer has no protocol to use.
            Picker::from_query_stdio().ok()
        });
        asked.as_ref().filter(|picker| picker.protocol_type() != ProtocolType::Halfblocks)
    }

    /// Draws each picture of message `id` that is in sight into `buf`,
    /// over the cells the reader gave it. `bytes` hands over the file a
    /// picture is, by its place among the message's attachments.
    pub fn paint(
        &mut self,
        id: &MessageId,
        placed: &[Placed],
        mut bytes: impl FnMut(usize) -> Option<Vec<u8>>,
        buf: &mut Buffer,
    ) {
        if self.of.as_ref() != Some(id) {
            (self.of, self.drawn) = (Some(id.clone()), HashMap::new());
        }
        for at in placed {
            let (Ok(width), Ok(height)) = (u16::try_from(at.width), u16::try_from(at.height)) else { continue };
            let (Ok(x), Ok(y)) = (u16::try_from(at.col), u16::try_from(at.row)) else { continue };
            let key = (at.index, width, height);
            if !self.drawn.contains_key(&key) {
                let Some(file) = bytes(at.index) else { continue };
                let made = self.picker().cloned().and_then(|picker| {
                    // What the protocol cannot encode stays a placeholder.
                    picker.new_protocol(decode(&file)?, Size::new(width, height), Resize::Fit(None)).ok()
                });
                self.drawn.insert(key, made);
            }
            let Some(Some(protocol)) = self.drawn.get(&key) else { continue };
            let cells = Rect::new(buf.area.x + x, buf.area.y + y, width, height).intersection(buf.area);
            if cells.width < width || cells.height < height {
                continue;
            }
            // The picture keeps its shape inside its cells, so the rest of
            // the placeholder is cleared, not left around it.
            for position in cells.positions() {
                buf[position].reset();
            }
            Image::new(protocol).render(cells, buf);
        }
    }
}

#[cfg(test)]
mod tests;
