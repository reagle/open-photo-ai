//! What writing a picture as a format loses against the file it came from: HDR, bits per channel, the colour profile.
//!
//! One assessment, asked before the file is written, which both front ends show and neither restates. It predicts what
//! `rust-sak` writes — the depth its encode dispatch narrows to, the formats that hold an ICC profile, the colour model
//! the file is written in — and `the_prediction_is_what_rust_sak_writes` holds it to that by encoding every format and
//! colour layout and reading the result back. [`write`](fn@super::write) then reconciles it with what the encoder
//! reports it embedded, which only the write can know: a profile too large for the format
//! ([`ProfileCause::NotWritten`]).
//!
//! Each kind is one place to change when a later slice of the colour series stops losing it: HDR in formats other than
//! JPEG, a chosen depth, profiles in AVIF and HEIF. A gain-map JPEG written as JPEG with its gain map carried loses no
//! HDR (the Ultra HDR slice); one written as plain JPEG leaves it out by choice ([`HdrCause::Chosen`]).

use std::fmt;
use std::path::Path;

use ::image::ColorType;
use serde::{Serialize, Serializer};

use rust_sak::image::Embedded;

use super::format::GainMapChoice;
use crate::image::profile::{colour_space, description, fits, matters_in, writes_colour};
use crate::image::{Depth, Fidelity, GainMap, Hdr, ImageFormat, ImageIoError, Picture, base_orientation, is_raw};

