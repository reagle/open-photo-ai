//! What a source file held that a written file can lose: its depth, whether its samples are floating point, whether it
//! is HDR, and the colour description the loaded picture does not carry on as an ICC profile.

use ::image::{ColorType, DynamicImage};
use rust_sak::image::{Hdr, ImageInfo, hdr_bytes, probe_bytes};

/// Bits per channel, and whether the samples are floating point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Depth {
    /// Bits per channel: 8, 10, 12, 16 or 32.
    pub bits: u8,
    /// Whether the samples are floating point (32-bit float) rather than integers.
    pub float: bool,
}

impl Depth {
    /// The depth `colour` stores its samples at.
    pub fn of(colour: ColorType) -> Self {
        let channels = u16::from(colour.channel_count()).max(1);
        let bits = u8::try_from(colour.bits_per_pixel() / channels).unwrap_or(u8::MAX);
        Self { bits, float: matches!(colour, ColorType::Rgb32F | ColorType::Rgba32F) }
    }

    /// Whether a file at `self` holds less than `source`: fewer bits per channel, or integers where it had floats.
    pub fn narrower_than(self, source: Self) -> bool {
        self.bits < source.bits || (source.float && !self.float)
    }
}

impl std::fmt::Display for Depth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.float { write!(f, "{}-bit float", self.bits) } else { write!(f, "{}-bit", self.bits) }
    }
}

/// A colour profile the source embedded that the loaded picture does not carry, because it is for another colour model
/// than the decoded pixels (a CMYK profile on a JPEG the decoder turned into RGB).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discarded {
    /// The profile's own description (`U.S. Web Coated (SWOP) v2`), if it has one.
    pub description: Option<String>,
    /// Its data colour space signature, trimmed (`CMYK`, `GRAY`, `Lab`).
    pub colour_space: String,
}

/// What the source file held that a written file can lose, recorded when it is loaded and carried by every picture
/// derived from it, so an export compares what it writes against the file the user chose rather than against
/// intermediate pixels.
///
/// - **`depth`**: the file's true bits per channel (10 or 12 for such an AVIF or HEIF, not the 16 it decodes to), and
///   whether its samples are floating point.
/// - **`hdr`**: how the file declares HDR, if it does ([`rust_sak::image::hdr_bytes`]: a JPEG gain map, or a PQ or HLG
///   transfer).
/// - **`discarded`**: an ICC profile the load left out because it does not describe the decoded pixels.
/// - **`coded`**: the name of a coded (CICP `nclx`) colour description other than sRGB, carried by an AVIF or HEIF
///   without an ICC profile (`Display P3`, `Rec. 2020`). Nothing writes one yet, so every export loses it. A PQ or HLG
///   description is the HDR above and is not repeated here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fidelity {
    /// The source's depth.
    pub depth: Depth,
    /// How the source declares HDR, if it does.
    pub hdr: Option<Hdr>,
    /// The ICC profile the load discarded, if any.
    pub discarded: Option<Discarded>,
    /// A coded colour description the source carries instead of an ICC profile, if it is not sRGB.
    pub coded: Option<String>,
}

impl Fidelity {
    /// What pixels with nothing behind them hold: their own depth, no HDR, nothing discarded. What
    /// [`Picture::new`](super::Picture::new) records, and what a developed RAW is (16-bit sRGB, SDR).
    pub fn of_pixels(pixels: &DynamicImage) -> Self {
        Self::of_colour(pixels.color())
    }

    /// [`of_pixels`](Self::of_pixels) for pixels of `colour`, without needing them.
    pub fn of_colour(colour: ColorType) -> Self {
        Self { depth: Depth::of(colour), hdr: None, discarded: None, coded: None }
    }

    /// Reads what the encoded file `bytes` holds, **without decoding its pixels**, given the colour type it decodes to
    /// (`decoded`) and the ICC profile it embeds (`profile`). Infallible: anything that cannot be read is read as the
    /// decoded pixels' own depth and as not HDR.
    ///
    /// The depth is the smaller of the header's and the decoded pixels', so a 10-bit AVIF that decodes to 16 is 10, and
    /// a palette PNG whose header names 4-bit indices is never wider than its 8-bit pixels.
    pub(crate) fn read(bytes: &[u8], decoded: ColorType, profile: Option<&[u8]>) -> Self {
        Self::read_with(probe_bytes(bytes).ok().as_ref(), bytes, decoded, profile)
    }

