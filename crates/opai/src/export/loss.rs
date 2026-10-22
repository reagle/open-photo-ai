//! What writing a picture as a format loses against the file it came from: HDR, bits per channel, the colour profile.
//!
//! One assessment, asked before the file is written, which both front ends show and neither restates. It predicts what
//! `rust-sak` writes — the depth its encode dispatch narrows to, the formats that hold an ICC profile, the colour model
//! the file is written in — and `the_prediction_is_what_rust_sak_writes` holds it to that by encoding every format and
//! colour layout and reading the result back. [`write`](fn@super::write) then reconciles it with what the encoder
//! reports it embedded, which only the write can know: a profile too large for the format
//! ([`ProfileCause::NotWritten`]).
//!
//! Each kind is one place to change when a later slice of the colour series stops losing it: HDR output (Ultra HDR),
//! a chosen depth, profiles in AVIF and HEIF.

use std::fmt;

use ::image::ColorType;
use serde::{Serialize, Serializer};

use rust_sak::image::Embedded;

use crate::image::profile::{colour_space, description, fits, matters_in, writes_colour};
use crate::image::{Depth, Fidelity, Hdr, ImageFormat, Picture};

/// One thing a written file lost against its source.
///
/// `Display` gives the English sentence the CLI and the log use (`16-bit → 8-bit in JPEG`); `Serialize` gives a value
/// tagged by `kind` (`hdr`, `depth`, `profile`) whose fields a translated front end words for itself. A format is
/// serialized by its name (`JPEG`, `HEIF`), which is not translated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Loss {
    /// The source is HDR and the file is SDR: no format is written as HDR yet.
    Hdr {
        /// How the source declared HDR, serialized as `gainMap`, `pq` or `hlg`.
        #[serde(serialize_with = "hdr_name")]
        hdr: Hdr,
        /// What the file is written as.
        #[serde(serialize_with = "format_name")]
        format: ImageFormat,
    },
    /// The file holds fewer bits per channel than the source, or integers where it had floats.
    Depth {
        /// The source's depth.
        from: Depth,
        /// The file's.
        to: Depth,
        /// What the file is written as.
        #[serde(serialize_with = "format_name")]
        format: ImageFormat,
    },
    /// The source's colour profile, or its coded colour description, is not in the file.
    Profile {
        /// The profile's own description (`Display P3`), or the coded description's name, if it has one.
        profile: Option<String>,
        /// The profile's colour space signature (`RGB`, `GRAY`, `CMYK`), empty for a coded description.
        colour_space: String,
        /// What the file is written as.
        #[serde(serialize_with = "format_name")]
        format: ImageFormat,
        /// Why it is not there.
        cause: ProfileCause,
    },
}

/// Why a [`Loss::Profile`] happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileCause {
    /// The format cannot hold an ICC profile: BMP and GIF, and AVIF and HEIF until a later slice writes one.
    Format,
    /// The profile is for another colour model than the file is written in: a grayscale profile on a picture
    /// colourisation turned to colour, or written as a format that is always colour.
    ColourModel,
    /// The load left the profile out because it does not describe the decoded pixels (a CMYK profile on a JPEG the
    /// decoder turned into RGB), so no export has it.
    Discarded,
    /// The source carried a coded (CICP `nclx`) colour description rather than a profile, and nothing writes one yet.
    Coded,
    /// The format holds a profile, but the encoder did not write this one: one too large for it, as a JPEG's 255
    /// `APP2` segments cap a profile at about 16 MB. Known only once the file is written, so never predicted.
    NotWritten,
}

impl fmt::Display for Loss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hdr { hdr: Hdr::GainMap, format } => {
                write!(f, "HDR gain map dropped: {} output is SDR only for now", format.name())
            }
            Self::Hdr { hdr, format } => {
                write!(f, "HDR ({}) dropped: {} output is SDR only for now", hdr.name(), format.name())
            }
            Self::Depth { from, to, format } => write!(f, "{from} → {to} in {}", format.name()),
            Self::Profile { profile, colour_space, format, cause } => {
                let name = profile.as_deref().unwrap_or("ICC");
                match cause {
                    ProfileCause::Format => write!(f, "{name} profile not embedded in {}", format.name()),
                    ProfileCause::ColourModel => write!(
                        f,
                        "{name} profile not embedded in {}: it is for {colour_space}, not the file's colours",
                        format.name()
                    ),
                    ProfileCause::Discarded => {
                        write!(f, "{name} profile dropped: it is for {colour_space}, not the decoded pixels")
                    }
                    ProfileCause::Coded => {
                        write!(f, "{name} colour description not written in {}", format.name())
                    }
                    ProfileCause::NotWritten => {
                        write!(f, "{name} profile not embedded in {}: the encoder could not write it", format.name())
                    }
                }
            }
        }
    }
}