/// One thing a written file lost against its source.
///
/// `Display` gives the English sentence the CLI and the log use (`16-bit → 8-bit in JPEG`); `Serialize` gives a value
/// tagged by `kind` (`hdr`, `depth`, `profile`) whose fields a translated front end words for itself. A format is
/// serialized by its name (`JPEG`, `HEIF`), which is not translated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Loss {
    /// The source is HDR and the file is SDR: only a gain-map JPEG written as JPEG keeps its HDR, by carrying its gain
    /// map.
    Hdr {
        /// How the source declared HDR, serialized as `gainMap`, `pq` or `hlg`.
        #[serde(serialize_with = "hdr_name")]
        hdr: Hdr,
        /// What the file is written as.
        #[serde(serialize_with = "format_name")]
        format: ImageFormat,
        /// Why it is lost.
        cause: HdrCause,
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

/// Why a [`Loss::Hdr`] happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HdrCause {
    /// The format is written as SDR only: every format for PQ and HLG, every format but JPEG for a gain map.
    Format,
    /// A gain-map JPEG written as JPEG whose gain map could not be carried: it could not be read at load, or could not
    /// be prepared at export.
    NotCarried,
    /// A gain-map JPEG written as plain JPEG, which leaves the gain map out by choice ([`GainMapChoice::Drop`]): an SDR
    /// JPEG for the sites and apps that mishandle an Ultra HDR one. Asked for, so a front end words it as what was done
    /// rather than as a warning.
    Chosen,
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
            Self::Hdr { hdr: Hdr::GainMap, cause: HdrCause::NotCarried, .. } => {
                write!(f, "HDR gain map dropped: it could not be carried")
            }
            Self::Hdr { cause: HdrCause::Chosen, .. } => write!(f, "written as SDR (gain map left out)"),
            Self::Hdr { hdr: Hdr::GainMap, format, .. } => {
                write!(f, "HDR gain map dropped: {} output is SDR only for now", format.name())
            }
            Self::Hdr { hdr, format, .. } => {
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
/// when nothing is lost. Asked before the write, from the picture's [`fidelity`](Picture::fidelity),
/// [`metadata`](Picture::metadata) and [`gain_map`](Picture::gain_map), so it costs no encode.
pub fn losses(picture: &Picture, format: ImageFormat) -> Vec<Loss> {
    losses_carrying(picture, format, GainMapChoice::Keep, picture.gain_map().is_some())
}

/// [`losses`] under the gain-map `choice`, with whether the gain map is written given: what
/// [`write`](fn@super::write) answers once it knows.
pub(crate) fn losses_carrying(
    picture: &Picture,
    format: ImageFormat,
    choice: GainMapChoice,
    gain_map: bool,
) -> Vec<Loss> {
    let profile = picture.metadata().icc_profile.as_deref();
    assess(picture.fidelity(), profile, picture.pixels().color(), format, choice, gain_map)
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

/// What writing the file at `path`, once loaded, as `format` will lose against it — [`losses`] for a file not yet
/// loaded, **without decoding its pixels**: its fidelity and profile are read from its bytes as
/// [`load`](fn@crate::image::load) reads them, and the written pixels are taken to be the file's own colour type, which
/// they are at the source's own depth. A camera RAW is judged by its extension, without reading it: it develops to
/// 16-bit sRGB with no profile.
///
/// Whether the gain map will be carried is judged from its MPF image's header, as the load judges it, without decoding
/// it.
///
/// **Only what the prediction needs is read** ([`header`]): a JPEG up to its first scan and a PNG up to its first
/// image data, where every block it reads lies; another format whole. A format change in the export dialog asks again
/// for every queued file, so a 30 MB JPEG costs its few kilobytes of header each time rather than its 30 MB. A gain-map
/// JPEG asked about as JPEG is the exception, read whole: its gain map lies after the primary image.
///
/// A lower bound: it may miss a loss but never names one the write will not have. An enhancement that narrows the
/// depth or changes the colour model is not foreseen (a grayscale photograph colourised loses its `GRAY` profile to the
/// colour model, which is not predicted), nor a profile the encoder cannot write ([`ProfileCause::NotWritten`]), nor a
/// gain map that passes its header check and then fails to decode or encode. The losses [`write`](fn@super::write)
/// answers are what was actually written.
///
/// # Errors
///
/// [`ImageIoError::Read`] where the file cannot be read, and [`ImageIoError::Decode`] where its header is not one of
/// a supported format's. An export of such a file will report its own failure.
pub fn predict_blocking(path: impl AsRef<Path>, format: ImageFormat) -> Result<Vec<Loss>, ImageIoError> {
    predict_export_blocking(path, format, GainMapChoice::Keep).map(|prediction| prediction.losses)
}

/// What [`predict_export_blocking`] foresees of one export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prediction {
    /// What the file will lose against its source, as [`predict_blocking`] gives it.
    pub losses: Vec<Loss>,
    /// Whether the source declares an HDR gain map — an Ultra HDR or ISO 21496-1 JPEG — as
    /// [`hdr_bytes`](rust_sak::image::hdr_bytes) reads its header, whether or not it will be carried: what a front
    /// end names the source by.
    pub gain_map: bool,
}

/// [`predict_blocking`] under the gain-map `choice` — a plain JPEG leaves the gain map out by choice
/// ([`HdrCause::Chosen`]) — with whether the source declares a gain map beside it.
///
/// A gain map left out is not read past the header: only one asked to be carried is judged, as
/// [`predict_blocking`] judges it.
///
/// # Errors
///
/// As [`predict_blocking`].
pub fn predict_export_blocking(
    path: impl AsRef<Path>,
    format: ImageFormat,
    choice: GainMapChoice,
) -> Result<Prediction, ImageIoError> {
    let path = path.as_ref();
    if is_raw(path) {
        let losses = assess(&Fidelity::of_colour(ColorType::Rgb16), None, ColorType::Rgb16, format, choice, false);
        return Ok(Prediction { losses, gain_map: false });
    }

    let mut bytes = header(path).map_err(ImageIoError::read(path))?;
    let gain_map = rust_sak::image::hdr_bytes(&bytes) == Some(Hdr::GainMap);
    // Whether the gain map is carried decides only a gain-map JPEG written as JPEG with it, and it lies after the
    // primary.
    if format == ImageFormat::Jpeg && choice == GainMapChoice::Keep && gain_map {
        bytes = std::fs::read(path).map_err(ImageIoError::read(path))?;
    }
    Ok(Prediction { losses: predict_from(&bytes, path, format, choice)?, gain_map })
}

/// [`predict_export_blocking`]'s losses from the file's `bytes`, as much of them as [`header`] reads.
fn predict_from(
    bytes: &[u8],
    path: &Path,
    format: ImageFormat,
    choice: GainMapChoice,
) -> Result<Vec<Loss>, ImageIoError> {
    let info = rust_sak::image::probe_bytes(bytes).map_err(ImageIoError::decode(path))?;
    let profile = rust_sak::image::icc_profile_bytes(bytes).ok().flatten();
    let fidelity = Fidelity::read_with(Some(&info), bytes, info.color_type, profile.as_deref());
    // The load keeps only a profile that describes the decoded pixels; the one it drops is `fidelity.discarded`.
    let carried = profile.filter(|profile| fits(profile, info.color_type.has_color()));
    // `probe_bytes` reports the base at its upright size, so the gain map is judged turned the same way.
    let metadata = rust_sak::image::metadata_bytes(bytes);
    let orientation = base_orientation(metadata.exif.as_deref());
    let gain_map = metadata
        .gain_map
        .is_some_and(|gain_map| GainMap::plausible(&gain_map, (info.width, info.height), orientation).is_ok());

    Ok(assess(&fidelity, carried.as_deref(), info.color_type, format, choice, gain_map))
}

/// How much more of a file [`header`] reads at a time.
const HEADER_STEP: usize = 64 * 1024;

/// The start of the file at `path`, as far as [`predict_blocking`] needs it: a JPEG through its first `SOS` segment,
/// where every `APPn` block a prediction reads — the ICC profile, the XMP and the ISO 21496-1 and MPF segments that
/// declare a gain map — lies; a PNG through its first `IDAT` chunk header, past which no `iCCP` or `cICP` chunk may come;
/// any other format whole, since a TIFF's directory or an ISO-BMFF `meta` box may lie anywhere. A file that ends sooner,
/// or whose structure breaks off, is answered as far as it goes, which is what reading it whole would have given.
fn header(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;

    // Reads at least `at_least` bytes more, and at least a step; answers whether there were any.
    fn more(file: &mut std::fs::File, bytes: &mut Vec<u8>, at_least: usize) -> std::io::Result<bool> {
        let want = u64::try_from(at_least.max(HEADER_STEP)).unwrap_or(u64::MAX);
        Ok(file.by_ref().take(want).read_to_end(bytes)? > 0)
    }

    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    more(&mut file, &mut bytes, HEADER_STEP)?;
    loop {
        let needed = if bytes.starts_with(&[0xFF, 0xD8]) {
            jpeg_header_end(&bytes)
        } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            png_header_end(&bytes)
        } else {
            None
        };
        match needed {
            // Everything needed is here.
            Some(end) if end <= bytes.len() => return Ok(bytes),
            // A structure runs past what was read: read on, at least to its end, unless the file ends first.
            Some(end) => {
                let missing = end - bytes.len();
                if !more(&mut file, &mut bytes, missing)? {
                    return Ok(bytes);
                }
            }
            // Not a JPEG or a PNG, or one whose structure could not be followed: the whole file.
            None => {
                file.read_to_end(&mut bytes)?;
                return Ok(bytes);
            }
        }
    }
}

/// How many bytes of the JPEG `bytes` hold everything up to and including its first `SOS` segment: possibly more than
/// `bytes` has, when a segment runs past its end. `None` where the markers cannot be followed.
fn jpeg_header_end(bytes: &[u8]) -> Option<usize> {
    let mut at = 2;
    loop {
        // Fill bytes are allowed before a marker.
        while bytes.get(at) == Some(&0xFF) && bytes.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        let (Some(&0xFF), Some(&marker)) = (bytes.get(at), bytes.get(at + 1)) else {
            // Out of bytes, at least the next marker and its length are needed; anything else is no marker.
            return (at + 2 > bytes.len()).then_some(at + 4);
        };
        match marker {
            // No length and no data.
            0x01 | 0xD0..=0xD7 => at += 2,
            // The end of the image before any scan: nothing further is needed.
            0xD9 => return Some(at + 2),
            _ => {
                let Some(length) = bytes.get(at + 2..at + 4) else { return Some(at + 4) };
                let end = at + 2 + usize::from(u16::from_be_bytes([length[0], length[1]]));
                if marker == 0xDA || end > bytes.len() {
                    return Some(end);
                }
                at = end;
            }
        }
    }
}

/// How many bytes of the PNG `bytes` hold everything up to and including its first `IDAT` chunk's length and type:
/// possibly more than `bytes` has. `None` where the chunks cannot be followed.
fn png_header_end(bytes: &[u8]) -> Option<usize> {
    let mut at = 8;
    loop {
        let Some(head) = bytes.get(at..at + 8) else { return Some(at + 8) };
        if &head[4..8] == b"IDAT" || &head[4..8] == b"IEND" {
            return Some(at + 8);
        }
        let length = usize::try_from(u32::from_be_bytes([head[0], head[1], head[2], head[3]])).ok()?;
        // Length, type, data and CRC.
        at = at.checked_add(12)?.checked_add(length)?;
    }
}

/// [`losses`] for pixels of `colour` carrying `profile`, from a source of `fidelity`, under the gain-map `choice`, with
/// or without its `gain_map` carried.
pub(crate) fn assess(
    fidelity: &Fidelity,
    profile: Option<&[u8]>,
    colour: ColorType,
    format: ImageFormat,
    choice: GainMapChoice,
    gain_map: bool,
) -> Vec<Loss> {
    let mut losses = Vec::new();

    // Kept only by a gain-map JPEG written as JPEG with its gain map; PQ and HLG are not written as HDR yet.
    match (fidelity.hdr, format) {
        (None, _) => {}
        (Some(hdr @ Hdr::GainMap), ImageFormat::Jpeg) if choice == GainMapChoice::Drop => {
            losses.push(Loss::Hdr { hdr, format, cause: HdrCause::Chosen });
        }
        (Some(Hdr::GainMap), ImageFormat::Jpeg) if gain_map => {}
        (Some(hdr @ Hdr::GainMap), ImageFormat::Jpeg) => {
            losses.push(Loss::Hdr { hdr, format, cause: HdrCause::NotCarried });
        }
        (Some(hdr), _) => losses.push(Loss::Hdr { hdr, format, cause: HdrCause::Format }),
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
    use ::image::metadata::Orientation;
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

    /// A gain-map JPEG as loaded: HDR by its gain map, with the gain map carried.
    fn ultra_hdr() -> Picture {
        use crate::image::test_support::{FIXTURE_SIZE, gain_map_jpeg};

        let mut fidelity = Fidelity::of_colour(ColorType::Rgb8);
        fidelity.hdr = Some(Hdr::GainMap);
        let gain_map =
            GainMap::read(&gain_map_jpeg(FIXTURE_SIZE, 3), FIXTURE_SIZE, Orientation::NoTransforms).expect("readable");
        picture(sample_image(), fidelity, Some(DISPLAY_P3)).with_gain_map(Some(gain_map))
    }

    #[test]
    fn an_ultra_hdr_jpeg_written_as_jpeg_with_its_gain_map_loses_nothing() {
        assert_eq!(losses(&ultra_hdr(), ImageFormat::Jpeg), []);
    }

    #[test]
    fn an_ultra_hdr_jpeg_whose_gain_map_is_not_carried_loses_its_hdr_in_jpeg_too() {
        let without = ultra_hdr().with_gain_map(None);

        let losses = losses(&without, ImageFormat::Jpeg);

        assert_eq!(
            losses,
            [Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Jpeg, cause: HdrCause::NotCarried }]
        );
        assert_eq!(losses[0].to_string(), "HDR gain map dropped: it could not be carried");
        // And the write's own answer, once a gain map the picture carries could not be prepared.
        assert_eq!(losses_carrying(&ultra_hdr(), ImageFormat::Jpeg, GainMapChoice::Keep, false), losses);
    }

    #[test]
    fn an_ultra_hdr_jpeg_written_as_png_loses_its_hdr_to_the_format() {
        let losses = losses(&ultra_hdr(), ImageFormat::Png);

        assert_eq!(losses, [Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Png, cause: HdrCause::Format }]);
        assert_eq!(losses[0].to_string(), "HDR gain map dropped: PNG output is SDR only for now");
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
        let hdr = Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Heif, cause: HdrCause::Format };
        let hdr = serde_json::to_value(hdr).expect("JSON");
        assert_eq!(hdr, serde_json::json!({ "kind": "hdr", "hdr": "gainMap", "format": "HEIF", "cause": "format" }));
        let not_carried = Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Jpeg, cause: HdrCause::NotCarried };
        assert_eq!(
            serde_json::to_value(not_carried).expect("JSON"),
            serde_json::json!({ "kind": "hdr", "hdr": "gainMap", "format": "JPEG", "cause": "notCarried" })
        );

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
    fn a_prediction_from_the_file_agrees_with_the_assessment_of_the_loaded_picture() {
        use crate::image::test_support::{
            truncated_ultra_hdr, write_fixture, written_jpeg, written_p3, written_turned_ultra_hdr_of,
            written_ultra_hdr, written_ultra_hdr_of,
        };

        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut deep = Vec::new();
        rust_sak::image::encode_writer(
            &DynamicImage::ImageRgb16(sample_image().to_rgb16()),
            &mut deep,
            ImageFormat::Png,
            None,
        )
        .expect("encodable");
        let files = [
            write_fixture(dir.path(), "hawk.jpg", &written_ultra_hdr()),
            write_fixture(dir.path(), "deep.png", &deep),
            write_fixture(dir.path(), "plain.jpg", &written_jpeg()),
            write_fixture(dir.path(), "p3.jpg", &written_p3(ImageFormat::Jpeg)),
            write_fixture(dir.path(), "cut.jpg", &truncated_ultra_hdr()),
            write_fixture(dir.path(), "large.jpg", &written_ultra_hdr_of((32, 24), (32, 25), 1)),
            write_fixture(dir.path(), "quarter.jpg", &written_ultra_hdr_of((40, 32), (10, 8), 1)),
            // A full-resolution gain map stored on its side, which fits the upright base only once turned with it.
            write_fixture(dir.path(), "turned.jpg", &written_turned_ultra_hdr_of((32, 24), (32, 24))),
        ];

        for path in &files {
            let loaded = crate::image::load_blocking(path).expect("decodable");
            for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Avif, ImageFormat::Gif] {
                assert_eq!(
                    predict_blocking(path, format).expect("readable"),
                    losses(&loaded, format),
                    "{} as {format:?}",
                    path.display()
                );
            }
        }
        let hdr = |format, cause| [Loss::Hdr { hdr: Hdr::GainMap, format, cause }];
        assert_eq!(predict_blocking(&files[0], ImageFormat::Jpeg).expect("readable"), []);
        assert_eq!(
            predict_blocking(&files[0], ImageFormat::Png).expect("readable"),
            hdr(ImageFormat::Png, HdrCause::Format)
        );
        assert_eq!(predict_blocking(&files[2], ImageFormat::Jpeg).expect("readable"), []);
        for cut in [&files[4], &files[5]] {
            assert_eq!(
                predict_blocking(cut, ImageFormat::Jpeg).expect("readable"),
                hdr(ImageFormat::Jpeg, HdrCause::NotCarried),
                "{}",
                cut.display()
            );
        }
        for carried in [&files[6], &files[7]] {
            assert_eq!(predict_blocking(carried, ImageFormat::Jpeg).expect("readable"), [], "{}", carried.display());
        }
    }

    #[test]
    fn a_gain_map_jpeg_asked_about_as_plain_jpeg_is_foreseen_as_sdr_by_choice_and_named_by_its_gain_map() {
        use crate::image::test_support::{truncated_ultra_hdr, write_fixture, written_jpeg, written_ultra_hdr};

        let dir = tempfile::tempdir().expect("a temporary directory");
        let hawk = write_fixture(dir.path(), "hawk.jpg", &written_ultra_hdr());
        let cut = write_fixture(dir.path(), "cut.jpg", &truncated_ultra_hdr());
        let plain = write_fixture(dir.path(), "plain.jpg", &written_jpeg());
        let chosen = [Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Jpeg, cause: HdrCause::Chosen }];

        // Left out by choice, a gain map is no longer judged: even one that could not be carried is simply left out.
        for path in [&hawk, &cut] {
            let prediction = predict_export_blocking(path, ImageFormat::Jpeg, GainMapChoice::Drop).expect("readable");
            assert_eq!(prediction, Prediction { losses: chosen.to_vec(), gain_map: true }, "{}", path.display());
            let loaded = crate::image::load_blocking(path).expect("decodable");
            assert_eq!(losses_carrying(&loaded, ImageFormat::Jpeg, GainMapChoice::Drop, false), chosen);
        }
        assert_eq!(chosen[0].to_string(), "written as SDR (gain map left out)");
        assert_eq!(
            serde_json::to_value(&chosen[0]).expect("serializes"),
            serde_json::json!({ "kind": "hdr", "hdr": "gainMap", "format": "JPEG", "cause": "chosen" })
        );

        // Kept, it is what `predict_blocking` foresees; and another format loses it to the format, choice or none.
        let kept = predict_export_blocking(&hawk, ImageFormat::Jpeg, GainMapChoice::Keep).expect("readable");
        assert_eq!(kept, Prediction { losses: Vec::new(), gain_map: true });
        assert_eq!(
            predict_export_blocking(&hawk, ImageFormat::Png, GainMapChoice::Drop).expect("readable").losses,
            [Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Png, cause: HdrCause::Format }]
        );

        // A JPEG without one has nothing to leave out, and is not named by one.
        let sdr = predict_export_blocking(&plain, ImageFormat::Jpeg, GainMapChoice::Drop).expect("readable");
        assert_eq!(sdr, Prediction { losses: Vec::new(), gain_map: false });
    }

    #[test]
    fn a_prediction_reads_a_jpegs_or_pngs_header_only_and_predicts_what_the_whole_file_would() {
        use crate::image::test_support::{srgb_profile, write_fixture, written_jpeg, written_p3, written_ultra_hdr};

        let dir = tempfile::tempdir().expect("a temporary directory");
        // Large enough that the pixels dwarf the header: noise does not compress.
        let mut seed = 7u32;
        let noise = DynamicImage::ImageRgb8(::image::RgbImage::from_fn(640, 480, |_, _| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let [a, b, c, _] = seed.to_be_bytes();
            ::image::Rgb([a, b, c])
        }));
        let encode = |pixels: &DynamicImage, format, profile: Option<Vec<u8>>| {
            let mut metadata = rust_sak::image::Metadata::default();
            metadata.icc_profile = profile;
            let mut bytes = Vec::new();
            encode_writer_with(pixels, &mut bytes, format, None, &metadata).expect("encodable");
            bytes
        };
        let deep = DynamicImage::ImageRgb16(noise.to_rgb16());
        let large = [
            // A profile across many `APP2` segments, and one `iCCP` chunk, each larger than a read step.
            write_fixture(dir.path(), "large.jpg", &encode(&noise, ImageFormat::Jpeg, Some(srgb_profile(300_000)))),
            write_fixture(dir.path(), "large.png", &encode(&deep, ImageFormat::Png, Some(srgb_profile(300_000)))),
            write_fixture(dir.path(), "p3.png", &encode(&deep, ImageFormat::Png, Some(DISPLAY_P3.to_vec()))),
            write_fixture(dir.path(), "plain.jpg", &encode(&noise, ImageFormat::Jpeg, None)),
        ];
        let small = [
            write_fixture(dir.path(), "hawk.jpg", &written_ultra_hdr()),
            write_fixture(dir.path(), "tiny.jpg", &written_jpeg()),
            write_fixture(dir.path(), "p3.jpg", &written_p3(ImageFormat::Jpeg)),
            write_fixture(dir.path(), "gray.png", &encode(&noise.grayscale(), ImageFormat::Png, None)),
            write_fixture(dir.path(), "deep.tif", &encode(&deep, ImageFormat::Tiff, Some(DISPLAY_P3.to_vec()))),
            write_fixture(dir.path(), "noise.webp", &encode(&noise, ImageFormat::WebP, Some(DISPLAY_P3.to_vec()))),
        ];

        for path in large.iter().chain(&small) {
            let whole = std::fs::read(path).expect("readable");
            let read = header(path).expect("readable");
            assert_eq!(read[..], whole[..read.len()], "a prefix of the file");
            if large.contains(path) {
                assert!(read.len() * 2 < whole.len(), "{}: {} of {} bytes", path.display(), read.len(), whole.len());
            }
            let formats = [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Avif, ImageFormat::Gif, ImageFormat::Tiff];
            for format in formats {
                assert_eq!(
                    predict_blocking(path, format).expect("readable"),
                    predict_from(&whole, path, format, GainMapChoice::Keep).expect("readable"),
                    "{} as {format:?}",
                    path.display()
                );
            }
        }
        // The headers carried what the prediction needs.
        let words = |path, format| -> Vec<String> {
            predict_blocking(path, format).expect("readable").iter().map(ToString::to_string).collect()
        };
        assert_eq!(words(&large[1], ImageFormat::Jpeg), ["16-bit → 8-bit in JPEG"]);
        assert_eq!(words(&large[2], ImageFormat::Gif), ["16-bit → 8-bit in GIF", "sP3 profile not embedded in GIF"]);
        assert_eq!(words(&large[0], ImageFormat::Gif), ["sRGB IEC61966-2.1 profile not embedded in GIF"]);
    }

    #[test]
    fn a_header_that_breaks_off_is_read_as_far_as_it_goes() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let jpeg = crate::image::test_support::written_jpeg();
        for cut in [3, 6, 40, jpeg.len() / 2] {
            let path = crate::image::test_support::write_fixture(dir.path(), "cut.jpg", &jpeg[..cut]);
            assert_eq!(header(&path).expect("readable"), jpeg[..cut], "cut at {cut}");
        }
        // A marker where none can be: the rest is read whole, as before.
        let mut odd = jpeg.clone();
        odd[2] = 0x00;
        let path = crate::image::test_support::write_fixture(dir.path(), "odd.jpg", &odd);
        assert_eq!(header(&path).expect("readable"), odd);
    }

    #[test]
    fn a_prediction_for_a_file_that_is_missing_or_not_an_image_is_an_error() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let text = crate::image::test_support::write_fixture(dir.path(), "notes.jpg", b"not an image");

        assert!(matches!(
            predict_blocking(dir.path().join("missing.jpg"), ImageFormat::Jpeg),
            Err(ImageIoError::Read { .. })
        ));
        assert!(matches!(predict_blocking(&text, ImageFormat::Jpeg), Err(ImageIoError::Decode { .. })));
        // A RAW is judged by its name, without being read.
        assert_eq!(
            predict_blocking(dir.path().join("DSC_0001.nef"), ImageFormat::Jpeg).expect("not read"),
            [Loss::Depth { from: SIXTEEN, to: EIGHT, format: ImageFormat::Jpeg }]
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

                let profile_lost = assess(
                    &Fidelity::of_pixels(pixels),
                    metadata.icc_profile.as_deref(),
                    pixels.color(),
                    format,
                    GainMapChoice::Keep,
                    false,
                )
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
