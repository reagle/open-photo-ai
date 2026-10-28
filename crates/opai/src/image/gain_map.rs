//! A JPEG's HDR gain map, carried beside a picture's pixels so that an export can write an Ultra HDR JPEG again.

use std::sync::Arc;

use ::image::metadata::Orientation;
use ::image::{ColorType, DynamicImage};
use rust_sak::image::{ImageError, ImageFormat, Metadata, decode_bytes, metadata_bytes, probe_bytes};

/// The gain-map namespace, whose declaration in a gain-map image's XMP says how to apply it.
const HDRGM: &[u8] = b"http://ns.adobe.com/hdr-gain-map/1.0/";

/// How much smaller than the base image a gain map may be, per axis, and still be believed. Real ones are full, half or
/// quarter resolution; anything smaller is not a gain map of this photograph.
const MAX_REDUCTION: u32 = 16;

/// The gain map of an Ultra HDR / ISO 21496-1 / Adobe gain-map JPEG: the image that records, for each part of the
/// scene, how much brighter it is in the HDR rendition than in the SDR base.
///
/// **It covers exactly the scene the picture's pixels cover**, at a resolution of its own: the source's, as cropped. An
/// enhancement leaves it alone (an upscale's result covers the same scene at more pixels, which the gain map need not
/// follow: viewers resample it to the base), and only a crop reframes it, through [`reframed`](Self::reframed).
///
/// Carried decoded (8-bit grayscale or RGB) and **turned upright with its base**: `rust-sak` applies the base's EXIF
/// orientation when it decodes it, and the gain-map image carries no EXIF of its own, so the load turns it by the
/// base's. With it come the gain-map image's own `hdrgm` XMP and ISO 21496-1 metadata, byte for byte; nothing in
/// either is interpreted, because a turn, a resize or a crop does not change the ratios they describe. **The source's
/// gain-map JPEG is kept too, until a crop reframes it**, so an export that neither reframes nor shrinks it writes it
/// back byte for byte rather than re-encoding it, which would lose a little more of it with every generation
/// ([`source`](Self::source)). A gain map that had to be turned has no such bytes: they hold the frame as stored.
#[derive(Debug, Clone)]
pub struct GainMap {
    pixels: Arc<DynamicImage>,
    /// The gain-map image's own `xmp` and `iso_gain_map`, and nothing else of its metadata.
    metadata: Arc<Metadata>,
    /// The source's gain-map-to-base scale per axis, fixed at load.
    scale: (f64, f64),
    /// The source's gain-map JPEG, as read, while the pixels are still exactly what it decodes to.
    source: Option<Arc<[u8]>>,
}

/// Why a JPEG's gain map was not carried.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GainMapError {
    /// The gain-map image is not a JPEG `rust-sak` can read.
    #[error("it does not decode: {0}")]
    Decode(#[from] ImageError),
    /// The gain-map image is a different format.
    #[error("it is {0:?}, not JPEG")]
    NotJpeg(ImageFormat),
    /// The gain-map image carries neither an `hdrgm` XMP packet nor an ISO 21496-1 payload of its own, so no reader can
    /// tell how to apply it: an MPF secondary image of the Gain Map type is not enough.
    #[error("it carries no gain-map metadata of its own (no hdrgm XMP, no ISO 21496-1 payload)")]
    Undescribed,
    /// The gain map is larger than the base, or implausibly small for it.
    #[error("it is {width}x{height}, which does not fit a {base_width}x{base_height} photograph")]
    Implausible { width: u32, height: u32, base_width: u32, base_height: u32 },
}

impl GainMap {
    /// Decodes the gain-map JPEG `encoded` of a base image of `base` pixels, as decoded, and turns it by `orientation`,
    /// the EXIF orientation the decode turned the base by ([`base_orientation`]).
    ///
    /// Its size is read from its header and checked **before** it is decoded: no larger than the base on either axis,
    /// and no smaller than a sixteenth of it, so a hostile file cannot make a load allocate more than its base image
    /// already did.
    pub(crate) fn read(encoded: &[u8], base: (u32, u32), orientation: Orientation) -> Result<Self, GainMapError> {
        Self::plausible(encoded, base, orientation)?;

        let mut pixels = decode_bytes(encoded)?;
        pixels.apply_orientation(orientation);
        let read = metadata_bytes(encoded);
        let mut metadata = Metadata::default();
        metadata.xmp = read.xmp;
        metadata.iso_gain_map = read.iso_gain_map;
        let scale = (f64::from(pixels.width()) / f64::from(base.0), f64::from(pixels.height()) / f64::from(base.1));
        let source = (orientation == Orientation::NoTransforms).then(|| Arc::from(encoded));
        Ok(Self { pixels: Arc::new(as_stored(pixels)), metadata: Arc::new(metadata), scale, source })
    }