    /// [`read`](Self::read) with the file's probe already in hand, or `None` where it could not be probed.
    pub(crate) fn read_with(
        info: Option<&ImageInfo>,
        bytes: &[u8],
        decoded: ColorType,
        profile: Option<&[u8]>,
    ) -> Self {
        let mut depth = Depth::of(decoded);
        if let Some(info) = info
            && !depth.float
        {
            depth.bits = depth.bits.min(info.bit_depth.max(1));
        }

        let discarded =
            profile
                .filter(|profile| !super::profile::fits(profile, decoded.has_color()))
                .map(|profile| Discarded {
                    description: super::profile::description(profile),
                    colour_space: super::profile::colour_space(profile).unwrap_or_default().to_owned(),
                });
        // `probe_bytes` names an embedded ICC profile first and a coded description only when there is none.
        let coded = profile
            .is_none()
            .then(|| info.and_then(|info| info.color_profile.clone()))
            .flatten()
            .filter(|name| name != "sRGB" && !name.starts_with("Rec. 2100"));

        Self { depth, hdr: hdr_bytes(bytes), discarded, coded }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::test_support::{DISPLAY_P3, encoded, sample_image, written_jpeg, written_p3};
    use rust_sak::image::{ImageFormat, decode_bytes};

    #[test]
    fn a_depth_is_its_colour_types_bits_per_channel() {
        assert_eq!(Depth::of(ColorType::Rgb8), Depth { bits: 8, float: false });
        assert_eq!(Depth::of(ColorType::La16), Depth { bits: 16, float: false });
        assert_eq!(Depth::of(ColorType::Rgba32F), Depth { bits: 32, float: true });
        assert_eq!(Depth { bits: 16, float: false }.to_string(), "16-bit");
        assert_eq!(Depth { bits: 32, float: true }.to_string(), "32-bit float");
    }

    #[test]
    fn narrower_is_fewer_bits_or_floats_lost() {
        let eight = Depth { bits: 8, float: false };
        let sixteen = Depth { bits: 16, float: false };
        let float = Depth { bits: 32, float: true };

        assert!(eight.narrower_than(sixteen));
        assert!(sixteen.narrower_than(float));
        assert!(!sixteen.narrower_than(eight));
        assert!(!sixteen.narrower_than(sixteen));
        assert!(!float.narrower_than(float));
        // Ten bits in sixteen loses nothing.
        assert!(!sixteen.narrower_than(Depth { bits: 10, float: false }));
    }

    #[test]
    fn an_ordinary_jpeg_is_8_bit_sdr_with_nothing_discarded() {
        let bytes = written_jpeg();
        let fidelity = Fidelity::read(&bytes, decode_bytes(&bytes).expect("decodable").color(), None);

        assert_eq!(fidelity, Fidelity::of_colour(ColorType::Rgb8));
    }

    #[test]
    fn a_16_bit_png_is_16_bit() {
        let pixels = DynamicImage::ImageRgb16(sample_image().to_rgb16());
        let mut bytes = Vec::new();
        rust_sak::image::encode_writer(&pixels, &mut bytes, ImageFormat::Png, None).expect("encodable");

        let fidelity = Fidelity::read(&bytes, ColorType::Rgb16, None);

        assert_eq!(fidelity.depth, Depth { bits: 16, float: false });
        assert_eq!(fidelity.hdr, None);
    }

    #[test]
    fn a_profile_for_another_colour_model_is_recorded_as_discarded() {
        let mut cmyk = DISPLAY_P3.to_vec();
        cmyk[16..20].copy_from_slice(b"CMYK");

        let fidelity = Fidelity::read(&written_p3(ImageFormat::Jpeg), ColorType::Rgb8, Some(&cmyk));

        assert_eq!(
            fidelity.discarded,
            Some(Discarded { description: Some("sP3".into()), colour_space: "CMYK".into() })
        );
        // One that fits is not.
        assert_eq!(
            Fidelity::read(&written_p3(ImageFormat::Jpeg), ColorType::Rgb8, Some(DISPLAY_P3)).discarded,
            None
        );
    }

    #[test]
    fn a_coded_display_p3_avif_records_its_description_and_srgb_does_not() {
        let avif = encoded(ImageFormat::Avif, None);
        let info = probe_bytes(&avif).expect("probed");
        let mut p3 = info.clone();
        p3.color_profile = Some("Display P3".into());
        let mut pq = info.clone();
        pq.color_profile = Some("Rec. 2100 PQ".into());
        let mut srgb = info;
        srgb.color_profile = Some("sRGB".into());

        assert_eq!(
            Fidelity::read_with(Some(&p3), &avif, ColorType::Rgb8, None).coded.as_deref(),
            Some("Display P3")
        );
        assert_eq!(Fidelity::read_with(Some(&srgb), &avif, ColorType::Rgb8, None).coded, None);
        assert_eq!(Fidelity::read_with(Some(&pq), &avif, ColorType::Rgb8, None).coded, None);
        // Beside an ICC profile, the probe's name is the profile's and nothing is coded.
        assert_eq!(Fidelity::read_with(Some(&p3), &avif, ColorType::Rgb8, Some(DISPLAY_P3)).coded, None);
    }

    #[test]
    fn unreadable_bytes_are_read_as_the_decoded_pixels() {
        assert_eq!(Fidelity::read(b"not an image", ColorType::Rgb16, None), Fidelity::of_colour(ColorType::Rgb16));
    }
}
