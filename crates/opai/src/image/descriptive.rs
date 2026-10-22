//! What of a picture's metadata an encode may be handed while the EXIF, XMP and IPTC it carries are not yet rewritten.

use rust_sak::image::Metadata;

/// `metadata` without its descriptive blocks — EXIF, XMP and IPTC — and with everything else, the ICC colour profile
/// among it, kept.
///
/// **Why the blocks are withheld.** Written as the source held them, they would put false facts into the file: an
/// upscale or a crop changes the pixel dimensions they state, the thumbnail they embed shows the uncropped original,
/// a developed RAW's Orientation no longer applies, and their Software names whatever wrote the source. Until the
/// export rewrites those fields, [`export::write`](fn@crate::export::write) encodes through this, so an export carries
/// none of them rather than wrong ones.
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
    fn nothing_carried_is_nothing_kept() {
        assert_eq!(without_descriptive(&Metadata::default()), Metadata::default());
    }
}
