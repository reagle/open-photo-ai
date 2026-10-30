//! What of a picture's metadata an export writes, and how it is rewritten so that every field is true of the file.
//!
//! The policy of the `preserve-metadata` series (slice 1's design, D6): keep everything, change only what would
//! otherwise be false. Applied by [`export::write`](fn@super::write) and nowhere else — a preview rendition carries none
//! of this, and `image::save` embeds whatever it is handed.
//!
//! - **EXIF** is rewritten by [`exif`] — software, modification time and offset, pixel dimensions, orientation 1,
//!   no thumbnail — **without moving the bytes it keeps**, so a camera's MakerNote stays valid.
//! - **IPTC** is filtered by [`iptc`] to the IPTC-IIM resource and its digest.
//! - **XMP** is rewritten by [`xmp`] — a new identity derived from the source's, the history extended, the dates and
//!   the EXIF mirrors made current, Camera Raw properties dropped, and gain-map properties dropped unless the gain map is
//!   written, in which case they are made true of it — and, for a PNG or WebP export, given the IPTC Core values those
//!   formats cannot hold as IIM.
//! - The fields that state facts about the source's **pixels** — the subject's position, the focal-plane resolution —
//!   are moved with them by [`pixels`], in the EXIF and their XMP mirrors, through the crop, the turn and the upscale
//!   the picture's [`edits`](Picture::edits) record. What the camera measured is never rewritten.
//! - An edited picture loses Google's **depth and original-image** XMP (`GImage:`, `GFocus:`, and `GDepth:` unless only
//!   an upscale changed the pixels): the original would carry what an erase took out, and the depth map would outline
//!   it.
//! - The **ICC profile** is passed on as it is.
//! - The **ISO 21496-1 version marker** is kept only when the gain map is written; the gain-map image itself is
//!   [`export::write`](fn@super::write)'s to add.

pub(crate) mod exif;
pub(crate) mod iptc;
pub(crate) mod pixels;
pub(crate) mod xmp;

use chrono::{DateTime, FixedOffset};
use rust_sak::image::{ImageFormat, MAX_JPEG_EXIF_LEN, Metadata};

use crate::image::Picture;

/// The metadata to embed when `picture` is exported as `format` at `now`, the export's local time, followed by a
/// gain-map JPEG of `gain_map` bytes if one is written.
///
/// Never fails: a block that cannot be rewritten so as to be true is left out, with a warning naming the source.
pub(crate) fn for_export(
    picture: &Picture,
    format: ImageFormat,
    now: DateTime<FixedOffset>,
    gain_map: Option<usize>,
) -> Metadata {
    let ids = (format!("xmp.did:{}", uuid::Uuid::new_v4()), format!("xmp.iid:{}", uuid::Uuid::new_v4()));
    for_export_with(picture, format, now, ids, gain_map)
}

/// [`for_export`] with the new XMP document and instance IDs given, so a test can name them.
pub(crate) fn for_export_with(
    picture: &Picture,
    format: ImageFormat,
    now: DateTime<FixedOffset>,
    (document_id, instance_id): (String, String),
    gain_map: Option<usize>,
) -> Metadata {
    let carried = picture.metadata();
    let (width, height) = picture.dimensions();
    let raw = crate::image::is_raw(picture.path());
    let edits = picture.edits();
    let geometry = pixels::Geometry::of(carried.exif.as_deref(), edits, (width, height));

    let exif_context = exif::Context {
        source: picture.path(),
        width,
        height,
        now,
        // Only JPEG bounds the block, at one `APP1` segment; PNG and WebP chunks hold any size.
        limit: (format == ImageFormat::Jpeg).then_some(MAX_JPEG_EXIF_LEN),
    };

    // IIM reaches only a JPEG as IIM; PNG and WebP get its IPTC Core values through XMP instead.
    let iim = matches!(format, ImageFormat::Png | ImageFormat::WebP)
        .then(|| carried.iptc.as_deref().and_then(iptc::iim))
        .flatten();
    let xmp_context = xmp::Context {
        source: picture.path(),
        source_type: source_type(picture.path(), raw),
        output_type: media_type(format),
        width,
        height,
        now,
        document_id,
        instance_id,
        exif: carried.exif.as_deref(),
        iim: iim.as_ref(),
        gain_map,
        edits,
        geometry: geometry.as_ref(),
    };
    let described = carried.exif.is_some() || carried.iptc.is_some();

    let mut written = crate::image::without_descriptive(carried);
    written.exif =
        carried
            .exif
            .as_deref()
            .and_then(|block| exif::rewrite(block, &exif_context))
            .map(|block| match &geometry {
                Some(geometry) => pixels::rewrite_exif(block, geometry),
                None => block,
            });
    written.iptc = carried.iptc.as_deref().and_then(|block| iptc::filter(block, picture.path()));
    written.xmp = xmp::rewrite(carried.xmp.as_deref(), described, &xmp_context);
    if gain_map.is_some() {
        written.iso_gain_map.clone_from(&carried.iso_gain_map);
    }
    written
}

