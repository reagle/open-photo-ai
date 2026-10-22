//! What of a picture's metadata an export writes, and how it is rewritten so that every field is true of the file.
//!
//! The policy of the `preserve-metadata` series (slice 1's design, D6): keep everything, change only what would
//! otherwise be false. Applied by [`export::write`](fn@super::write) and nowhere else — a preview rendition carries none
//! of this, and `image::save` embeds whatever it is handed.
//!
//! - **EXIF** is rewritten by [`exif`] — software, modification time and offset, pixel dimensions, orientation for RAW,
//!   no thumbnail — **without moving the bytes it keeps**, so a camera's MakerNote stays valid.
//! - **IPTC** is filtered by [`iptc`] to the IPTC-IIM resource and its digest.
//! - **XMP** is withheld until it, too, is rewritten (the series' slice 3).
//! - The **ICC profile** is passed on as it is.

pub(crate) mod exif;
pub(crate) mod iptc;

use chrono::{DateTime, FixedOffset};
use rust_sak::image::{ImageFormat, MAX_JPEG_EXIF_LEN, Metadata};

use crate::image::Picture;

/// The metadata to embed when `picture` is exported as `format` at `now`, the export's local time.
///
/// Never fails: a block that cannot be rewritten so as to be true is left out, with a warning naming the source.
pub(crate) fn for_export(picture: &Picture, format: ImageFormat, now: DateTime<FixedOffset>) -> Metadata {
    let carried = picture.metadata();
    let (width, height) = picture.dimensions();
    let context = exif::Context {
        source: picture.path(),
        width,
        height,
        raw: crate::image::is_raw(picture.path()),
        now,
        // Only JPEG bounds the block, at one `APP1` segment; PNG and WebP chunks hold any size.
        limit: (format == ImageFormat::Jpeg).then_some(MAX_JPEG_EXIF_LEN),
    };

    let mut written = crate::image::without_descriptive(carried);
    written.exif = carried.exif.as_deref().and_then(|block| exif::rewrite(block, &context));
    written.iptc = carried.iptc.as_deref().and_then(|block| iptc::filter(block, picture.path()));
    written
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::image::test_support::{DISPLAY_P3, described};

    #[test]
    fn the_exif_is_rewritten_the_iptc_filtered_the_xmp_withheld_and_the_profile_kept() {
        let pixels = Arc::new(::image::DynamicImage::new_rgb8(64, 48));
        let picture = Picture::new("/pictures/described.jpg", pixels, "0123456789abcdef").with_metadata(described());
        let now = DateTime::parse_from_rfc3339("2026-10-22T09:30:00-04:00").expect("a time");

        let written = for_export(&picture, ImageFormat::Jpeg, now);

        let exif = written.exif.as_deref().expect("EXIF");
        assert_eq!(
            rust_sak::image::exif_field(exif, rust_sak::image::ExifIfd::Primary, 0x0132),
            Some(rust_sak::image::ExifValue::Ascii("2026:10:22 09:30:00".into()))
        );
        assert!(written.iptc.is_some());
        assert_eq!(written.xmp, None);
        assert_eq!(written.icc_profile.as_deref(), Some(DISPLAY_P3));
    }
}
