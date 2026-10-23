//! The EXIF an export writes: the source's, with the fields that would be false of the exported file rewritten.
//!
//! | Field | IFD | What it becomes |
//! |---|---|---|
//! | Software `0x0131` | IFD0 | `Open Photo AI <version> (<YYYY-MM-DD>)`, the export's local date |
//! | DateTime `0x0132` | IFD0 | the export's local time |
//! | OffsetTime `0x9010` | Exif | the export's UTC offset, where the source records one |
//! | SubSecTime `0x9290` | Exif | the export time's milliseconds, where the source records them (they belong to DateTime) |
//! | PixelXDimension `0xA002`, PixelYDimension `0xA003` | Exif | the exported size (added where an Exif IFD exists) |
//! | ImageWidth `0x0100`, ImageLength `0x0101` | IFD0 | the exported size, where the source has them |
//! | Orientation `0x0112` | IFD0 | kept; 1 for camera RAW, whose decoder already turned the pixels |
//! | IFD1 and its thumbnail | — | removed: it shows the unenhanced, uncropped original |
//!
//! Everything else — capture dates and offsets, camera, lens, exposure, GPS, the MakerNote — is kept byte for byte.
//!
//! **OffsetTime is replaced, never added.** It exists only from EXIF 2.31 on, and a source without it is usually an
//! older EXIF, where an added one is itself an error a validator reports; its DateTime then stays offset-less, as the
//! source's own dates are.

use std::path::Path;

use chrono::{DateTime, FixedOffset};
use rust_sak::image::{ExifEdit, ExifError, ExifIfd, ExifValue, exif_field, patch_exif, rebuild_exif};

const SOFTWARE: u16 = 0x0131;
const DATE_TIME: u16 = 0x0132;
const ORIENTATION: u16 = 0x0112;
const IMAGE_WIDTH: u16 = 0x0100;
const IMAGE_LENGTH: u16 = 0x0101;
const MAKE: u16 = 0x010F;
const OFFSET_TIME: u16 = 0x9010;
const SUB_SEC_TIME: u16 = 0x9290;
const PIXEL_X_DIMENSION: u16 = 0xA002;
const PIXEL_Y_DIMENSION: u16 = 0xA003;

/// What the rewrite needs to know about the export.
pub(crate) struct Context<'a> {
    /// The source photograph, for the warnings.
    pub(crate) source: &'a Path,
    /// The exported image's size.
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// Whether the source is camera RAW, whose decoder applied the orientation.
    pub(crate) raw: bool,
    /// The export's local time and UTC offset.
    pub(crate) now: DateTime<FixedOffset>,
    /// The most the target format holds, if it is bounded (JPEG).
    pub(crate) limit: Option<usize>,
}

/// The Software value an export records: the application, its version and the export's date.
pub(crate) fn software(now: DateTime<FixedOffset>) -> String {
    format!("Open Photo AI {} ({})", env!("CARGO_PKG_VERSION"), now.format("%Y-%m-%d"))
}

/// The edits of the module's table, for `context`.
pub(crate) fn edits(context: &Context<'_>) -> Vec<ExifEdit> {
    let dimension = |value: u32| u16::try_from(value).map_or(ExifValue::Long(value), ExifValue::Short);
    let mut edits = vec![
        ExifEdit::DropThumbnail,
        ExifEdit::Set { ifd: ExifIfd::Primary, tag: SOFTWARE, value: ExifValue::Ascii(software(context.now)) },
        ExifEdit::Set {
            ifd: ExifIfd::Primary,
            tag: DATE_TIME,
            value: ExifValue::Ascii(context.now.format("%Y:%m:%d %H:%M:%S").to_string()),
        },
        // DateTime's fraction of a second: kept in step with it, never left from the source's.
        ExifEdit::Replace {
            ifd: ExifIfd::Exif,
            tag: SUB_SEC_TIME,
            value: ExifValue::Ascii(context.now.format("%3f").to_string()),
        },
        ExifEdit::Replace {
            ifd: ExifIfd::Exif,
            tag: OFFSET_TIME,
            value: ExifValue::Ascii(context.now.format("%:z").to_string()),
        },
        ExifEdit::Set { ifd: ExifIfd::Exif, tag: PIXEL_X_DIMENSION, value: dimension(context.width) },
        ExifEdit::Set { ifd: ExifIfd::Exif, tag: PIXEL_Y_DIMENSION, value: dimension(context.height) },
        ExifEdit::Replace { ifd: ExifIfd::Primary, tag: IMAGE_WIDTH, value: dimension(context.width) },
        ExifEdit::Replace { ifd: ExifIfd::Primary, tag: IMAGE_LENGTH, value: dimension(context.height) },
    ];
    if context.raw {
        edits.push(ExifEdit::Replace { ifd: ExifIfd::Primary, tag: ORIENTATION, value: ExifValue::Short(1) });
    }
    edits
}

