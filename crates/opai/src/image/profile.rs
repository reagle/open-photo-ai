//! What this crate needs to know about an ICC colour profile it carries: which colour model it is for, and what it is
//! called. Nothing else in a profile is read here — colour conversion is not this module's, nor yet anyone's.

use std::borrow::Cow;
use std::path::Path;

use ::image::{ColorType, DynamicImage};
use rust_sak::image::{Embedded, ImageFormat, Metadata};

/// The profile's data colour space, the four-character signature at bytes 16..20 of its header (`RGB `, `GRAY`,
/// `CMYK`, …), trimmed; `None` if there is no header to read it from.
pub(crate) fn colour_space(profile: &[u8]) -> Option<&str> {
    let signature = profile.get(16..20)?;
    std::str::from_utf8(signature).ok().map(str::trim_end)
}

/// Whether `profile` is for the colour model `pixels` are in: an `RGB ` profile for colour pixels, a `GRAY` one for
/// grayscale. Anything else (`CMYK`, `Lab `, a malformed header) describes neither, and is not embedded beside them.
pub(super) fn describes(profile: &[u8], pixels: &DynamicImage) -> bool {
    fits(profile, pixels.color().has_color())
}

/// Whether `profile` is for colour pixels (`colour`) or grayscale ones (`!colour`).
pub(crate) fn fits(profile: &[u8], colour: bool) -> bool {
    match colour_space(profile) {
        Some("RGB") => colour,
        Some("GRAY") => !colour,
        _ => false,
    }
}

/// Whether the file `rust-sak` writes from `pixels` as `format` holds colour pixels — which is not always whether
/// `pixels` themselves do. Grayscale stays grayscale only in JPEG, PNG and (without alpha) TIFF. Everywhere else it
/// is written as colour: TIFF has no grayscale-with-alpha, so `rust-sak` expands it to RGBA; GIF and BMP are written
/// as RGB; libwebp always encodes colour (YUV); and the AVIF and HEIF encoders write colour too.
///
/// Mirrors `rust_sak::image`'s `make_compatible` and the codecs behind it, which are not public; the test
/// `the_written_colour_model_is_the_one_rust_sak_writes` encodes through `rust-sak` and fails if the two drift apart.
pub(super) fn written_in_colour(format: ImageFormat, pixels: &DynamicImage) -> bool {
    writes_colour(format, pixels.color())
}

/// [`written_in_colour`] for pixels of `colour`, without needing them.
pub(crate) fn writes_colour(format: ImageFormat, colour: ColorType) -> bool {
    if colour.has_color() {
        return true;
    }
    match format {
        ImageFormat::Jpeg | ImageFormat::Png => false,
        ImageFormat::Tiff => colour.has_alpha(),
        ImageFormat::Bmp | ImageFormat::Gif | ImageFormat::WebP | ImageFormat::Avif | ImageFormat::Heif => true,
    }
}

/// The profile's own description (`Display P3`), for a log record to name it by.
pub(crate) fn description(profile: &[u8]) -> Option<String> {
    rust_sak::image::icc_profile_description(profile)
}

/// Whether `profile` is plain sRGB, by the description the common sRGB profiles give themselves (`sRGB
/// IEC61966-2.1`, its black-scaled and 2014 variants, `sRGB built-in`): what an AVIF or HEIF file says without one, since
/// its unspecified colour description is read as sRGB. Judged by name, so a profile that calls itself sRGB and is not
/// is believed; the cost is one warning missed for a mislabelled profile.
pub(crate) fn is_srgb(profile: &[u8]) -> bool {
    colour_space(profile) == Some("RGB")
        && description(profile).is_some_and(|name| {
            let name = name.to_ascii_lowercase();
            name == "srgb"
                || name == "srgb built-in"
                || name.starts_with("srgb iec61966-2")
                || name.starts_with("srgb2014")
        })
}

