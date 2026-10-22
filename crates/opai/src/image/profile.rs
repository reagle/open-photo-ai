//! What this crate needs to know about an ICC colour profile it carries: which colour model it is for, and what it is
//! called. Nothing else in a profile is read here — colour conversion is not this module's, nor yet anyone's.

use std::borrow::Cow;
use std::path::Path;

use ::image::DynamicImage;
use rust_sak::image::{Embedded, ImageFormat, Metadata};

/// The profile's data colour space, the four-character signature at bytes 16..20 of its header (`RGB `, `GRAY`,
/// `CMYK`, …), trimmed; `None` if there is no header to read it from.
pub(super) fn colour_space(profile: &[u8]) -> Option<&str> {
    let signature = profile.get(16..20)?;
    std::str::from_utf8(signature).ok().map(str::trim_end)
}

/// Whether `profile` is for the colour model `pixels` are in: an `RGB ` profile for colour pixels, a `GRAY` one for
/// grayscale. Anything else (`CMYK`, `Lab `, a malformed header) describes neither, and is not embedded beside them.
pub(super) fn describes(profile: &[u8], pixels: &DynamicImage) -> bool {
    fits(profile, pixels.color().has_color())
}

/// Whether `profile` is for colour pixels (`colour`) or grayscale ones (`!colour`).
fn fits(profile: &[u8], colour: bool) -> bool {
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
    let colour = pixels.color();
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
pub(super) fn description(profile: &[u8]) -> Option<String> {
    rust_sak::image::icc_profile_description(profile)
}

/// `metadata` as it may be embedded beside `pixels` written as `format`: unchanged, unless its profile is for another
/// colour model than the file will hold — a grayscale photograph's `GRAY` profile after colourisation made it RGB, or
/// beside grayscale-with-alpha written as TIFF, which `rust-sak` expands to RGBA (see [`written_in_colour`]) — in which
/// case that profile is left out. Borrowed whenever nothing is left out, which is every ordinary export.
pub(super) fn embeddable<'a>(metadata: &'a Metadata, pixels: &DynamicImage, format: ImageFormat) -> Cow<'a, Metadata> {
    match &metadata.icc_profile {
        Some(profile) if !fits(profile, written_in_colour(format, pixels)) => {
            let mut fitting = metadata.clone();
            fitting.icc_profile = None;
            Cow::Owned(fitting)
        }
        _ => Cow::Borrowed(metadata),
    }
}

/// Records, once, that the profile `metadata` carried did not reach a file written as `format`: either because the
/// format cannot hold one (BMP, GIF, AVIF and HEIF today) or because it did not describe the pixels. A warning rather
/// than a refusal — the file is still the photograph — and silent when there was no profile to lose.
pub(super) fn warn_if_dropped(metadata: &Metadata, embedded: Embedded, format: ImageFormat, path: Option<&Path>) {
    let Some(profile) = &metadata.icc_profile else { return };
    if embedded.icc_profile {
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
    fn the_fixture_names_itself() {
        // The compact profiles name themselves tersely; what matters is that the name comes back at all.
        assert_eq!(description(DISPLAY_P3).as_deref(), Some("sP3"));
    }
}