impl Loss {
    /// The kind, as `Serialize` tags it and the CLI counts it: `hdr`, `depth` or `profile`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Hdr { .. } => "hdr",
            Self::Depth { .. } => "depth",
            Self::Profile { .. } => "profile",
        }
    }
}

/// What writing `picture` as `format` loses against the file it came from, in the order HDR, depth, profile; empty
/// when nothing is lost. Asked before the write, from the picture's [`fidelity`](Picture::fidelity) and
/// [`metadata`](Picture::metadata), so it costs no encode.
pub fn losses(picture: &Picture, format: ImageFormat) -> Vec<Loss> {
    assess(picture.fidelity(), picture.metadata().icc_profile.as_deref(), picture.pixels().color(), format)
}

/// `predicted` — [`losses`] of `picture` as `format` — made to agree with what the encoder reports it `embedded`: a
/// profile the picture carried, not already counted as lost, that the file does not hold is lost as
/// [`ProfileCause::NotWritten`], beside the other profile losses.
pub(crate) fn reconcile(
    mut predicted: Vec<Loss>,
    picture: &Picture,
    format: ImageFormat,
    embedded: Embedded,
) -> Vec<Loss> {
    let Some(profile) = picture.metadata().icc_profile.as_deref() else { return predicted };
    let counted = predicted
        .iter()
        .any(|loss| matches!(loss, Loss::Profile { cause: ProfileCause::Format | ProfileCause::ColourModel, .. }));
    if embedded.icc_profile || counted || !matters_in(profile, format) {
        return predicted;
    }
    let at = predicted
        .iter()
        .position(|loss| matches!(loss, Loss::Profile { .. }))
        .unwrap_or(predicted.len());
    let lost = Loss::Profile {
        profile: description(profile),
        colour_space: colour_space(profile).unwrap_or_default().to_owned(),
        format,
        cause: ProfileCause::NotWritten,
    };
    predicted.insert(at, lost);
    predicted
}

/// [`losses`] for pixels of `colour` carrying `profile`, from a source of `fidelity`.
pub(crate) fn assess(fidelity: &Fidelity, profile: Option<&[u8]>, colour: ColorType, format: ImageFormat) -> Vec<Loss> {
    let mut losses = Vec::new();

    // Always lost, for now: nothing is written as HDR until the Ultra HDR slice.
    if let Some(hdr) = fidelity.hdr {
        losses.push(Loss::Hdr { hdr, format });
    }

    let to = written_depth(format, colour);
    if to.narrower_than(fidelity.depth) {
        losses.push(Loss::Depth { from: fidelity.depth, to, format });
    }

    let profile_loss = |profile: &[u8], cause| Loss::Profile {
        profile: description(profile),
        colour_space: colour_space(profile).unwrap_or_default().to_owned(),
        format,
        cause,
    };
    if let Some(profile) = profile {
        if !holds_profile(format) {
            // Plain sRGB in an AVIF or HEIF is not lost: their colours read as sRGB without it.
            if matters_in(profile, format) {
                losses.push(profile_loss(profile, ProfileCause::Format));
            }
        } else if !fits(profile, writes_colour(format, colour)) {
            losses.push(profile_loss(profile, ProfileCause::ColourModel));
        }
    }
    if let Some(discarded) = &fidelity.discarded {
        losses.push(Loss::Profile {
            profile: discarded.description.clone(),
            colour_space: discarded.colour_space.clone(),
            format,
            cause: ProfileCause::Discarded,
        });
    }
    if let Some(coded) = &fidelity.coded {
        losses.push(Loss::Profile {
            profile: Some(coded.clone()),
            colour_space: String::new(),
            format,
            cause: ProfileCause::Coded,
        });
    }

    losses
}