/// Whether leaving `profile` out of a file written as `format` changes how its colours read: not for plain sRGB in an
/// AVIF or HEIF, whose colours are then read as sRGB anyway ([`is_srgb`]).
pub(crate) fn matters_in(profile: &[u8], format: ImageFormat) -> bool {
    !(matches!(format, ImageFormat::Avif | ImageFormat::Heif) && is_srgb(profile))
}

/// `metadata` as it may be embedded beside `pixels` written as `format`: unchanged, unless its profile is for another
/// colour model than the file will hold — a grayscale photograph's `GRAY` profile after colourisation made it RGB, or
/// beside grayscale-with-alpha written as TIFF, which `rust-sak` expands to RGBA (see [`written_in_colour`]) — in which
/// case that profile is left out. Borrowed whenever nothing is left out, which is every ordinary export.
pub(crate) fn embeddable<'a>(metadata: &'a Metadata, pixels: &DynamicImage, format: ImageFormat) -> Cow<'a, Metadata> {
    match &metadata.icc_profile {
        Some(profile) if !fits(profile, written_in_colour(format, pixels)) => {
            let mut fitting = metadata.clone();
            fitting.icc_profile = None;
            Cow::Owned(fitting)
        }
        _ => Cow::Borrowed(metadata),
    }
}

