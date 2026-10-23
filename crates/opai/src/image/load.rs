//! Reading an image file into memory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ::image::DynamicImage;
use rust_sak::crypto::xxh3_bytes;
use rust_sak::image::{Hdr, Metadata, decode_bytes, decode_raw_bytes, metadata_bytes};

use super::error::{self, ImageIoError};
use super::fidelity::Fidelity;
use super::gain_map::GainMap;
use super::picture::Picture;
use super::raw;
use crate::task::spawn_blocking;
use crate::telemetry::unit::{Unit, unit_span};

/// Reads and decodes the image at `path`, without occupying the thread driving the caller.
///
/// See [`load_blocking`] for what it does; the two report the same result and the same errors.
///
/// # Errors
///
/// Everything [`load_blocking`] returns, plus [`ImageIoError::Cancelled`] if the runtime shut down before the read
/// could run.
pub async fn load(path: impl Into<PathBuf>) -> Result<Picture, ImageIoError> {
    let path = path.into();
    spawn_blocking::<_, ImageIoError, _>(move || load_blocking(&path)).await?
}

/// Reads and decodes the image at `path`, on the calling thread.
///
/// The result carries the pixels, the path they came from, the XXH3-64 of the whole of the file's bytes, and the
/// metadata the file holds beside its pixels — its ICC colour profile, EXIF, XMP and IPTC — each byte for byte. See
/// [`Picture`] for what that identity promises.
///
/// # The EXIF, XMP and IPTC
///
/// Read from the same buffer as the pixels and the identity, for JPEG (EXIF, XMP with any extended XMP joined in, and
/// the `APP13` Photoshop block that holds IPTC), PNG and WebP (EXIF and XMP). Nothing is interpreted on the way in. A
/// malformed block is left out and the rest kept; it never fails the load. TIFF, AVIF, HEIF, BMP, GIF and camera RAW
/// carry none yet. Nothing is written back out yet either: see [`without_descriptive`](super::without_descriptive).
///
/// # The gain map
///
/// An Ultra HDR / ISO 21496-1 / Adobe gain-map JPEG's gain map — the MPF secondary image `rust-sak` locates — is decoded
/// and attached as the picture's [`gain_map`](Picture::gain_map), with its own `hdrgm` XMP and ISO 21496-1 metadata. The
/// encoded bytes are taken out of the picture's metadata, so the stale image never reaches an export. A gain map that
/// cannot be located, is not a JPEG, does not decode, or is larger than the photograph or smaller than a sixteenth of
/// it, is dropped and the drop is logged; the photograph still loads, and an export reports the HDR it could not carry.
///
/// # The colour profile
///
/// Read from the same buffer as the pixels, for every format that can carry one (JPEG, PNG, TIFF, WebP, AVIF, HEIF).
/// **No colour is converted**: the pixels are exactly what they would be without it, and the profile is the label that
/// says what they mean. A file with no profile, or only coded (`nclx`) colour values, has none; so does a developed RAW,
/// whose pixels are sRGB. A profile for a different colour model than the decoded pixels — a CMYK profile on a JPEG
/// the decoder turned into RGB — no longer describes them, so it is dropped and the drop is logged.
///
/// The format is read from the content's magic bytes, not from the file's name, so a JPEG saved as `.png` decodes as
/// the JPEG it is. Camera RAW is the exception and routes on the extension; see [`is_raw`](super::is_raw).
///
/// # What a developed RAW is
///
/// 16 bits per channel, sRGB, demosaiced and tone-mapped, then cropped and oriented as the camera recorded — the
/// photograph rather than the sensor readings. Every format behind [`save_blocking`](super::save_blocking) writes it;
/// the four that cannot hold 16 bits narrow it rather than refusing, so which ones keep the depth is what the module
/// docs spell out.
///
/// # Errors
///
/// - [`ImageIoError::UnsupportedRaw`] if the extension names one of the recognised camera RAW formats that has no
///   decoder. Checked before the file is opened, so a RAW on a disconnected drive names its format rather than an
///   I/O failure.
/// - [`ImageIoError::Read`] if the file does not exist or cannot be read.
/// - [`ImageIoError::Decode`] if its content matches no supported format, or the codec refuses it — including a RAW
///   file the decoder cannot develop, and a RAW-named file whose content is not RAW at all.
pub fn load_blocking(path: impl AsRef<Path>) -> Result<Picture, ImageIoError> {
    let path = path.as_ref();

    let span = unit_span!("image_load", path = %path.display());
    error::traced(Unit::ImageLoad, span, || read(path).inspect_err(error::unreadable("load", path)))
}