    /// Whether the gain-map JPEG `encoded` is one [`read`](Self::read) would try to decode for a base image of `base`
    /// pixels turned by `orientation`, judged from its header alone: a JPEG, of a plausible size once turned the same
    /// way, that describes itself as a gain map with its own `hdrgm` XMP or ISO 21496-1 payload.
    pub(crate) fn plausible(encoded: &[u8], base: (u32, u32), orientation: Orientation) -> Result<(), GainMapError> {
        let info = probe_bytes(encoded)?;
        if info.format != ImageFormat::Jpeg {
            return Err(GainMapError::NotJpeg(info.format));
        }
        let (width, height) =
            if swaps_axes(orientation) { (info.height, info.width) } else { (info.width, info.height) };
        let fits = |side: u32, base: u32| side <= base && side.saturating_mul(MAX_REDUCTION) >= base;
        if !fits(width, base.0) || !fits(height, base.1) {
            return Err(GainMapError::Implausible { width, height, base_width: base.0, base_height: base.1 });
        }
        let own = metadata_bytes(encoded);
        let described = own.iso_gain_map.is_some()
            || own.xmp.as_deref().is_some_and(|xmp| xmp.windows(HDRGM.len()).any(|window| window == HDRGM));
        if !described {
            return Err(GainMapError::Undescribed);
        }
        Ok(())
    }

    /// The gain-map pixels: 8-bit grayscale (one gain for all channels) or RGB (one per channel).
    pub fn pixels(&self) -> &DynamicImage {
        &self.pixels
    }

    /// The source's gain-map JPEG, byte for byte, its own metadata inside it — `None` once a crop has reframed the gain
    /// map, when its pixels are no longer what those bytes decode to.
    pub fn source(&self) -> Option<&[u8]> {
        self.source.as_deref()
    }

    /// The gain-map image's own metadata: its `hdrgm` XMP packet and its ISO 21496-1 payload, byte for byte.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// The source's gain-map-to-base scale, horizontal and vertical: 1 for a full-resolution gain map, 0.25 for a
    /// quarter-resolution one.
    pub fn scale(&self) -> (f64, f64) {
        self.scale
    }

    /// This gain map with `frame` applied to its pixels, `frame` being handed them and the horizontal
    /// [`scale`](Self::scale) to express the framing at. The answer is brought back to the gain map's own colour type —
    /// a turn's alpha is dropped: the corners it exposes are transparent in the base and flattened in a JPEG, so their
    /// gain does not matter — and the metadata and scale are kept. The source's bytes are not: they no longer describe
    /// these pixels.
    #[must_use]
    pub fn reframed(&self, frame: impl FnOnce(&DynamicImage, f64) -> DynamicImage) -> Self {
        let framed = frame(&self.pixels, self.scale.0);
        let pixels = match self.pixels.color() {
            ColorType::L8 => DynamicImage::ImageLuma8(framed.to_luma8()),
            _ => DynamicImage::ImageRgb8(framed.to_rgb8()),
        };
        Self { pixels: Arc::new(pixels), metadata: Arc::clone(&self.metadata), scale: self.scale, source: None }
    }

    /// The pixels as a shared handle, for the run cache to store a rewritten gain map without copying it.
    pub(crate) fn shared_pixels(&self) -> Arc<DynamicImage> {
        Arc::clone(&self.pixels)
    }

    /// This gain map with `pixels` in place of its own, at the same resolution, encoding the same parameters: how an
    /// enhancement that changes what the photograph shows — a removal — hands back the gain map of its result. The
    /// metadata and scale are kept and the source's bytes are not, as for [`reframed`](Self::reframed), and the pixels
    /// are brought to the gain map's own colour type.
    #[must_use]
    pub(crate) fn rewritten(&self, pixels: DynamicImage) -> Self {
        self.reframed(|_, _| pixels)
    }

    /// A gain map holding `pixels` (8-bit grayscale or RGB) beside a base of `base` pixels, with the hawk's parameters.
    #[cfg(test)]
    pub(crate) fn holding(pixels: DynamicImage, base: (u32, u32)) -> Self {
        let channels = if pixels.color().has_color() { 3 } else { 1 };
        let encoded = crate::image::test_support::gain_map_jpeg((pixels.width(), pixels.height()), channels);
        Self::read(&encoded, base, Orientation::NoTransforms).expect("readable").rewritten(pixels)
    }