/// Records, once, that the profile `metadata` carried did not reach a file written as `format`: because the format
/// cannot hold one (BMP, GIF, AVIF and HEIF today), because it did not describe the pixels, or because the encoder could
/// not write it (a profile too large for the format). A warning rather than a refusal — the file is still the
/// photograph — and silent when there was no profile to lose, or when it was plain sRGB left out of an AVIF or HEIF,
/// which reads as sRGB without it ([`matters_in`]).
pub(super) fn warn_if_dropped(metadata: &Metadata, embedded: Embedded, format: ImageFormat, path: Option<&Path>) {
    let Some(profile) = &metadata.icc_profile else { return };
    if embedded.icc_profile || !matters_in(profile, format) {
        return;
    }

    let op = if path.is_some() { "save" } else { "encode" };
    let path = path.map(|path| path.display().to_string());
    tracing::warn!(
        op,
        path,
        format = format.name(),
        profile = description(profile).unwrap_or_default(),
        colour_space = colour_space(profile).unwrap_or_default(),
        "colour profile not embedded"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::test_support::{DISPLAY_P3, sample_image};

    fn with_colour_space(signature: &[u8; 4]) -> Vec<u8> {
        let mut profile = DISPLAY_P3.to_vec();
        profile[16..20].copy_from_slice(signature);
        profile
    }

    #[test]
    fn an_rgb_profile_describes_colour_pixels_and_not_grayscale_ones() {
        let colour = sample_image();
        let gray = DynamicImage::ImageLuma8(colour.to_luma8());

        assert_eq!(colour_space(DISPLAY_P3), Some("RGB"));
        assert!(describes(DISPLAY_P3, &colour));
        assert!(!describes(DISPLAY_P3, &gray));
    }

    #[test]
    fn a_gray_profile_describes_only_grayscale_pixels_and_cmyk_describes_neither() {
        let colour = sample_image();
        let gray = DynamicImage::ImageLumaA8(colour.to_luma_alpha8());

        assert!(describes(&with_colour_space(b"GRAY"), &gray));
        assert!(!describes(&with_colour_space(b"GRAY"), &colour));
        assert!(!describes(&with_colour_space(b"CMYK"), &colour));
        assert!(!describes(b"short", &colour));
    }

    #[test]
    fn the_written_colour_model_is_the_one_rust_sak_writes() {
        // `written_in_colour` restates what `rust-sak` converts to before encoding; this encodes through `rust-sak`
        // itself, for every format and every grayscale and colour layout, and asks the written file. A `rust-sak`
        // that changed what it writes fails here rather than embedding a profile for the wrong colour model.
        let colour = sample_image();
        let pictures = [
            DynamicImage::ImageLuma8(colour.to_luma8()),
            DynamicImage::ImageLumaA8(colour.to_luma_alpha8()),
            DynamicImage::ImageLuma16(colour.to_luma16()),
            DynamicImage::ImageLumaA16(colour.to_luma_alpha16()),
            colour.clone(),
            DynamicImage::ImageRgba8(colour.to_rgba8()),
        ];
        let formats = [
            ImageFormat::Bmp,
            ImageFormat::Gif,
            ImageFormat::Jpeg,
            ImageFormat::Png,
            ImageFormat::Tiff,
            ImageFormat::Avif,
            ImageFormat::Heif,
            ImageFormat::WebP,
        ];

        let mut drifted = Vec::new();
        for format in formats {
            for pixels in &pictures {
                let mut bytes = Vec::new();
                rust_sak::image::encode_writer(pixels, &mut bytes, format, None).expect("encodable");
                let written = rust_sak::image::decode_bytes(&bytes).expect("decodable");
                if written_in_colour(format, pixels) != written.color().has_color() {
                    drifted.push(format!("{format:?} from {:?} is written as {:?}", pixels.color(), written.color()));
                }
            }
        }
        assert!(drifted.is_empty(), "`written_in_colour` disagrees with rust-sak:\n{}", drifted.join("\n"));
    }

    #[test]
    fn a_gray_profile_is_left_out_where_the_file_will_hold_colour() {
        let mut gray = DISPLAY_P3.to_vec();
        gray[16..20].copy_from_slice(b"GRAY");
        let mut metadata = Metadata::default();
        metadata.icc_profile = Some(gray);
        let luma_alpha = DynamicImage::ImageLumaA8(sample_image().to_luma_alpha8());
        let luma = DynamicImage::ImageLuma8(sample_image().to_luma8());

        // Grayscale-with-alpha becomes RGBA in TIFF, and WebP is always colour.
        assert_eq!(embeddable(&metadata, &luma_alpha, ImageFormat::Tiff).icc_profile, None);
        assert_eq!(embeddable(&metadata, &luma, ImageFormat::WebP).icc_profile, None);
        // Where grayscale stays grayscale, the profile describes it.
        assert!(embeddable(&metadata, &luma, ImageFormat::Tiff).icc_profile.is_some());
        assert!(embeddable(&metadata, &luma_alpha, ImageFormat::Png).icc_profile.is_some());
        assert!(embeddable(&metadata, &luma_alpha, ImageFormat::Jpeg).icc_profile.is_some());
    }

    #[test]
    fn plain_srgb_is_known_by_its_name_and_only_matters_outside_avif_and_heif() {
        use crate::image::test_support::{rgb_profile_named, srgb_profile};

        for name in ["sRGB IEC61966-2.1", "sRGB IEC61966-2-1 black scaled", "sRGB built-in", "sRGB2014", "sRGB"] {
            assert!(is_srgb(&rgb_profile_named(name, 0)), "{name}");
        }
        for name in ["Display P3", "sRGB linear", "Adobe RGB (1998)", "ProPhoto sRGB-ish"] {
            assert!(!is_srgb(&rgb_profile_named(name, 0)), "{name}");
        }
        let mut gray = srgb_profile(0);
        gray[16..20].copy_from_slice(b"GRAY");
        assert!(!is_srgb(&gray), "a grayscale profile is not sRGB, whatever it says");
        assert!(!is_srgb(DISPLAY_P3));

        let srgb = srgb_profile(0);
        assert!(!matters_in(&srgb, ImageFormat::Avif) && !matters_in(&srgb, ImageFormat::Heif));
        assert!(matters_in(&srgb, ImageFormat::Gif) && matters_in(DISPLAY_P3, ImageFormat::Avif));
    }

    #[test]
    fn the_fixture_names_itself() {
        // The compact profiles name themselves tersely; what matters is that the name comes back at all.
        assert_eq!(description(DISPLAY_P3).as_deref(), Some("sP3"));
    }
}