/// `exif` rewritten for `context`, or `None` when it cannot be made true and is left out.
///
/// Patched, so that nothing kept moves and a MakerNote stays valid. Only when the patched block is too large for the
/// target, or its thumbnail cannot be removed in place without leaving its bytes behind, is it rebuilt instead — compact, without the MakerNote — and a MakerNote lost that way is logged with the
/// source, the camera maker and the reason. A block whose structure does not parse is left out and logged: no rewrite
/// of it can be trusted to be true, and copying it unchanged is what this module exists to prevent.
pub(crate) fn rewrite(exif: &[u8], context: &Context<'_>) -> Option<Vec<u8>> {
    let edits = edits(context);
    let source = context.source.display().to_string();

    let reason = match patch_exif(exif, &edits) {
        Ok(patched) => match context.limit.filter(|&limit| patched.len() > limit) {
            None => return Some(patched),
            Some(_) => "the EXIF would not fit the format's limit",
        },
        // The thumbnail could not be removed in place; the rebuild never carries one.
        Err(ExifError::ThumbnailUnremovable) => "the thumbnail could not be removed in place",
        Err(error) => {
            left_out(&source, error);
            return None;
        }
    };

    match rebuild_exif(exif, &edits) {
        Ok(rebuilt) => {
            if rebuilt.maker_note_dropped {
                let maker = match exif_field(exif, ExifIfd::Primary, MAKE) {
                    Some(ExifValue::Ascii(maker)) => maker,
                    _ => String::new(),
                };
                tracing::warn!(op = "export", path = source, maker, reason, "MakerNote dropped");
            }
            Some(rebuilt.exif)
        }
        Err(error) => {
            left_out(&source, error);
            None
        }
    }
}

