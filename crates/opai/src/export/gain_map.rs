//! Preparing a picture's gain map for a JPEG export: kept at its own resolution, shrunk only to a smaller written image,
//! and encoded as the gain-map JPEG `rust-sak` appends after the primary.

use ::image::DynamicImage;
use ::image::imageops::FilterType;
use rust_sak::image::{EncodeOptions, ImageError, ImageFormat, Metadata, encode_writer_with};

use crate::image::GainMap;

/// The lowest JPEG quality a gain map is encoded at. A gain map's small errors become visible brightness errors in the
/// HDR rendition, so the export's own quality is raised to this, for a few kilobytes.
pub(crate) const MIN_GAIN_MAP_QUALITY: u8 = 90;

/// The largest encoded gain map written: `rust-sak`'s own bound on what it appends.
const MAX_GAIN_MAP_LEN: usize = 64 * 1024 * 1024;

/// Why a gain map was not prepared. The export then writes an SDR JPEG and reports the HDR it could not carry.
#[derive(Debug, thiserror::Error)]
pub(crate) enum NotPrepared {
    /// The JPEG encoder refused it.
    #[error("it could not be encoded: {0}")]
    Encode(#[from] ImageError),
    /// Its own `hdrgm` XMP or ISO 21496-1 metadata was not written into it, so readers could not interpret it.
    #[error("its gain-map metadata could not be embedded")]
    Metadata,
    /// It encoded larger than a JPEG's gain map may be.
    #[error("it encoded to {0} bytes, over the {MAX_GAIN_MAP_LEN}-byte bound")]
    TooLarge(usize),
}

/// `gain_map` encoded for an export whose primary image is `written` pixels, at the export's JPEG `quality` or
/// [`MIN_GAIN_MAP_QUALITY`], whichever is higher: the complete gain-map JPEG, its own XMP and ISO 21496-1 metadata
/// inside it.
///
/// **Passed through where nothing changed it**: a gain map no crop reframed and the written image does not shrink is the
/// source's own gain-map JPEG ([`GainMap::source`]), byte for byte, so exporting an export does not lose a generation
/// of it. Only a reframed or shrunk one is encoded.
///
/// **Never enlarged.** It keeps its own resolution — the source's, as cropped — because ISO 21496-1 and Ultra HDR allow
/// a gain map smaller than the base and every viewer resamples it, and enlarging a smooth map adds bytes, not
/// information. Only where the written image is smaller than it on an axis is it shrunk, to the written image's size on
/// that axis, with [`FilterType::Triangle`]: bilinear, as Ultra HDR requires at least, and without the overshoot of
/// Lanczos or Catmull-Rom, which in a map of log-gains would put bright and dark halos along every edge.
pub(crate) fn prepare(gain_map: &GainMap, written: (u32, u32), quality: u8) -> Result<Vec<u8>, NotPrepared> {
    let pixels = fitted(gain_map.pixels(), written);
    // Neither reframed nor shrunk: the source's own gain-map JPEG, as it was, with nothing lost to a re-encode.
    if pixels.is_none()
        && let Some(source) = gain_map.source()
    {
        if source.len() > MAX_GAIN_MAP_LEN {
            return Err(NotPrepared::TooLarge(source.len()));
        }
        return Ok(source.to_vec());
    }
    let pixels = pixels.as_ref().unwrap_or(gain_map.pixels());

    let own = gain_map.metadata();
    let mut metadata = Metadata::default();
    metadata.xmp.clone_from(&own.xmp);
    metadata.iso_gain_map.clone_from(&own.iso_gain_map);

    let options = EncodeOptions::Jpeg { quality: quality.max(MIN_GAIN_MAP_QUALITY) };
    let mut encoded = Vec::new();
    let embedded = encode_writer_with(pixels, &mut encoded, ImageFormat::Jpeg, Some(options), &metadata)?;
    if (own.xmp.is_some() && !embedded.xmp) || (own.iso_gain_map.is_some() && !embedded.iso_gain_map) {
        return Err(NotPrepared::Metadata);
    }
    if encoded.len() > MAX_GAIN_MAP_LEN {
        return Err(NotPrepared::TooLarge(encoded.len()));
    }
    Ok(encoded)
}

/// `pixels` shrunk to fit inside `written` on each axis it overruns, or `None` where it already fits.
fn fitted(pixels: &DynamicImage, written: (u32, u32)) -> Option<DynamicImage> {
    let (width, height) = (pixels.width().min(written.0.max(1)), pixels.height().min(written.1.max(1)));
    ((width, height) != (pixels.width(), pixels.height()))
        .then(|| pixels.resize_exact(width, height, FilterType::Triangle))
}

#[cfg(test)]
mod tests {
    use ::image::metadata::Orientation;
    use rust_sak::image::{decode_bytes, metadata_bytes};