/// The media type XMP's history names a format by.
fn media_type(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Avif => "image/avif",
        ImageFormat::Bmp => "image/bmp",
        ImageFormat::Gif => "image/gif",
        ImageFormat::Heif => "image/heic",
        ImageFormat::Jpeg => "image/jpeg",
        ImageFormat::Png => "image/png",
        ImageFormat::Tiff => "image/tiff",
        ImageFormat::WebP => "image/webp",
    }
}

/// The source's media type, from its name: its format's, or `image/x-<extension>` for camera RAW as Adobe names them.
fn source_type(path: &std::path::Path, raw: bool) -> Option<String> {
    if raw {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        return Some(format!("image/x-{extension}"));
    }
    ImageFormat::from_path(path).map(|format| media_type(format).to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::image::test_support::{DISPLAY_P3, described};

    #[test]
    fn the_exif_is_rewritten_the_iptc_filtered_the_xmp_rewritten_and_the_profile_kept() {
        let pixels = Arc::new(::image::DynamicImage::new_rgb8(64, 48));
        let picture = Picture::new("/pictures/described.jpg", pixels, "0123456789abcdef").with_metadata(described());
        let now = DateTime::parse_from_rfc3339("2026-10-22T09:30:00-04:00").expect("a time");

        let written = for_export(&picture, ImageFormat::Jpeg, now, None);

        let exif = written.exif.as_deref().expect("EXIF");
        assert_eq!(
            rust_sak::image::exif_field(exif, rust_sak::image::ExifIfd::Primary, 0x0132),
            Some(rust_sak::image::ExifValue::Ascii("2026:10:22 09:30:00".into()))
        );
        assert!(written.iptc.is_some());
        let xmp = String::from_utf8(written.xmp.expect("XMP")).expect("UTF-8");
        assert!(xmp.contains("<xmp:ModifyDate>2026-10-22T09:30:00-04:00</xmp:ModifyDate>"), "{xmp}");
        assert!(xmp.contains("<rdf:li>hawk</rdf:li>"), "{xmp}");
        assert_eq!(written.icc_profile.as_deref(), Some(DISPLAY_P3));
    }

    #[test]
    fn a_cropped_export_moves_the_subject_with_the_pixels_and_keeps_what_the_camera_measured() {
        use crate::image::Affine;
        use pixels::tests::{block, read};

        let source = Picture::new("/pictures/framed.jpg", Arc::new(::image::DynamicImage::new_rgb8(400, 300)), "00")
            .with_metadata({
                let mut metadata = Metadata::default();
                metadata.exif = Some(block(1));
                metadata
            });
        // A tenth of the 4000x3000 the EXIF describes: the subject at (3000, 1000) is decoded at (300, 100).
        let cropped = source
            .derive(Arc::new(::image::DynamicImage::new_rgb8(150, 150)), "01")
            .framed((400, 300), Affine::translation(-200.0, -50.0));
        let now = DateTime::parse_from_rfc3339("2026-10-30T09:30:00-04:00").expect("a time");

        let written = for_export(&cropped, ImageFormat::Jpeg, now, None);
        let exif = written.exif.as_deref().expect("EXIF");

        assert_eq!(read(exif, pixels::SUBJECT_LOCATION), Some(vec![100, 50]));
        assert_eq!(read(exif, pixels::SUBJECT_AREA), Some(vec![100, 50, 40, 20]));
        // The focal plane is measured against PixelXDimension, now 150 for 4000: a tenth the pixels per unit.
        assert_eq!(read(exif, pixels::FOCAL_PLANE_X_RESOLUTION), Some(vec![400, 1]));
    }

    #[test]
    fn media_types_name_formats_and_raw_extensions() {
        assert_eq!(source_type(std::path::Path::new("/a/b.JPG"), false).as_deref(), Some("image/jpeg"));
        assert_eq!(source_type(std::path::Path::new("/a/b.ARW"), true).as_deref(), Some("image/x-arw"));
        assert_eq!(source_type(std::path::Path::new("/a/b"), false), None);
        assert_eq!(media_type(ImageFormat::WebP), "image/webp");
    }
}