fn left_out(source: &str, error: ExifError) {
    tracing::warn!(op = "export", path = source, %error, "EXIF left out: it could not be rewritten");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logging::{field, records, records_of_blocking};

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-22T09:30:05-04:00").expect("a time")
    }

    fn context(width: u32, height: u32, raw: bool, limit: Option<usize>) -> Context<'static> {
        Context { source: Path::new("/pictures/source.jpg"), width, height, raw, now: now(), limit }
    }

    fn ascii(text: &str) -> Option<ExifValue> {
        Some(ExifValue::Ascii(text.to_owned()))
    }

    /// A little-endian block with IFD0 (Make, Orientation 6, ImageWidth, Exif pointer, IFD1 next), an Exif IFD
    /// (DateTimeOriginal, OffsetTime, PixelX/YDimension as SHORT, a 16-byte MakerNote) and IFD1 with a thumbnail.
    fn source(offset_time: bool) -> Vec<u8> {
        let mut b = Vec::new();
        let u16b = |v: u16| v.to_le_bytes();
        let u32b = |v: u32| v.to_le_bytes();
        let entry = |b: &mut Vec<u8>, tag: u16, kind: u16, count: u32, raw: [u8; 4]| {
            b.extend(u16b(tag));
            b.extend(u16b(kind));
            b.extend(u32b(count));
            b.extend(raw);
        };
        let short = |v: u16| {
            let mut raw = [0; 4];
            raw[..2].copy_from_slice(&u16b(v));
            raw
        };
        b.extend(b"II\x2a\0");
        b.extend(u32b(8));
        // IFD0 at 8: 4 entries -> ends at 8 + 54 = 62. Make "Cam\0" inline.
        b.extend(u16b(4));
        entry(&mut b, IMAGE_WIDTH, 3, 1, short(4000));
        entry(&mut b, MAKE, 2, 4, *b"Cam\0");
        entry(&mut b, ORIENTATION, 3, 1, short(6));
        entry(&mut b, 0x8769, 4, 1, u32b(62));
        // Exif IFD at 62, then its values: DateTimeOriginal (20), the MakerNote (16) and, if any, OffsetTime (8).
        let entries: u16 = if offset_time { 5 } else { 4 };
        let values = 62 + 2 + u32::from(entries) * 12 + 4;
        let ifd1_at = values + 20 + 16 + if offset_time { 8 } else { 0 };
        b.extend(u32b(ifd1_at));
        b.extend(u16b(entries));
        entry(&mut b, 0x9003, 2, 20, u32b(values));
        if offset_time {
            entry(&mut b, OFFSET_TIME, 2, 7, u32b(values + 36));
        }
        entry(&mut b, 0x927C, 7, 16, u32b(values + 20));
        entry(&mut b, PIXEL_X_DIMENSION, 3, 1, short(4000));
        entry(&mut b, PIXEL_Y_DIMENSION, 3, 1, short(3000));
        b.extend(u32b(0));
        b.extend(b"2026:10:18 16:03:50\0");
        b.extend(b"MAKERNOTE-BYTES!");
        if offset_time {
            b.extend(b"+02:00\0\0");
        }
        assert_eq!(b.len(), ifd1_at as usize);
        // IFD1: thumbnail offset and length; the thumbnail last.
        let thumbnail_at = ifd1_at + 2 + 2 * 12 + 4;
        b.extend(u16b(2));
        entry(&mut b, 0x0201, 4, 1, u32b(thumbnail_at));
        entry(&mut b, 0x0202, 4, 1, u32b(64));
        b.extend(u32b(0));
        b.extend([0xD8; 64]);
        b
    }

    #[test]
    fn every_field_of_the_table_is_rewritten_and_the_rest_kept() {
        let exif = source(false);
        let rewritten = rewrite(&exif, &context(800, 600, false, None)).expect("rewritten");

        let field = |ifd, tag| exif_field(&rewritten, ifd, tag);
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(field(ExifIfd::Primary, SOFTWARE), ascii(&format!("Open Photo AI {version} (2026-10-22)")));
        assert_eq!(field(ExifIfd::Primary, DATE_TIME), ascii("2026:10:22 09:30:05"));
        assert_eq!(field(ExifIfd::Exif, PIXEL_X_DIMENSION), Some(ExifValue::Short(800)));
        assert_eq!(field(ExifIfd::Exif, PIXEL_Y_DIMENSION), Some(ExifValue::Short(600)));
        assert_eq!(field(ExifIfd::Primary, IMAGE_WIDTH), Some(ExifValue::Short(800)));
        // ImageLength was not there, so it is not added.
        assert_eq!(field(ExifIfd::Primary, IMAGE_LENGTH), None);
        // Nor is OffsetTime, which this older-style block does not record.
        assert_eq!(field(ExifIfd::Exif, OFFSET_TIME), None);
        // Kept.
        assert_eq!(field(ExifIfd::Primary, ORIENTATION), Some(ExifValue::Short(6)));
        assert_eq!(field(ExifIfd::Primary, MAKE), ascii("Cam"));
        assert_eq!(field(ExifIfd::Exif, 0x9003), ascii("2026:10:18 16:03:50"));
        let maker_note = rewritten.windows(16).position(|window| window == b"MAKERNOTE-BYTES!");
        let original = exif.windows(16).position(|window| window == b"MAKERNOTE-BYTES!");
        assert_eq!(maker_note, original, "the MakerNote moved");
        // The thumbnail is gone.
        assert!(!rewritten.windows(8).any(|window| window == [0xD8; 8]), "the thumbnail is still there");
    }

    #[test]
    fn an_offset_the_source_records_is_replaced_with_the_exports() {
        let exif = source(true);
        let rewritten = rewrite(&exif, &context(800, 600, false, None)).expect("rewritten");
        assert_eq!(exif_field(&rewritten, ExifIfd::Exif, OFFSET_TIME), ascii("-04:00"));
    }

    #[test]
    fn a_dimension_too_large_for_a_short_is_written_as_a_long() {
        let rewritten = rewrite(&source(false), &context(70_000, 600, false, None)).expect("rewritten");
        assert_eq!(exif_field(&rewritten, ExifIfd::Exif, PIXEL_X_DIMENSION), Some(ExifValue::Long(70_000)));
        assert_eq!(exif_field(&rewritten, ExifIfd::Primary, IMAGE_WIDTH), Some(ExifValue::Long(70_000)));
    }

    #[test]
    fn a_raw_sources_orientation_becomes_one_and_a_native_ones_is_kept() {
        let native = rewrite(&source(false), &context(8, 8, false, None)).expect("rewritten");
        let raw = rewrite(&source(false), &context(8, 8, true, None)).expect("rewritten");
        assert_eq!(exif_field(&native, ExifIfd::Primary, ORIENTATION), Some(ExifValue::Short(6)));
        assert_eq!(exif_field(&raw, ExifIfd::Primary, ORIENTATION), Some(ExifValue::Short(1)));
    }

    #[test]
    fn over_the_limit_it_is_rebuilt_and_the_dropped_maker_note_is_logged() {
        let exif = source(false);
        let (log, rewritten) = records_of_blocking("info", || rewrite(&exif, &context(8, 8, false, Some(100))));
        let rewritten = rewritten.expect("rebuilt");

        assert!(!rewritten.windows(9).any(|window| window == b"MAKERNOTE"), "the MakerNote came along");
        assert_eq!(exif_field(&rewritten, ExifIfd::Primary, MAKE), ascii("Cam"));
        assert_eq!(exif_field(&rewritten, ExifIfd::Exif, PIXEL_X_DIMENSION), Some(ExifValue::Short(8)));
        let [record] = records(&log, "MakerNote dropped")[..] else {
            panic!("expected one record:\n{log}")
        };
        assert_eq!(field(record, "maker"), Some("Cam"), "{record}");
        assert_eq!(field(record, "path"), Some("/pictures/source.jpg"), "{record}");
        assert!(record.contains("level=WARN"), "{record}");
    }

    #[test]
    fn subsecond_time_is_kept_in_step_with_date_time() {
        // An Exif IFD holding SubSecTime "666" beside the DateTime the export rewrites.
        let mut exif = source(false);
        let edits = [ExifEdit::Set { ifd: ExifIfd::Exif, tag: SUB_SEC_TIME, value: ExifValue::Ascii("666".into()) }];
        exif = patch_exif(&exif, &edits).expect("patched");
        let now = DateTime::parse_from_rfc3339("2026-10-22T09:30:05.25-04:00").expect("a time");
        let context = Context { now, ..context(8, 8, false, None) };

        let rewritten = rewrite(&exif, &context).expect("rewritten");
        assert_eq!(exif_field(&rewritten, ExifIfd::Exif, SUB_SEC_TIME), ascii("250"));
        // And none is added where the source recorded none.
        let plain = rewrite(&source(false), &context).expect("rewritten");
        assert_eq!(exif_field(&plain, ExifIfd::Exif, SUB_SEC_TIME), None);
    }

    #[test]
    fn a_thumbnail_that_cannot_be_removed_in_place_is_rebuilt_away_and_logged() {
        // IFD1's JPEG offset now points past the block: the patch cannot prove it removed it.
        let mut exif = source(false);
        let ifd1 = exif.len() - 64 - (2 + 2 * 12 + 4);
        exif[ifd1 + 2 + 8..ifd1 + 2 + 12].copy_from_slice(&9_999_u32.to_le_bytes());
        let (log, rewritten) = records_of_blocking("info", || rewrite(&exif, &context(8, 8, false, None)));
        let rewritten = rewritten.expect("rebuilt");

        assert!(!rewritten.windows(8).any(|window| window == [0xD8; 8]), "the thumbnail survived");
        assert_eq!(exif_field(&rewritten, ExifIfd::Primary, MAKE), ascii("Cam"));
        let [record] = records(&log, "MakerNote dropped")[..] else {
            panic!("expected one record:\n{log}")
        };
        assert_eq!(field(record, "reason"), Some("the thumbnail could not be removed in place"), "{record}");
    }

    #[test]
    fn a_block_that_does_not_parse_is_left_out_and_logged() {
        let (log, rewritten) =
            records_of_blocking("info", || rewrite(b"MM\0\x2a\0\0\0\x08", &context(8, 8, false, None)));
        assert_eq!(rewritten, None);
        assert_eq!(records(&log, "EXIF left out: it could not be rewritten").len(), 1, "{log}");
    }
}