    use super::*;
    use crate::image::test_support::{HAWK_GAIN_MAP_ISO, HAWK_GAIN_MAP_XMP, gain_map_jpeg};

    fn gain_map(size: (u32, u32), base: (u32, u32)) -> GainMap {
        GainMap::read(&gain_map_jpeg(size, 3), base, Orientation::NoTransforms).expect("readable")
    }

    /// [`gain_map`] reframed to itself, as a crop to the whole frame leaves it: the same pixels, no source bytes, so it
    /// is encoded.
    fn encoded_anew(size: (u32, u32), base: (u32, u32)) -> GainMap {
        gain_map(size, base).reframed(|pixels, _| pixels.clone())
    }

    fn size_of(jpeg: &[u8]) -> (u32, u32) {
        let decoded = decode_bytes(jpeg).expect("decodable");
        (decoded.width(), decoded.height())
    }

    #[test]
    fn an_upscaled_pictures_gain_map_is_not_resampled() {
        let source = encoded_anew((32, 24), (32, 24));

        for written in [(32, 24), (64, 48), (128, 96)] {
            let encoded = prepare(&source, written, 90).expect("prepared");
            assert_eq!(size_of(&encoded), (32, 24), "written at {written:?}");
        }
        // A quarter-resolution one upscaled 4×, as the spec's 1000×750 / 250×188 example.
        let quarter = encoded_anew((10, 8), (40, 32));
        assert_eq!(size_of(&prepare(&quarter, (160, 128), 90).expect("prepared")), (10, 8));
    }

    #[test]
    fn a_written_image_smaller_than_the_gain_map_shrinks_it_to_fit() {
        let source = gain_map((32, 24), (32, 24));

        assert_eq!(size_of(&prepare(&source, (16, 12), 90).expect("prepared")), (16, 12));
        // Only the axis that overruns.
        assert_eq!(size_of(&prepare(&source, (40, 12), 90).expect("prepared")), (32, 12));
    }

    #[test]
    fn an_unchanged_gain_map_is_the_sources_own_bytes_at_any_quality_and_at_any_larger_size() {
        let bytes = gain_map_jpeg((32, 24), 3);
        let source = GainMap::read(&bytes, (32, 24), Orientation::NoTransforms).expect("readable");

        for (written, quality) in [((32, 24), 70), ((32, 24), 100), ((128, 96), 90)] {
            assert_eq!(prepare(&source, written, quality).expect("prepared"), bytes, "{written:?} at {quality}");
        }
        // Reframed or shrunk, it is encoded.
        assert_ne!(prepare(&encoded_anew((32, 24), (32, 24)), (32, 24), 90).expect("prepared"), bytes);
        assert_eq!(size_of(&prepare(&source, (16, 12), 90).expect("prepared")), (16, 12));
    }

    #[test]
    fn the_gain_map_keeps_its_own_metadata_and_is_never_encoded_below_quality_90() {
        let source = encoded_anew((32, 24), (32, 24));

        let at_70 = prepare(&source, (32, 24), 70).expect("prepared");
        let at_90 = prepare(&source, (32, 24), 90).expect("prepared");
        let at_95 = prepare(&source, (32, 24), 95).expect("prepared");

        assert_eq!(at_70, at_90, "70 is raised to the floor of 90");
        assert_ne!(at_95, at_90, "95 is kept");
        assert!(at_95.len() > at_90.len());
        let read = metadata_bytes(&at_70);
        assert_eq!(read.xmp.as_deref(), Some(HAWK_GAIN_MAP_XMP));
        assert_eq!(read.iso_gain_map.as_deref(), Some(HAWK_GAIN_MAP_ISO));
        assert_eq!((read.icc_profile, read.exif), (None, None), "a gain map carries no profile and no EXIF");
    }

    #[test]
    fn a_failure_is_answered_as_a_reason_not_an_error() {
        // An ISO 21496-1 payload too large for the one `APP2` it must fit in: `rust-sak` leaves it out, and a gain map
        // without its metadata is not one a reader can interpret.
        let mut metadata = Metadata::default();
        metadata.iso_gain_map = Some(vec![1; 70_000]);
        let refused = gain_map((8, 6), (8, 6)).with_metadata(metadata);

        let reason = prepare(&refused, (8, 6), 90).expect_err("its ISO payload does not fit");

        assert!(matches!(reason, NotPrepared::Metadata), "{reason:?}");
        assert_eq!(reason.to_string(), "its gain-map metadata could not be embedded");
    }
}