    /// This gain map with `metadata` in place of its own, for a test to give it metadata a writer refuses.
    #[cfg(test)]
    pub(crate) fn with_metadata(&self, metadata: Metadata) -> Self {
        Self { pixels: Arc::clone(&self.pixels), metadata: Arc::new(metadata), scale: self.scale, source: None }
    }
}

/// The EXIF orientation the decode turned a base image by, from its EXIF block `exif`: the block's Orientation, read as
/// the `image` crate's decoder reads it, or none without one.
pub(crate) fn base_orientation(exif: Option<&[u8]>) -> Orientation {
    exif.and_then(Orientation::from_exif_chunk).unwrap_or(Orientation::NoTransforms)
}

/// Whether `orientation` exchanges width and height.
fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    )
}

/// `pixels` as a gain map is held: grayscale or RGB, 8-bit. A JPEG decodes to one of the two already; anything else
/// (a CMYK JPEG the decoder turned into RGB is RGB) is brought to RGB.
fn as_stored(pixels: DynamicImage) -> DynamicImage {
    match pixels {
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageRgb8(_) => pixels,
        other if !other.color().has_color() => DynamicImage::ImageLuma8(other.to_luma8()),
        other => DynamicImage::ImageRgb8(other.to_rgb8()),
    }
}

#[cfg(test)]
mod tests {
    use ::image::{GrayImage, Luma, Rgba, RgbaImage};

    use super::*;
    use crate::image::test_support::{HAWK_GAIN_MAP_ISO, HAWK_GAIN_MAP_XMP, gain_map_jpeg};

    #[test]
    fn a_gain_map_is_read_with_its_own_metadata_and_scale() {
        let gain_map = GainMap::read(&gain_map_jpeg((8, 6), 3), (32, 24), Orientation::NoTransforms).expect("readable");

        assert_eq!((gain_map.pixels().width(), gain_map.pixels().height()), (8, 6));
        assert_eq!(gain_map.pixels().color(), ColorType::Rgb8);
        assert_eq!(gain_map.scale(), (0.25, 0.25));
        assert_eq!(gain_map.metadata().xmp.as_deref(), Some(HAWK_GAIN_MAP_XMP));
        assert_eq!(gain_map.metadata().iso_gain_map.as_deref(), Some(HAWK_GAIN_MAP_ISO));
        assert_eq!((gain_map.metadata().exif.as_ref(), gain_map.metadata().icc_profile.as_ref()), (None, None));

        let gray = GainMap::read(&gain_map_jpeg((32, 24), 1), (32, 24), Orientation::NoTransforms).expect("readable");
        assert_eq!(gray.pixels().color(), ColorType::L8);
        assert_eq!(gray.scale(), (1.0, 1.0));
    }

    #[test]
    fn a_gain_map_larger_than_the_base_or_far_smaller_is_refused_before_it_is_decoded() {
        for (size, base) in [((33, 24), (32, 24)), ((32, 25), (32, 24)), ((4, 4), (100, 64))] {
            let error =
                GainMap::read(&gain_map_jpeg(size, 1), base, Orientation::NoTransforms).expect_err("implausible");
            assert!(matches!(error, GainMapError::Implausible { .. }), "{size:?} for {base:?}: {error:?}");
        }
        assert!(matches!(
            GainMap::read(b"not an image", (32, 24), Orientation::NoTransforms),
            Err(GainMapError::Decode(_))
        ));
        let mut png = Vec::new();
        rust_sak::image::encode_writer(
            &DynamicImage::ImageLuma8(GrayImage::new(8, 6)),
            &mut png,
            ImageFormat::Png,
            None,
        )
        .expect("encodable");
        assert!(matches!(
            GainMap::read(&png, (32, 24), Orientation::NoTransforms),
            Err(GainMapError::NotJpeg(ImageFormat::Png))
        ));
    }