/// [`load_blocking`]'s body, which leaves recording its failure to the public boundary.
fn read(path: &Path) -> Result<Picture, ImageIoError> {
    // Which decoder this file goes to, decided from its name before the file is even opened; see `raw::route`.
    let raw_extension = raw::route(path)?;

    let bytes = std::fs::read(path).map_err(ImageIoError::read(path))?;

    // Identity and pixels both from the same buffer, which is the whole reason it is held: the hash covers every byte,
    // including any the decoder stops before, and so matches `xxh3_file` of this same file without a second pass. The
    // reference reaches the same property through a `TeeReader` into a pipe feeding the hasher plus a drain loop,
    // purely to avoid holding a 60 MB RAW in memory; the buffer here is small beside the decoded image that is about
    // to exist anyway.
    let identity = xxh3_bytes(&bytes);

    // RAW decodes from the bytes rather than from the path for the same reason: `rust-sak` offers both, and the
    // path-taking form would read a 60 MB NEF a second time or cost the identity the property above.
    //
    // Everything `decode_raw_bytes` can refuse — a camera the backend does not know, a corrupt file, or a RAW-named
    // file that is not RAW — is "read it, could not decode it", which is what `Decode` already means for a corrupt
    // JPEG. It never reports an I/O failure, having been handed bytes rather than a path.
    let decoded = if raw_extension.is_some() { decode_raw_bytes(&bytes) } else { decode_bytes(&bytes) };
    let pixels = decoded.map_err(ImageIoError::decode(path))?;

    // A developed RAW is sRGB whatever the camera embedded, so it carries no profile; its EXIF and XMP are a later
    // slice's, read from the RAW container and its sidecar. Its fidelity is its pixels': 16-bit, SDR.
    let (mut metadata, fidelity) = if raw_extension.is_none() {
        metadata_of(path, &bytes, &pixels)
    } else {
        (Metadata::default(), Fidelity::of_pixels(&pixels))
    };
    let gain_map = gain_map_of(path, metadata.gain_map.take(), &fidelity, &pixels);

    Ok(Picture::new(path, Arc::new(pixels), identity)
        .with_metadata(metadata)
        .with_fidelity(fidelity)
        .with_gain_map(gain_map))
}

/// The gain map `encoded` decoded for a base image of `pixels`; `None`, and the drop logged, if the file declares a gain
/// map (`fidelity`) that could not be located or read.
fn gain_map_of(path: &Path, encoded: Option<Vec<u8>>, fidelity: &Fidelity, pixels: &DynamicImage) -> Option<GainMap> {
    let warn = |error: &dyn std::fmt::Display| {
        tracing::warn!(op = "load", path = %path.display(), error = %error, "HDR gain map dropped: it could not be read");
    };
    match encoded {
        Some(encoded) => {
            GainMap::read(&encoded, (pixels.width(), pixels.height())).inspect_err(|error| warn(error)).ok()
        }
        None => {
            if fidelity.hdr == Some(Hdr::GainMap) {
                warn(&"the file declares one, and no gain-map image was found where its MPF index says");
            }
            None
        }
    }
}

/// Everything `bytes` holds beside its pixels, with the ICC profile left out if it does not describe `pixels`, and what
/// of it an export can lose.
///
/// Never a reason to refuse the photograph: `metadata_bytes` answers a block it cannot read with `None`, and
/// [`Fidelity::read`] reads what it cannot as nothing to lose.
fn metadata_of(path: &Path, bytes: &[u8], pixels: &DynamicImage) -> (Metadata, Fidelity) {
    let mut metadata = metadata_bytes(bytes);
    let fidelity = Fidelity::read(bytes, pixels.color(), metadata.icc_profile.as_deref());
    metadata.icc_profile = metadata.icc_profile.take().and_then(|profile| fitting(path, profile, pixels));
    (metadata, fidelity)
}