/// The depth `rust-sak` writes pixels of `colour` at as `format`: 8-bit for JPEG, BMP, GIF and WebP, and for AVIF and
/// HEIF too, whose encoders take 16-bit pixels but write 8-bit files until a later slice encodes 10 and 12 bits; float
/// narrowed to 16-bit for PNG; TIFF as given.
///
/// Mirrors `rust_sak::image`'s `make_compatible` and the codecs behind it, which are not public;
/// `the_prediction_is_what_rust_sak_writes` fails if the two drift apart.
fn written_depth(format: ImageFormat, colour: ColorType) -> Depth {
    let given = Depth::of(colour);
    match format {
        ImageFormat::Jpeg
        | ImageFormat::Bmp
        | ImageFormat::Gif
        | ImageFormat::WebP
        | ImageFormat::Avif
        | ImageFormat::Heif => Depth { bits: 8, float: false },
        ImageFormat::Png if given.float => Depth { bits: 16, float: false },
        ImageFormat::Png | ImageFormat::Tiff => given,
    }
}

/// Whether `rust-sak` embeds an ICC profile in `format`.
fn holds_profile(format: ImageFormat) -> bool {
    matches!(format, ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Tiff | ImageFormat::WebP)
}

fn hdr_name<S: Serializer>(hdr: &Hdr, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(match hdr {
        Hdr::GainMap => "gainMap",
        Hdr::Pq => "pq",
        Hdr::Hlg => "hlg",
        // `Hdr` is `#[non_exhaustive]`; a kind added later is still HDR, and a front end words it generically.
        _ => "other",
    })
}

