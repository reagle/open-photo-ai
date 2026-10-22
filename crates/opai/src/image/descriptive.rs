//! What of a picture's metadata an encode may be handed while the EXIF, XMP and IPTC it carries are not yet rewritten.

use std::path::Path;

use rust_sak::image::{Embedded, ImageFormat, Metadata};

/// `metadata` without its descriptive blocks — EXIF, XMP and IPTC — and with everything else, the ICC colour profile
/// among it, kept.
///
/// **Why the blocks are withheld.** Written as the source held them, they would put false facts into the file: an
/// upscale or a crop changes the pixel dimensions they state, the thumbnail they embed shows the uncropped original,
/// a developed RAW's Orientation no longer applies, and their Software names whatever wrote the source.
/// [`export::write`](fn@crate::export::write) starts from this and puts back what its policy has rewritten to be true —
/// the EXIF and IPTC today, the XMP once it is rewritten too.
///
/// **Where it stays for good.** A preview rendition is encoded through it permanently: a browser applies an EXIF
/// Orientation to an `<img>`, which would turn the preview away from the frame the picture's pixels — and any crop
/// measured on them — are in, and a preview has no use for the rest.
pub fn without_descriptive(metadata: &Metadata) -> Metadata {
    let mut kept = metadata.clone();
    kept.exif = None;
    kept.xmp = None;
    kept.iptc = None;
    kept
}

/// Records, once, which of the EXIF, XMP and IPTC handed to an encode did not reach the file because `format` has no
/// place for them (BMP and GIF for all three; TIFF, AVIF and HEIF until the series reaches them; IPTC in PNG and WebP).
/// A warning rather than a refusal, as for the colour profile, and silent when everything given was written.
pub(super) fn warn_if_dropped(metadata: &Metadata, embedded: Embedded, format: ImageFormat, path: Option<&Path>) {
    let dropped: Vec<&str> = [
        ("exif", metadata.exif.is_some() && !embedded.exif),
        ("xmp", metadata.xmp.is_some() && !embedded.xmp),
        ("iptc", metadata.iptc.is_some() && !embedded.iptc),
    ]
    .into_iter()
    .filter_map(|(name, dropped)| dropped.then_some(name))
    .collect();
    if dropped.is_empty() {
        return;
    }

    let op = if path.is_some() { "save" } else { "encode" };
    let path = path.map(|path| path.display().to_string());
    tracing::warn!(op, path, format = format.name(), blocks = dropped.join(", "), "metadata not embedded");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::test_support::{DISPLAY_P3, described};

    #[test]
    fn the_descriptive_blocks_go_and_the_colour_profile_stays() {
        let carried = described();
        assert!(carried.exif.is_some() && carried.xmp.is_some() && carried.iptc.is_some());

        let kept = without_descriptive(&carried);

        assert_eq!((kept.exif, kept.xmp, kept.iptc), (None, None, None));
        assert_eq!(kept.icc_profile.as_deref(), Some(DISPLAY_P3));
    }

    #[test]
    fn a_save_that_drops_blocks_names_them_and_one_that_drops_none_is_silent() {
        use crate::image::save_blocking;
        use crate::image::test_support::sample_image;
        use crate::logging::{field, records, records_of_blocking};

        let dir = tempfile::tempdir().expect("a temporary directory");
        for (format, dropped) in [
            (ImageFormat::Bmp, Some("exif, xmp, iptc")),
            (ImageFormat::Png, Some("iptc")),
            (ImageFormat::Jpeg, None),
        ] {
            let path = dir.path().join(format!("out.{}", format.extension()));
            let (log, saved) =
                records_of_blocking("info", || save_blocking(&sample_image(), &described(), &path, format, None));
            saved.expect("written all the same");

            let found = records(&log, "metadata not embedded");
            match dropped {
                Some(blocks) => {
                    let [record] = found[..] else { panic!("{format:?}: expected one record:\n{log}") };
                    assert_eq!(field(record, "blocks"), Some(blocks), "{record}");
                    assert_eq!(field(record, "format"), Some(format.name()), "{record}");
                }
                None => assert!(found.is_empty(), "{format:?}:\n{log}"),
            }
        }
    }

    #[test]
    fn nothing_carried_is_nothing_kept() {
        assert_eq!(without_descriptive(&Metadata::default()), Metadata::default());
    }
}