    #[test]
    fn a_gain_map_without_metadata_of_its_own_is_refused() {
        // An MPF secondary image of the Gain Map type, with neither `hdrgm` XMP nor an ISO 21496-1 payload: libultrahdr
        // and ImageIO find no gain map in it, so it must not be written back as one.
        let mut bare = Vec::new();
        let pixels = DynamicImage::ImageLuma8(GrayImage::from_pixel(8, 6, Luma([128])));
        rust_sak::image::encode_writer(&pixels, &mut bare, ImageFormat::Jpeg, None).expect("encodable");
        assert!(matches!(
            GainMap::plausible(&bare, (32, 24), Orientation::NoTransforms),
            Err(GainMapError::Undescribed)
        ));
        let error = GainMap::read(&bare, (32, 24), Orientation::NoTransforms).expect_err("undescribed");
        assert_eq!(
            error.to_string(),
            "it carries no gain-map metadata of its own (no hdrgm XMP, no ISO 21496-1 payload)"
        );

        // Either one is enough.
        for (xmp, iso) in [(true, false), (false, true)] {
            let mut metadata = Metadata::default();
            metadata.xmp = xmp.then(|| HAWK_GAIN_MAP_XMP.to_vec());
            metadata.iso_gain_map = iso.then(|| HAWK_GAIN_MAP_ISO.to_vec());
            let mut described = Vec::new();
            rust_sak::image::encode_writer_with(&pixels, &mut described, ImageFormat::Jpeg, None, &metadata)
                .expect("encodable");
            assert!(GainMap::read(&described, (32, 24), Orientation::NoTransforms).is_ok(), "xmp {xmp}, iso {iso}");
        }
    }

    #[test]
    fn a_gain_map_is_turned_with_its_base_and_judged_turned() {
        // Stored 8x6 beside a base the decode turned upright from 32x24 to 24x32 (Orientation 6).
        let bytes = gain_map_jpeg((8, 6), 3);
        let turned = GainMap::read(&bytes, (24, 32), Orientation::Rotate90).expect("readable");

        assert_eq!((turned.pixels().width(), turned.pixels().height()), (6, 8));
        assert_eq!(turned.scale(), (0.25, 0.25));
        assert_eq!(turned.source(), None, "the source's bytes hold the frame as stored");
        // The first channel ramps across the stored frame, so a quarter turn clockwise makes it ramp downward.
        let first = |x, y| turned.pixels().to_rgb8().get_pixel(x, y).0[0];
        assert!(first(0, 7) > first(0, 0) + 100 && first(0, 0).abs_diff(first(5, 0)) < 12);

        // Judged at its stored size against the upright base, a full-resolution one would not fit.
        let full = gain_map_jpeg((32, 24), 1);
        assert!(GainMap::read(&full, (24, 32), Orientation::Rotate90).is_ok());
        let error = GainMap::read(&full, (24, 32), Orientation::NoTransforms).expect_err("not turned");
        assert!(matches!(error, GainMapError::Implausible { .. }), "{error:?}");
    }

    #[test]
    fn the_orientation_of_a_base_is_its_exif_orientation_or_none() {
        assert_eq!(base_orientation(Some(crate::image::test_support::EXIF)), Orientation::Rotate90);
        assert_eq!(base_orientation(None), Orientation::NoTransforms);
        assert_eq!(base_orientation(Some(b"not exif")), Orientation::NoTransforms);
    }

    #[test]
    fn the_source_bytes_are_kept_until_a_reframe() {
        let bytes = gain_map_jpeg((8, 6), 1);
        let gain_map = GainMap::read(&bytes, (32, 24), Orientation::NoTransforms).expect("readable");

        assert_eq!(gain_map.source(), Some(&bytes[..]));
        assert_eq!(gain_map.clone().source(), Some(&bytes[..]), "a clone, as a derived picture holds, keeps them");
        assert_eq!(gain_map.reframed(|pixels, _| pixels.clone()).source(), None);
    }

    #[test]
    fn reframing_keeps_the_colour_type_and_the_metadata_and_drops_alpha() {
        let gain_map = GainMap::read(&gain_map_jpeg((8, 6), 1), (32, 24), Orientation::NoTransforms).expect("readable");

        let mut handed = None;
        let framed = gain_map.reframed(|pixels, scale| {
            handed = Some(scale);
            assert_eq!(pixels.width(), 8);
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 2, Rgba([9, 9, 9, 0])))
        });

        assert_eq!(handed, Some(0.25));
        assert_eq!(framed.pixels().color(), ColorType::L8);
        assert_eq!(framed.pixels().as_luma8().expect("gray").get_pixel(0, 0), &Luma([9]));
        assert!(Arc::ptr_eq(&framed.metadata, &gain_map.metadata));
        assert_eq!(framed.scale(), gain_map.scale());
    }
}
