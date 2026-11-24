use std::io::Cursor;

use image::{DynamicImage, ImageFormat};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui_image::picker::{Picker, ProtocolType};
use tuimex_core::{AccountId, MessageId};
use tuimex_ui::reader::Placed;

use super::{MAX_SIDE, Painter, decode, size};

fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(width, height).write_to(&mut out, format).unwrap();
    out.into_inner()
}

const FOUR: [ImageFormat; 4] = [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Gif, ImageFormat::WebP];

#[test]
fn the_four_formats_are_sized_from_their_headers_and_decoded() {
    for format in FOUR {
        let bytes = encoded(40, 30, format);
        assert_eq!(size(&bytes), Some((40, 30)), "{format:?}");
        let picture = decode(&bytes).unwrap();
        assert_eq!((picture.width(), picture.height()), (40, 30), "{format:?}");
    }
}

/// What the file is called or claims to be is not asked, and nothing but
/// the four formats is a picture: PLAN §1 links no other decoder.
#[test]
fn another_format_is_not_a_picture() {
    let bmp = b"BM\x46\0\0\0\0\0\0\0\x36\0\0\0\x28\0\0\0\x02\0\0\0\x02\0\0\0\x01\0\x18\0";
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>steal()</script></svg>";
    for bytes in [&bmp[..], &svg[..], b"", b"GIF", b"not a picture at all"] {
        assert_eq!(size(bytes), None);
        assert!(decode(bytes).is_none());
    }
}

/// A PNG's length-prefixed chunk, with the checksum a decoder checks.
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut crc = u32::MAX;
    for byte in kind.iter().chain(data) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    let mut out = (data.len() as u32).to_be_bytes().to_vec();
    out.extend(kind.iter().chain(data));
    out.extend((!crc).to_be_bytes());
    out
}

/// A header is all it takes to claim a size, so the claim is refused
/// before anything is allocated for it.
#[test]
fn a_picture_claiming_more_than_the_cap_is_never_decoded() {
    let png = |width: u32, height: u32| {
        let mut header = [width.to_be_bytes(), height.to_be_bytes()].concat();
        header.extend([8, 2, 0, 0, 0]);
        [b"\x89PNG\r\n\x1a\n".to_vec(), chunk(b"IHDR", &header), chunk(b"IDAT", &[]), chunk(b"IEND", &[])].concat()
    };
    assert_eq!(size(&png(MAX_SIDE, 1)), Some((MAX_SIDE, 1)), "the header itself is well formed");
    for (width, height) in [(MAX_SIDE + 1, 1), (1, MAX_SIDE + 1), (60_000, 60_000), (u32::MAX, u32::MAX), (0, 9)] {
        assert_eq!(size(&png(width, height)), None, "{width}x{height}");
        assert!(decode(&png(width, height)).is_none(), "{width}x{height}");
    }
    assert!(decode(&png(MAX_SIDE, MAX_SIDE)).is_none(), "a size within the cap with no pixels behind it is an error");
}

/// The bytes are a stranger's: cut short or changed anywhere, a picture
/// decodes or it does not, and nothing panics.
#[test]
fn damaged_bytes_decode_or_fail_and_never_panic() {
    for format in FOUR {
        let whole = encoded(24, 16, format);
        for cut in 0..whole.len() {
            decode(&whole[..cut]);
        }
        for at in 0..whole.len() {
            for flip in [0x01, 0x80, 0xff] {
                let mut bytes = whole.clone();
                bytes[at] ^= flip;
                if let Some(picture) = decode(&bytes) {
                    assert!(picture.width() <= MAX_SIDE && picture.height() <= MAX_SIDE, "{format:?} at {at}");
                }
            }
        }
    }
}

fn id(key: &str) -> MessageId {
    MessageId { account: AccountId("test".into()), key: key.into() }
}

fn placeholder() -> Buffer {
    Buffer::filled(Rect::new(0, 0, 60, 20), ratatui::buffer::Cell::new("░"))
}

fn shades(buf: &Buffer, cells: Rect) -> usize {
    cells.positions().filter(|at| buf[*at].symbol() == "░").count()
}

const AT: Placed = Placed { index: 0, row: 3, col: 4, width: 48, height: 12 };
const CELLS: Rect = Rect::new(4, 3, 48, 12);

/// The terminal's own protocol carries the picture; no test has a terminal
/// to see it on, so what is proven is that the picture's cells are handed
/// to the protocol and the cells around them are left alone.
#[test]
fn a_terminal_with_a_protocol_gets_the_picture_over_its_placeholder() {
    for protocol in [ProtocolType::Kitty, ProtocolType::Sixel, ProtocolType::Iterm2] {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(protocol);
        let (mut painter, mut buf, mut asked) = (Painter::with(picker), placeholder(), 0);
        let png = encoded(320, 200, ImageFormat::Png);
        let mut frame = |buf: &mut Buffer| {
            let bytes = |_| {
                asked += 1;
                Some(png.clone())
            };
            painter.paint(&id("one"), &[AT], bytes, buf);
        };
        frame(&mut buf);
        // Kitty sends the picture with the first frame it is in, and names
        // it in the frames after.
        assert!(CELLS.positions().any(|at| buf[at].symbol().contains('\x1b')), "{protocol:?} wrote no sequence");
        frame(&mut buf);
        frame(&mut buf);
        assert_eq!(shades(&buf, CELLS), 0, "{protocol:?}");
        assert_eq!(shades(&buf, buf.area), 60 * 20 - 48 * 12, "{protocol:?}: nothing outside its cells is touched");
        assert_eq!(asked, 1, "{protocol:?}: the bytes are asked for once, not every frame");
    }
}

/// The design's placeholder is what a terminal without a protocol shows,
/// and what stays when the bytes are missing, are not a picture, or would
/// not fit on the screen.
#[test]
fn the_placeholder_stays_where_no_picture_can_be_drawn() {
    let kitty = || {
        let mut picker = Picker::halfblocks();
        picker.set_protocol_type(ProtocolType::Kitty);
        Painter::with(picker)
    };
    let png = encoded(320, 200, ImageFormat::Png);
    let untouched = |mut painter: Painter, placed: Placed, bytes: Option<Vec<u8>>| {
        let mut buf = placeholder();
        painter.paint(&id("one"), &[placed], |_| bytes.clone(), &mut buf);
        buf == placeholder()
    };
    assert!(untouched(Painter::with(Picker::halfblocks()), AT, Some(png.clone())), "blocks are not a picture");
    assert!(untouched(kitty(), AT, None), "no bytes");
    assert!(untouched(kitty(), AT, Some(b"<svg/>".to_vec())), "not a picture");
    assert!(untouched(kitty(), Placed { row: 15, ..AT }, Some(png.clone())), "off the bottom of the screen");
    assert!(untouched(kitty(), Placed { width: usize::MAX, ..AT }, Some(png)), "a size no screen has");
}

#[test]
fn another_message_s_pictures_are_not_drawn_for_this_one() {
    let mut picker = Picker::halfblocks();
    picker.set_protocol_type(ProtocolType::Kitty);
    let (mut painter, mut buf) = (Painter::with(picker), placeholder());
    painter.paint(&id("one"), &[AT], |_| Some(encoded(320, 200, ImageFormat::Png)), &mut buf);
    let mut next = placeholder();
    painter.paint(&id("two"), &[AT], |_| None, &mut next);
    assert_eq!(next, placeholder(), "the first message's picture was kept for the second");
}