/// `profile`, if it describes `pixels`; otherwise `None`, and the drop is logged.
fn fitting(path: &Path, profile: Vec<u8>, pixels: &DynamicImage) -> Option<Vec<u8>> {
    if super::profile::describes(&profile, pixels) {
        return Some(profile);
    }

    tracing::warn!(
        op = "load",
        path = %path.display(),
        profile = super::profile::description(&profile).unwrap_or_default(),
        colour_space = super::profile::colour_space(&profile).unwrap_or_default(),
        "colour profile dropped: it does not describe the decoded pixels"
    );
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::test_support::{
        BLUE, DISPLAY_P3, DNG_FIXTURE_SIZE, FIXTURE_SIZE, GREEN, RED, channel_means, minimal_dng, on_a_dead_runtime,
        write_fixture, written_dng, written_jpeg, written_p3, written_png,
    };

    use rust_sak::crypto::xxh3_file;
    use rust_sak::image::{ImageError, ImageFormat};
    use tempfile::tempdir;

    #[test]
    fn a_supported_image_is_decoded_with_its_path_and_its_identity() {
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "holiday.png", &written_png());

        let source = load_blocking(&path).expect("a PNG this crate wrote is decodable");

        assert_eq!(source.path(), path);
        assert_eq!(source.dimensions(), FIXTURE_SIZE);
        assert!(!source.identity().is_empty());
    }

    #[test]
    fn the_identity_is_the_same_value_as_hashing_the_file_without_decoding_it() {
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "holiday.png", &written_png());

        let source = load_blocking(&path).expect("decodable");

        assert_eq!(source.identity(), xxh3_file(&path).expect("the file is readable"));
    }

    #[test]
    fn the_identity_covers_bytes_the_decoder_never_reads() {
        // A decoder stops at the end of the image data; trailing metadata or padding after it is never read. The hash
        // still has to cover it, or a file listing and a loaded image disagree about a file neither of them changed.
        let dir = tempdir().expect("a temporary directory");
        let mut padded = written_png();
        padded.extend_from_slice(b"trailing bytes the PNG decoder stops before");
        let path = write_fixture(dir.path(), "padded.png", &padded);

        let source = load_blocking(&path).expect("the trailing bytes do not stop the decode");

        assert_eq!(source.identity(), xxh3_file(&path).expect("readable"));
        assert_ne!(source.identity(), xxh3_bytes(&written_png()), "the trailing bytes were not hashed");
    }

    #[test]
    fn two_files_with_the_same_bytes_and_different_names_share_one_identity() {
        let dir = tempdir().expect("a temporary directory");
        let bytes = written_png();
        let first = load_blocking(write_fixture(dir.path(), "first.png", &bytes)).expect("decodable");
        let second = load_blocking(write_fixture(dir.path(), "second.png", &bytes)).expect("decodable");

        assert_eq!(first.identity(), second.identity());
        assert_ne!(first.path(), second.path());
    }

    #[test]
    fn the_content_decides_the_format_rather_than_the_extension() {
        // A JPEG under a `.png` name. The Go application routes on content for the same reason, and a user who
        // renamed a file should still see their photograph.
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "mislabelled.png", &written_jpeg());

        let source = load_blocking(&path).expect("the JPEG content decodes despite the PNG name");

        assert_eq!(source.dimensions(), FIXTURE_SIZE);
    }

    #[test]
    fn a_file_that_is_not_an_image_is_refused_and_names_itself() {
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "notes.png", b"this is not an image, whatever it is called");

        let error = load_blocking(&path).expect_err("plain text is not an image");

        assert!(matches!(error, ImageIoError::Decode { .. }), "expected a decode failure, got {error:?}");
        assert!(error.to_string().contains("notes.png"), "the message did not name the file: {error}");
    }

    #[test]
    fn a_missing_file_is_refused_as_unreadable_rather_than_undecodable() {
        // Two different things to tell a user — "this file is not there" and "this file is not a photograph" — so the
        // missing file must not arrive as a decode failure.
        let dir = tempdir().expect("a temporary directory");
        let path = dir.path().join("never-written.png");

        let error = load_blocking(&path).expect_err("a file that was never written cannot be read");

        assert!(matches!(error, ImageIoError::Read { .. }), "expected a read failure, got {error:?}");
        assert!(error.to_string().contains("never-written.png"), "the message did not name the file: {error}");
    }

    #[test]
    fn a_raw_file_is_decoded_as_raw_rather_than_as_the_tiff_it_resembles() {
        // A DNG is a TIFF container, so a content sniffer would route it to the TIFF decoder, which would succeed
        // and hand back the CFA mosaic or an embedded preview. What comes back instead is the developed photograph:
        // 16-bit RGB at the sensor's size, which the TIFF path could not produce from this file.
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "DSC_0001.dng", &written_dng());

        let source = load_blocking(&path).expect("a DNG this crate wrote is decodable");

        assert_eq!(source.path(), path);
        assert_eq!(source.dimensions(), DNG_FIXTURE_SIZE);
        assert!(
            source.pixels().as_rgb16().is_some(),
            "a developed RAW is 16-bit RGB, got {:?}",
            source.pixels().color()
        );
        assert_eq!(source.identity(), xxh3_file(&path).expect("the file is readable"));
    }

    #[test]
    fn the_raw_fixture_develops_to_the_picture_it_encodes() {
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "red.dng", &written_dng());

        let source = load_blocking(&path).expect("decodable");

        let [red, green, blue] = channel_means(source.pixels());
        assert!(
            red > green && red > blue,
            "an RGGB sensor with its red photosites bright must develop to a red-dominant picture, \
             got r={red:.0} g={green:.0} b={blue:.0}"
        );
    }

    #[test]
    fn transposing_the_channel_order_moves_the_dominant_channel() {
        // What proves the assertion above has teeth. The same builder and the same bright photosites, differing only
        // in which colour the CFA says they are — so a decode that ignored the CFA, or a route that never reached the
        // RAW decoder at all, would produce two identical pictures and fail here.
        let dir = tempdir().expect("a temporary directory");
        let rggb = write_fixture(dir.path(), "rggb.dng", &minimal_dng(64, 64, [RED, GREEN, GREEN, BLUE], RED));
        let bggr = write_fixture(dir.path(), "bggr.dng", &minimal_dng(64, 64, [BLUE, GREEN, GREEN, RED], BLUE));

        let [red_r, _, red_b] = channel_means(load_blocking(&rggb).expect("decodable").pixels());
        let [blue_r, _, blue_b] = channel_means(load_blocking(&bggr).expect("decodable").pixels());

        assert!(red_r > red_b, "RGGB with bright red should be red-dominant");
        assert!(blue_b > blue_r, "BGGR with bright blue should be blue-dominant");
        // And stated the other way round: the red fixture's assertion applied to the transposed file fails.
        assert!(blue_r <= blue_b, "the transposed fixture must not also be red-dominant");
    }

    #[test]
    fn a_raw_format_with_no_decoder_is_refused_by_its_extension_before_its_content_is_looked_at() {
        // The file holds a genuine, decodable TIFF, which is exactly what a RAW file looks like to a content sniffer.
        // A `.gpr` must still be refused rather than decoded as that TIFF: what came back would be the mosaic or an
        // embedded preview, a wrong image reported as a success.
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "HERO0001.gpr", &crate::image::test_support::written_tiff());

        let error = load_blocking(&path).expect_err("GoPro RAW has no decoder in this backend");

        match error {
            ImageIoError::UnsupportedRaw { extension, .. } => assert_eq!(extension, "gpr"),
            other => panic!("expected an unsupported-RAW refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_decodable_raw_name_holding_something_that_is_not_raw_is_a_decode_failure() {
        // The distinction the two errors carry: `UnsupportedRaw` is about the *format*, so a NEF — a format that is
        // decoded — can only fail on its content, and does so the way a corrupt JPEG does.
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "DSC_0001.nef", &written_png());

        let error = load_blocking(&path).expect_err("PNG bytes are not a RAW file");

        match error {
            ImageIoError::Decode { source, .. } => {
                assert!(matches!(source, ImageError::NotRaw), "expected NotRaw, got {source:?}");
            }
            other => panic!("expected a decode failure, got {other:?}"),
        }
        let message = load_blocking(&path).expect_err("still not RAW").to_string();
        assert!(message.contains("DSC_0001.nef"), "the message did not name the file: {message}");
    }

    #[test]
    fn a_raw_file_the_decoder_refuses_is_a_decode_failure_rather_than_an_unsupported_format() {
        // A corrupt file and a camera the backend does not know arrive as one error from `zenraw`, which cannot tell
        // them apart. Either way it must not be reported as the format being unsupported: the next DNG in the same
        // folder opens fine, and telling a photographer DNG is unsupported would be wrong about both files.
        let dir = tempdir().expect("a temporary directory");
        let mut corrupt = written_dng();
        corrupt.truncate(corrupt.len() / 2);
        let path = write_fixture(dir.path(), "truncated.dng", &corrupt);

        let error = load_blocking(&path).expect_err("half a DNG cannot be developed");

        match error {
            ImageIoError::Decode { source, .. } => {
                assert!(matches!(source, ImageError::Raw(_)), "expected the RAW decoder's own words, got {source:?}");
            }
            other => panic!("expected a decode failure, got {other:?}"),
        }
    }

    #[test]
    fn a_genuine_tiff_still_loads() {
        // The other half of the RAW check: it must refuse NEFs without refusing the format they imitate.
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "scan.tiff", &crate::image::test_support::written_tiff());

        let source = load_blocking(&path).expect("a real TIFF is decoded normally");

        assert_eq!(source.dimensions(), FIXTURE_SIZE);
    }

    #[test]
    fn an_undecodable_raw_is_refused_before_it_is_read_at_all() {
        // Refused on the name alone, so a `.gpr` on a disconnected drive reports the format it cannot open rather
        // than an I/O failure that sends the user looking at their hardware. The directory does not exist, so a
        // check that ran after the read would report `Read` here and the user would never learn which it was.
        let error =
            load_blocking("/nowhere-at-all/HERO0001.gpr").expect_err("a RAW under a missing directory is still RAW");

        assert!(matches!(error, ImageIoError::UnsupportedRaw { .. }), "expected a RAW refusal, got {error:?}");

        // The other half of the ordering: a *decodable* RAW under the same missing directory has to be read before
        // anything can be said about it, so it is the read that fails.
        let readable =
            load_blocking("/nowhere-at-all/DSC_0001.dng").expect_err("a missing file cannot be decoded either");
        assert!(matches!(readable, ImageIoError::Read { .. }), "expected a read failure, got {readable:?}");
    }

    #[tokio::test]
    async fn a_load_through_the_async_form_is_one_span_in_its_callers_trace() {
        use tracing::Instrument as _;

        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "holiday.png", &written_png());

        let (log, observed, loaded) = crate::logging::traced_of("debug", || async {
            load(&path).instrument(tracing::info_span!("caller")).await
        })
        .await;
        loaded.expect("decodable");

        // Across the blocking hop, under the caller.
        let caller = observed.only("caller");
        let span = observed.only("image_load");
        assert_eq!(span.parent, Some(caller.index));
        assert_eq!(span.field("path"), Some(path.display().to_string().as_str()));
        assert_eq!(span.field("outcome"), Some("finished"));
        assert!(!log.contains(" outcome="), "the unit span reached the file: {log}");
    }

    #[tokio::test]
    async fn a_load_that_fails_marks_its_span_failed() {
        let dir = tempdir().expect("a temporary directory");
        let path = dir.path().join("never-written.png");

        let (_, observed, loaded) = crate::logging::traced_of("info", || async { load(&path).await }).await;
        assert!(matches!(loaded, Err(ImageIoError::Read { .. })), "{loaded:?}");

        let span = observed.only("image_load");
        assert_eq!(span.field("outcome"), Some("failed"));
        assert!(span.field("error").is_some_and(|error| error.contains("never-written.png")), "{span:?}");
        assert_eq!(observed.event("an image could not be read").and_then(|event| event.span), Some(span.index));
    }

    #[tokio::test]
    async fn the_asynchronous_form_reports_what_the_blocking_one_does() {
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "holiday.png", &written_png());

        let asynchronous = load(&path).await.expect("decodable");
        let blocking = load_blocking(&path).expect("decodable");

        assert_eq!(asynchronous.path(), blocking.path());
        assert_eq!(asynchronous.identity(), blocking.identity());
        assert_eq!(asynchronous.pixels(), blocking.pixels());
    }

    #[tokio::test]
    async fn the_asynchronous_form_reports_a_missing_file_the_way_the_blocking_one_does() {
        let dir = tempdir().expect("a temporary directory");
        let path = dir.path().join("never-written.png");

        let error = load(&path).await.expect_err("a file that was never written cannot be read");

        assert!(matches!(error, ImageIoError::Read { .. }), "expected a read failure, got {error:?}");
    }

    #[test]
    fn a_runtime_that_shuts_down_before_the_read_runs_reports_cancellation() {
        let outcome = on_a_dead_runtime(load("/pictures/holiday.png"));

        match outcome {
            Err(ImageIoError::Cancelled) => {}
            Err(other) => panic!("expected cancellation, got {other:?}"),
            Ok(_) => panic!("the read ran against a runtime that was already gone"),
        }
    }

    #[test]
    fn a_failure_is_recorded_once_by_either_form_and_a_success_not_at_all() {
        use crate::image::test_log::{as_written, recorded_async, the_one_failure};
        use crate::logging::records_of_blocking;

        let dir = tempdir().expect("a temporary directory");
        let not_an_image = write_fixture(dir.path(), "notes.png", b"this is not an image");

        let (log, outcome) = records_of_blocking("info", || load_blocking(&not_an_image));
        assert!(outcome.is_err());
        let record = the_one_failure(&log, "an image could not be read", "load");
        assert!(record.contains(&format!("path={}", as_written(&not_an_image))), "{record}");

        let (log, outcome) = recorded_async(super::load(not_an_image.clone()));
        assert!(outcome.is_err());
        the_one_failure(&log, "an image could not be read", "load");

        let readable = write_fixture(dir.path(), "holiday.png", &written_png());
        let (log, outcome) = records_of_blocking("info", || load_blocking(&readable));
        assert!(outcome.is_ok());
        assert!(!log.contains("msg="), "a success was recorded:\n{log}");
    }

    #[test]
    fn a_display_p3_jpeg_keeps_its_profile_byte_for_byte_and_its_pixels_unconverted() {
        let dir = tempdir().expect("a temporary directory");
        let tagged =
            load_blocking(write_fixture(dir.path(), "p3.jpg", &written_p3(ImageFormat::Jpeg))).expect("decodable");
        let plain = load_blocking(write_fixture(dir.path(), "plain.jpg", &written_jpeg())).expect("decodable");

        assert_eq!(tagged.metadata().icc_profile.as_deref(), Some(DISPLAY_P3));
        // The profile is a label: the same encoder with and without it decodes to the same values.
        assert_eq!(tagged.pixels(), plain.pixels(), "reading the profile changed the pixels");
        assert_eq!(plain.metadata().icc_profile, None);
    }

    #[test]
    fn every_format_that_can_carry_a_profile_reports_it() {
        let dir = tempdir().expect("a temporary directory");
        for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Tiff, ImageFormat::WebP] {
            let path = write_fixture(dir.path(), &format!("p3.{}", format.extension()), &written_p3(format));
            let loaded = load_blocking(&path).unwrap_or_else(|err| panic!("{format:?}: {err}"));
            assert_eq!(loaded.metadata().icc_profile.as_deref(), Some(DISPLAY_P3), "{format:?}");
        }
    }

    #[test]
    fn a_png_without_a_profile_reports_none() {
        let dir = tempdir().expect("a temporary directory");
        let loaded = load_blocking(write_fixture(dir.path(), "plain.png", &written_png())).expect("decodable");

        assert_eq!(loaded.metadata(), &Metadata::default());
    }

    #[test]
    fn a_developed_raw_reports_no_profile() {
        let dir = tempdir().expect("a temporary directory");
        let loaded = load_blocking(write_fixture(dir.path(), "DSC_0001.dng", &written_dng())).expect("decodable");

        assert_eq!(loaded.metadata().icc_profile, None);
    }

    #[test]
    fn a_profile_for_another_colour_model_is_dropped_and_the_drop_is_logged() {
        use crate::logging::{field, records, records_of_blocking};

        // A CMYK profile on an RGB JPEG: the decoder hands back RGB, which the profile does not describe.
        let mut cmyk = DISPLAY_P3.to_vec();
        cmyk[16..20].copy_from_slice(b"CMYK");
        let mut metadata = Metadata::default();
        metadata.icc_profile = Some(cmyk);
        let mut bytes = Vec::new();
        rust_sak::image::encode_writer_with(
            &crate::image::test_support::sample_image(),
            &mut bytes,
            ImageFormat::Jpeg,
            None,
            &metadata,
        )
        .expect("encodable");
        let dir = tempdir().expect("a temporary directory");
        let path = write_fixture(dir.path(), "cmyk-profile.jpg", &bytes);

        let (log, loaded) = records_of_blocking("info", || load_blocking(&path));
        let loaded = loaded.expect("the photograph still loads");

        assert_eq!(loaded.metadata().icc_profile, None);
        let message = "colour profile dropped: it does not describe the decoded pixels";
        let [record] = records(&log, message)[..] else { panic!("expected one record:\n{log}") };
        assert_eq!(field(record, "colour_space"), Some("CMYK"), "{record}");
        assert!(record.contains("level=WARN"), "{record}");
    }

    #[test]
    fn a_jpeg_carries_its_exif_xmp_and_iptc_byte_for_byte_beside_its_profile() {
        use crate::image::test_support::{EXIF, IPTC, XMP, written_described};

        let dir = tempdir().expect("a temporary directory");
        let bytes = written_described(ImageFormat::Jpeg);
        let path = write_fixture(dir.path(), "described.jpg", &bytes);

        let loaded = load_blocking(&path).expect("decodable");

        let metadata = loaded.metadata();
        assert_eq!(metadata.exif.as_deref(), Some(EXIF));
        assert_eq!(metadata.xmp.as_deref(), Some(XMP));
        assert_eq!(metadata.iptc.as_deref(), Some(IPTC));
        assert_eq!(metadata.icc_profile.as_deref(), Some(DISPLAY_P3));
        // The identity still hashes the whole file, blocks and all.
        assert_eq!(loaded.identity(), xxh3_file(&path).expect("readable"));
    }

    #[test]
    fn png_and_webp_carry_their_exif_and_xmp() {
        use crate::image::test_support::{EXIF, XMP, written_described};

        let dir = tempdir().expect("a temporary directory");
        for format in [ImageFormat::Png, ImageFormat::WebP] {
            let path =
                write_fixture(dir.path(), &format!("described.{}", format.extension()), &written_described(format));
            let loaded = load_blocking(&path).unwrap_or_else(|err| panic!("{format:?}: {err}"));

            assert_eq!(loaded.metadata().exif.as_deref(), Some(EXIF), "{format:?}");
            assert_eq!(loaded.metadata().xmp.as_deref(), Some(XMP), "{format:?}");
            assert_eq!(loaded.metadata().iptc, None, "{format:?}");
        }
    }

    #[test]
    fn a_corrupt_exif_block_is_left_out_and_the_photograph_still_loads() {
        use crate::image::test_support::{XMP, described, sample_image};

        // An EXIF block cut inside its first directory, written as a JPEG would carry it.
        let mut metadata = described();
        metadata.exif = Some(crate::image::test_support::EXIF[..12].to_vec());
        let mut bytes = Vec::new();
        rust_sak::image::encode_writer_with(&sample_image(), &mut bytes, ImageFormat::Jpeg, None, &metadata)
            .expect("encodable");
        let dir = tempdir().expect("a temporary directory");

        let loaded = load_blocking(write_fixture(dir.path(), "corrupt-exif.jpg", &bytes)).expect("still decodable");

        assert_eq!(loaded.metadata().exif, None);
        assert_eq!(loaded.metadata().xmp.as_deref(), Some(XMP));
        assert_eq!(loaded.dimensions(), FIXTURE_SIZE);
    }

    #[test]
    fn a_file_without_metadata_and_a_raw_carry_none() {
        let dir = tempdir().expect("a temporary directory");
        let plain = load_blocking(write_fixture(dir.path(), "plain.png", &written_png())).expect("decodable");
        let raw = load_blocking(write_fixture(dir.path(), "DSC_0001.dng", &written_dng())).expect("decodable");

        assert_eq!(plain.metadata(), &Metadata::default());
        assert_eq!(raw.metadata(), &Metadata::default());
    }

    #[test]
    fn an_ultra_hdr_jpeg_is_recorded_as_8_bit_hdr_with_a_gain_map() {
        use crate::image::test_support::written_ultra_hdr;
        use rust_sak::image::Hdr;

        let dir = tempdir().expect("a temporary directory");
        let loaded = load_blocking(write_fixture(dir.path(), "hawk.jpg", &written_ultra_hdr())).expect("decodable");

        assert_eq!(loaded.fidelity().hdr, Some(Hdr::GainMap));
        assert_eq!(loaded.fidelity().depth, crate::image::Depth { bits: 8, float: false });
        assert_eq!(loaded.fidelity().discarded, None);
        assert_eq!(loaded.metadata().icc_profile.as_deref(), Some(DISPLAY_P3));
    }

    #[test]
    fn an_ultra_hdr_jpeg_keeps_its_gain_map_with_its_own_metadata_and_not_the_encoded_bytes() {
        use crate::image::test_support::{
            HAWK_GAIN_MAP_ISO, HAWK_GAIN_MAP_XMP, ISO_VERSION, written_ultra_hdr, written_ultra_hdr_of,
        };

        let dir = tempdir().expect("a temporary directory");
        let (log, loaded) = crate::logging::records_of_blocking("info", || {
            load_blocking(write_fixture(dir.path(), "hawk.jpg", &written_ultra_hdr()))
        });
        let loaded = loaded.expect("decodable");

        let gain_map = loaded.gain_map().expect("the gain map is carried");
        assert_eq!((gain_map.pixels().width(), gain_map.pixels().height()), FIXTURE_SIZE);
        assert_eq!(gain_map.pixels().color(), ::image::ColorType::Rgb8, "three channels, as stored");
        assert_eq!(gain_map.scale(), (1.0, 1.0));
        assert_eq!(gain_map.metadata().xmp.as_deref(), Some(HAWK_GAIN_MAP_XMP));
        assert_eq!(gain_map.metadata().iso_gain_map.as_deref(), Some(HAWK_GAIN_MAP_ISO));
        // The encoded image is not left in the picture's metadata; the primary's version marker is.
        assert_eq!(loaded.metadata().gain_map, None);
        assert_eq!(loaded.metadata().iso_gain_map.as_deref(), Some(ISO_VERSION));
        assert!(crate::logging::records(&log, "HDR gain map dropped: it could not be read").is_empty(), "{log}");

        // A quarter-resolution gray one.
        let bytes = written_ultra_hdr_of((40, 32), (10, 8), 1);
        let loaded = load_blocking(write_fixture(dir.path(), "quarter.jpg", &bytes)).expect("decodable");
        let gain_map = loaded.gain_map().expect("carried");
        assert_eq!((gain_map.pixels().width(), gain_map.pixels().height(), gain_map.scale()), (10, 8, (0.25, 0.25)));
        assert_eq!(gain_map.pixels().color(), ::image::ColorType::L8);
    }

    #[test]
    fn a_gain_map_an_editor_moved_without_updating_its_offset_is_still_carried() {
        use crate::image::test_support::written_ultra_hdr;

        // As ExifTool leaves an Ultra HDR JPEG it added a comment to: a `COM` after the MPF index, its offset stale.
        let bytes = written_ultra_hdr();
        let index_end = bytes.windows(4).position(|w| w == b"MPF\0").expect("an MPF index") + 86;
        let comment = [&[0xFF, 0xFE, 0x00, 0x0B][..], b"fresh run"].concat();
        let edited = [&bytes[..index_end], &comment, &bytes[index_end..]].concat();
        let dir = tempdir().expect("a temporary directory");

        let loaded = load_blocking(write_fixture(dir.path(), "edited.jpg", &edited)).expect("decodable");

        let gain_map = loaded.gain_map().expect("found after the primary's EOI");
        assert_eq!((gain_map.pixels().width(), gain_map.pixels().height()), FIXTURE_SIZE);
    }

    #[test]
    fn a_gain_map_that_cannot_be_read_is_dropped_with_a_warning_and_the_photograph_still_loads() {
        use crate::image::test_support::{truncated_ultra_hdr, written_ultra_hdr_of};
        use crate::logging::{field, records, records_of_blocking};

        let dir = tempdir().expect("a temporary directory");
        // Cut inside the gain map, so the MPF index points past the end of the file; and a gain map larger than the
        // photograph it belongs to.
        let cut = write_fixture(dir.path(), "cut.jpg", &truncated_ultra_hdr());
        let large = write_fixture(dir.path(), "large.jpg", &written_ultra_hdr_of((32, 24), (32, 25), 1));

        for (path, why) in [(&cut, "no gain-map image was found"), (&large, "does not fit")] {
            let (log, loaded) = records_of_blocking("info", || load_blocking(path));
            let loaded = loaded.expect("the photograph still loads");

            assert!(loaded.gain_map().is_none(), "{}", path.display());
            assert_eq!(loaded.fidelity().hdr, Some(rust_sak::image::Hdr::GainMap), "still recorded as HDR");
            assert_eq!(loaded.dimensions(), FIXTURE_SIZE);
            let [record] = records(&log, "HDR gain map dropped: it could not be read")[..] else {
                panic!("expected one record:\n{log}")
            };
            assert!(record.contains("level=WARN"), "{record}");
            assert!(field(record, "error").is_some_and(|error| error.contains(why)), "{record}");
        }
    }

    #[test]
    fn a_16_bit_png_is_recorded_as_16_bit_sdr_and_a_raw_as_its_developed_16_bits() {
        let pixels = DynamicImage::ImageRgb16(crate::image::test_support::sample_image().to_rgb16());
        let mut png = Vec::new();
        rust_sak::image::encode_writer(&pixels, &mut png, ImageFormat::Png, None).expect("encodable");
        let dir = tempdir().expect("a temporary directory");

        for name in ["deep.png", "DSC_0001.dng"] {
            let bytes = if name.ends_with("png") { png.clone() } else { written_dng() };
            let loaded = load_blocking(write_fixture(dir.path(), name, &bytes)).expect("decodable");

            assert_eq!(loaded.fidelity().depth, crate::image::Depth { bits: 16, float: false }, "{name}");
            assert_eq!(loaded.fidelity().hdr, None, "{name}");
        }
    }

    #[test]
    fn a_truncated_mpf_segment_loads_and_is_not_hdr() {
        use crate::image::test_support::with_app2;

        let bytes = with_app2(&written_jpeg(), b"MPF\0MM\0\x2a\0\0\0\x08\0");
        let dir = tempdir().expect("a temporary directory");

        let loaded = load_blocking(write_fixture(dir.path(), "cut.jpg", &bytes)).expect("still decodable");

        assert_eq!(loaded.fidelity().hdr, None);
        assert_eq!(loaded.dimensions(), FIXTURE_SIZE);
    }

    #[test]
    fn a_cmyk_profile_the_load_drops_is_recorded_as_discarded() {
        let mut cmyk = DISPLAY_P3.to_vec();
        cmyk[16..20].copy_from_slice(b"CMYK");
        let mut metadata = Metadata::default();
        metadata.icc_profile = Some(cmyk);
        let mut bytes = Vec::new();
        rust_sak::image::encode_writer_with(
            &crate::image::test_support::sample_image(),
            &mut bytes,
            ImageFormat::Jpeg,
            None,
            &metadata,
        )
        .expect("encodable");
        let dir = tempdir().expect("a temporary directory");

        let loaded = load_blocking(write_fixture(dir.path(), "cmyk.jpg", &bytes)).expect("decodable");

        assert_eq!(loaded.metadata().icc_profile, None);
        assert_eq!(loaded.fidelity().discarded.as_ref().map(|d| d.colour_space.as_str()), Some("CMYK"));
    }
}