fn format_name<S: Serializer>(format: &ImageFormat, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(format.name())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ::image::DynamicImage;
    use rust_sak::image::{encode_writer_with, metadata_bytes, probe_bytes};

    use super::*;
    use crate::image::test_support::{DISPLAY_P3, display_p3, sample_image};

    const EIGHT: Depth = Depth { bits: 8, float: false };
    const SIXTEEN: Depth = Depth { bits: 16, float: false };

    fn picture(pixels: DynamicImage, fidelity: Fidelity, profile: Option<&[u8]>) -> Picture {
        let mut metadata = crate::image::Metadata::default();
        metadata.icc_profile = profile.map(<[u8]>::to_vec);
        Picture::new("/pictures/source.jpg", Arc::new(pixels), "cafebabecafebabe")
            .with_metadata(metadata)
            .with_fidelity(fidelity)
    }

    fn ultra_hdr() -> Picture {
        let mut fidelity = Fidelity::of_colour(ColorType::Rgb8);
        fidelity.hdr = Some(Hdr::GainMap);
        picture(sample_image(), fidelity, Some(DISPLAY_P3))
    }

    #[test]
    fn an_ultra_hdr_jpeg_written_as_jpeg_loses_only_its_hdr() {
        let losses = losses(&ultra_hdr(), ImageFormat::Jpeg);

        assert_eq!(losses, [Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Jpeg }]);
        assert_eq!(losses[0].to_string(), "HDR gain map dropped: JPEG output is SDR only for now");
    }

    #[test]
    fn the_same_photograph_written_as_avif_also_loses_its_profile() {
        let losses: Vec<String> = losses(&ultra_hdr(), ImageFormat::Avif).iter().map(ToString::to_string).collect();

        // The fixture profile names itself tersely; a real Display P3 profile says `Display P3`.
        assert_eq!(
            losses,
            ["HDR gain map dropped: AVIF output is SDR only for now", "sP3 profile not embedded in AVIF"]
        );
    }

    #[test]
    fn a_16_bit_source_written_as_jpeg_loses_depth_and_as_png_or_tiff_does_not() {
        let deep = DynamicImage::ImageRgb16(sample_image().to_rgb16());
        let source = picture(deep, Fidelity::of_colour(ColorType::Rgb16), None);

        let jpeg = losses(&source, ImageFormat::Jpeg);
        assert_eq!(jpeg, [Loss::Depth { from: SIXTEEN, to: EIGHT, format: ImageFormat::Jpeg }]);
        assert_eq!(jpeg[0].to_string(), "16-bit → 8-bit in JPEG");
        assert_eq!(losses(&source, ImageFormat::Png), []);
        assert_eq!(losses(&source, ImageFormat::Tiff), []);
    }

    #[test]
    fn a_float_source_written_as_png_loses_its_floats() {
        let float = DynamicImage::ImageRgb32F(sample_image().to_rgb32f());
        let source = picture(float, Fidelity::of_colour(ColorType::Rgb32F), None);

        let png = losses(&source, ImageFormat::Png);
        assert_eq!(png[0].to_string(), "32-bit float → 16-bit in PNG");
        assert_eq!(losses(&source, ImageFormat::Tiff), []);
    }

    #[test]
    fn a_grayscale_profile_on_a_colourised_picture_is_a_colour_model_loss() {
        let mut gray = DISPLAY_P3.to_vec();
        gray[16..20].copy_from_slice(b"GRAY");
        let source = picture(sample_image(), Fidelity::of_colour(ColorType::L8), Some(&gray));

        let losses = losses(&source, ImageFormat::Png);
        assert!(
            matches!(&losses[..], [Loss::Profile { cause: ProfileCause::ColourModel, colour_space, .. }] if colour_space == "GRAY"),
            "{losses:?}"
        );
        assert_eq!(losses[0].to_string(), "sP3 profile not embedded in PNG: it is for GRAY, not the file's colours");
    }

    #[test]
    fn a_discarded_or_coded_description_is_lost_in_every_format() {
        let mut fidelity = Fidelity::of_colour(ColorType::Rgb8);
        fidelity.discarded =
            Some(crate::image::Discarded { description: Some("SWOP".into()), colour_space: "CMYK".into() });
        fidelity.coded = Some("Display P3".into());
        let source = picture(sample_image(), fidelity, None);

        let words: Vec<String> = losses(&source, ImageFormat::Png).iter().map(ToString::to_string).collect();
        assert_eq!(
            words,
            [
                "SWOP profile dropped: it is for CMYK, not the decoded pixels",
                "Display P3 colour description not written in PNG"
            ]
        );
    }

    #[test]
    fn an_srgb_jpeg_written_as_jpeg_or_png_loses_nothing() {
        let source = picture(sample_image(), Fidelity::of_colour(ColorType::Rgb8), Some(DISPLAY_P3));

        for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Tiff, ImageFormat::WebP] {
            assert_eq!(losses(&source, format), [], "{format:?}");
        }
        // And a picture with no source behind it loses nothing anywhere.
        let bare = Picture::new("/pictures/new.png", Arc::new(sample_image()), "cafebabecafebabe");
        for format in [ImageFormat::Bmp, ImageFormat::Gif, ImageFormat::Avif, ImageFormat::Heif] {
            assert_eq!(losses(&bare, format), [], "{format:?}");
        }
    }

    #[test]
    fn plain_srgb_is_not_lost_in_avif_or_heif_but_display_p3_is() {
        use crate::image::test_support::{rgb_profile_named, srgb_profile};

        let srgb = srgb_profile(0);
        let source = picture(sample_image(), Fidelity::of_colour(ColorType::Rgb8), Some(&srgb));
        for format in [ImageFormat::Avif, ImageFormat::Heif] {
            // An AVIF or HEIF without a profile reads as sRGB: nothing to warn about.
            assert_eq!(losses(&source, format), [], "{format:?}");
        }
        // BMP and GIF are not read as anything in particular, so the profile is still named there.
        assert_eq!(
            losses(&source, ImageFormat::Gif)[0].to_string(),
            "sRGB IEC61966-2.1 profile not embedded in GIF"
        );

        for named in ["Display P3", "Adobe RGB (1998)", "sRGB linear"] {
            let other = rgb_profile_named(named, 0);
            let source = picture(sample_image(), Fidelity::of_colour(ColorType::Rgb8), Some(&other));
            assert_eq!(
                losses(&source, ImageFormat::Avif).iter().map(ToString::to_string).collect::<Vec<_>>(),
                [format!("{named} profile not embedded in AVIF")]
            );
        }
    }

    #[test]
    fn a_profile_the_encoder_left_out_is_reconciled_into_the_losses() {
        use rust_sak::image::Embedded;

        let source = picture(sample_image(), Fidelity::of_colour(ColorType::Rgb8), Some(DISPLAY_P3));
        let mut kept = Embedded::default();
        kept.icc_profile = true;
        let dropped = Embedded::default();

        assert_eq!(reconcile(losses(&source, ImageFormat::Jpeg), &source, ImageFormat::Jpeg, kept), []);
        let reconciled = reconcile(losses(&source, ImageFormat::Jpeg), &source, ImageFormat::Jpeg, dropped);
        assert!(
            matches!(&reconciled[..], [Loss::Profile { cause: ProfileCause::NotWritten, colour_space, .. }] if colour_space == "RGB"),
            "{reconciled:?}"
        );
        assert_eq!(reconciled[0].to_string(), "sP3 profile not embedded in JPEG: the encoder could not write it");
        // A loss already predicted is not counted twice.
        let gif = losses(&source, ImageFormat::Gif);
        assert_eq!(reconcile(gif.clone(), &source, ImageFormat::Gif, dropped), gif);
        // And with no profile there is nothing to reconcile.
        let bare = picture(sample_image(), Fidelity::of_colour(ColorType::Rgb8), None);
        assert_eq!(reconcile(Vec::new(), &bare, ImageFormat::Jpeg, dropped), []);
    }

    #[test]
    fn a_loss_serializes_tagged_by_kind_for_a_front_end_to_word() {
        let hdr = serde_json::to_value(Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Heif }).expect("JSON");
        assert_eq!(hdr, serde_json::json!({ "kind": "hdr", "hdr": "gainMap", "format": "HEIF" }));

        let depth = serde_json::to_value(Loss::Depth { from: SIXTEEN, to: EIGHT, format: ImageFormat::Jpeg });
        assert_eq!(
            depth.expect("JSON"),
            serde_json::json!({
                "kind": "depth",
                "from": { "bits": 16, "float": false },
                "to": { "bits": 8, "float": false },
                "format": "JPEG"
            })
        );

        let profile = Loss::Profile {
            profile: None,
            colour_space: "RGB".into(),
            format: ImageFormat::Gif,
            cause: ProfileCause::Format,
        };
        assert_eq!(profile.to_string(), "ICC profile not embedded in GIF");
        assert_eq!(
            serde_json::to_value(profile).expect("JSON"),
            serde_json::json!({
                "kind": "profile", "profile": null, "colourSpace": "RGB", "format": "GIF", "cause": "format"
            })
        );
    }

    #[test]
    fn the_prediction_is_what_rust_sak_writes() {
        // `written_depth` and `holds_profile` restate what `rust-sak` narrows to and embeds; this encodes through
        // `rust-sak` itself, for every format and every colour layout, with a fitting profile, and asks the written
        // file. A `rust-sak` that changed what it writes fails here rather than mis-warning a user.
        let colour = sample_image();
        let layouts = [
            DynamicImage::ImageLuma8(colour.to_luma8()),
            DynamicImage::ImageLumaA8(colour.to_luma_alpha8()),
            DynamicImage::ImageLuma16(colour.to_luma16()),
            DynamicImage::ImageLumaA16(colour.to_luma_alpha16()),
            colour.clone(),
            DynamicImage::ImageRgba8(colour.to_rgba8()),
            DynamicImage::ImageRgb16(colour.to_rgb16()),
            DynamicImage::ImageRgba16(colour.to_rgba16()),
            DynamicImage::ImageRgb32F(colour.to_rgb32f()),
            DynamicImage::ImageRgba32F(colour.to_rgba32f()),
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
            for pixels in &layouts {
                let mut gray = DISPLAY_P3.to_vec();
                gray[16..20].copy_from_slice(b"GRAY");
                let mut metadata = display_p3();
                if !pixels.color().has_color() {
                    metadata.icc_profile = Some(gray);
                }
                let fitting = crate::image::profile::embeddable(&metadata, pixels, format);
                let mut bytes = Vec::new();
                encode_writer_with(pixels, &mut bytes, format, None, &fitting).expect("encodable");

                let info = probe_bytes(&bytes).expect("probed");
                let written = Depth {
                    bits: info.bit_depth,
                    float: matches!(info.color_type, ColorType::Rgb32F | ColorType::Rgba32F),
                };
                let predicted = written_depth(format, pixels.color());
                if predicted != written {
                    drifted
                        .push(format!("{format:?} from {:?}: predicted {predicted}, wrote {written}", pixels.color()));
                }

                let profile_lost =
                    assess(&Fidelity::of_pixels(pixels), metadata.icc_profile.as_deref(), pixels.color(), format)
                        .iter()
                        .any(|loss| matches!(loss, Loss::Profile { .. }));
                let embedded = metadata_bytes(&bytes).icc_profile.is_some();
                if profile_lost == embedded {
                    drifted.push(format!(
                        "{format:?} from {:?}: predicted the profile {}, the file has {}",
                        pixels.color(),
                        if profile_lost { "lost" } else { "kept" },
                        if embedded { "one" } else { "none" }
                    ));
                }
            }
        }
        assert!(drifted.is_empty(), "the loss prediction disagrees with rust-sak:\n{}", drifted.join("\n"));
    }
}
