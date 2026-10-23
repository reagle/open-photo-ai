//! Writing an exported photograph: the destination claimed, the picture saved into it, and the claim kept — the three
//! steps every front end takes, in the one order that leaves nothing behind when a step fails.

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rust_sak::image::EncodeOptions;

use super::claim::claim;
use rust_sak::image::Embedded;

use super::gain_map::prepare;
use super::loss::{Loss, losses_carrying, reconcile};
use crate::image::{self, ImageFormat, ImageIoError, Picture, recommended_encode_options};
use crate::task::{Cancelled, spawn_blocking};

/// The largest XMP packet a JPEG holds in its standard `APP1` segment; a larger one `rust-sak` moves whole into extended
/// XMP, leaving only a reference behind (`rust_sak::image`'s `MAX_STANDARD_LEN`, which is not public).
const MAX_STANDARD_XMP: usize = 65_502;

/// How [`write()`] writes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WriteOptions {
    /// The encoder quality, or `None` for the format's default — see
    /// [`crate::image::recommended_encode_options`].
    pub quality: Option<f64>,
    /// Replace a file already at the destination once the new one is complete, rather than numbering the result
    /// `_1`, `_2`, … beside it.
    pub overwrite: bool,
    /// Create the destination's directory first where it is missing, rather than failing.
    pub create_parent: bool,
}

/// What [`write()`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// Where it went: the destination, or the numbered name beside it.
    pub path: PathBuf,
    /// How many bytes it wrote.
    pub bytes: u64,
    /// What the file lost against the picture's source — HDR, bits per channel, the colour profile — in the order
    /// [`losses`](super::losses) gives them; empty when nothing was lost. Each was also logged as a warning.
    pub losses: Vec<Loss>,
}

/// Why [`write()`] wrote nothing.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The destination's directory could not be created, or no name could be held or kept. The filesystem's error
    /// names no path, so the destination asked for is named here.
    #[error("cannot write {}: {source}", destination.display())]
    Unwritable {
        /// The destination asked for.
        destination: PathBuf,
        /// What the filesystem refused.
        #[source]
        source: io::Error,
    },
    /// The picture could not be encoded or saved. The library's message names the file.
    #[error(transparent)]
    Save(#[from] ImageIoError),
}

impl From<Cancelled> for WriteError {
    fn from(cancelled: Cancelled) -> Self {
        Self::Save(cancelled.into())
    }
}

/// Writes `picture` to `destination` as `format`, at the library's recommended settings for it and `options`' quality,
/// embedding its source's ICC colour profile, from the picture's [`metadata`](Picture::metadata), where `format` can
/// hold it.
///
/// **The source's EXIF, XMP and IPTC are written too, rewritten to be true of the file**: Open Photo AI as the software,
/// the export's time and offset, the written size, orientation 1 for a camera RAW (whose pixels were turned on decode),
/// no thumbnail; a new XMP identity derived from the source's, an Open Photo AI history event, no Camera Raw settings;
/// everything else — capture time, camera, lens, exposure, location, keywords, rating, the MakerNote where it can be
/// kept without moving it — as it was, and of the IPTC block only the IPTC-IIM record and its digest (in a PNG or WebP,
/// which cannot hold it, its values go into the XMP). A block the format cannot hold is left out with a warning.
///
/// **A gain-map JPEG written as JPEG stays an Ultra HDR JPEG**: the picture's [`gain_map`](Picture::gain_map) is
/// written back as the source's own gain-map JPEG, byte for byte, where no crop reframed it and the written image does
/// not shrink it; otherwise it is encoded at its own resolution (shrunk only where the written image is smaller than it,
/// never enlarged) at the export's quality or 90, whichever is higher, with its own `hdrgm` XMP and ISO 21496-1
/// metadata. It is appended after the primary with an MPF index; the primary keeps its `hdrgm` XMP, its
/// `Container:Directory` (with the gain map's new length) and its ISO 21496-1 version marker. Every other format leaves
/// the gain map out. **Nothing about the gain map fails the export**: one that cannot be prepared, or whose primary XMP
/// is too large for a JPEG's standard XMP segment (where `hdrgm:Version` must be), is left out, the primary is written as
/// SDR without its gain-map XMP, and the HDR it could not carry is reported.
///
/// **What the file loses against the picture's source is assessed as it is written** ([`losses`](super::losses)):
/// HDR written as SDR, fewer bits per channel, a colour profile not embedded — **reconciled with what the encoder
/// reports it embedded**, so a profile it could not write (one too large for the format), or a gain map it did not
/// append, is a loss too. Each loss is logged as a warning and answered on [`Written::losses`]; none of them stops the
/// write.
///
/// Without [`overwrite`](WriteOptions::overwrite) a file already there is kept and the result takes the first free
/// `_1` … `_999` name beside it; with it, the file is replaced once the new one is complete. **A write that does not
/// finish leaves nothing behind**: the name claimed is given back, and a file being replaced is untouched.
///
/// The filesystem work is done off the async runtime. Every failure is recorded once: a failed save where it happened,
/// and a destination that could not be claimed or kept here.
///
/// # Errors
///
/// [`WriteError`].
pub async fn write(
    picture: &Picture,
    destination: &Path,
    format: ImageFormat,
    options: WriteOptions,
) -> Result<Written, WriteError> {
    let pixels = picture.shared_pixels();
    let encode = recommended_encode_options(format, options.quality);
    let gain_map = gain_map_for(picture, format, encode).await?;
    // The source photograph's metadata, carried through every crop and enhancement by `Picture::derive`, as the export
    // policy rewrites it for these pixels and this moment; `image::save` warns about whatever the format cannot hold.
    let now = chrono::Local::now().fixed_offset();
    let mut metadata = super::metadata::for_export(picture, format, now, gain_map.as_ref().map(Vec::len));
    // A primary whose XMP could not be rewritten has no `hdrgm:Version`, which a Google-style reader needs to find the
    // gain map, unless it has the ISO version marker; without either the gain map is not written.
    let undeclared = picture.metadata().xmp.is_some() && metadata.xmp.is_none() && metadata.iso_gain_map.is_none();
    // Nor where the XMP is too large for a JPEG's standard XMP segment: `rust-sak` would move all of it, `hdrgm:Version`
    // and the `Container:Directory` too, into extended XMP, where Ultra HDR readers do not look, and the file would
    // claim its HDR where no reader sees it. Its XMP is then rewritten as for an SDR export, without the gain map's.
    let oversized = metadata.xmp.as_ref().is_some_and(|xmp| xmp.len() > MAX_STANDARD_XMP);
    if gain_map.is_some() && undeclared {
        dropped(picture, &"the XMP that declares it could not be rewritten");
    } else if gain_map.is_some() && oversized {
        dropped(picture, &"the XMP that declares it is too large for a JPEG's standard XMP segment");
        metadata = super::metadata::for_export(picture, format, now, None);
    } else {
        metadata.gain_map = gain_map;
    }
    let carrying = metadata.gain_map.is_some();
    let metadata = Arc::new(metadata);

    let (mut written, embedded) =
        write_with(destination, options, |path| image::save_embedding(pixels, metadata, path, format, Some(encode)))
            .await?;
    // What the encoder actually wrote, which no prediction can know: a gain map it did not append, a profile too large
    // for the format.
    if carrying && !embedded.gain_map {
        dropped(picture, &"the encoder did not append it");
    }
    let predicted = losses_carrying(picture, format, carrying && embedded.gain_map);
    let losses = reconcile(predicted, picture, format, embedded);
    for loss in &losses {
        tracing::warn!(op = "export", path = %written.path.display(), kind = loss.kind(), %loss, "an export lost something against its source");
    }
    written.losses = losses;
    Ok(written)
}

/// The gain-map JPEG to append to `picture` written as `format` with `encode`: only for a JPEG of a picture that carries
/// a gain map, prepared off the async runtime; `None`, logged, where it could not be prepared.
async fn gain_map_for(
    picture: &Picture,
    format: ImageFormat,
    encode: EncodeOptions,
) -> Result<Option<Vec<u8>>, WriteError> {
    let (ImageFormat::Jpeg, Some(gain_map), EncodeOptions::Jpeg { quality }) =
        (format, picture.shared_gain_map(), encode)
    else {
        return Ok(None);
    };
    let written = picture.dimensions();
    let prepared = spawn_blocking::<_, WriteError, _>(move || prepare(&gain_map, written, quality)).await?;
    Ok(prepared.inspect_err(|reason| dropped(picture, reason)).ok())
}

/// Logs that `picture`'s gain map is not written, and why.
fn dropped(picture: &Picture, reason: &dyn std::fmt::Display) {
    tracing::warn!(op = "export", path = %picture.path().display(), error = %reason, "HDR gain map dropped: it could not be carried");
}

/// [`write`] with the encoding left to `save`, which is handed the path to write into and answers the bytes written and
/// what it embedded, which is answered beside what was written.
async fn write_with<F>(
    destination: &Path,
    options: WriteOptions,
    save: impl FnOnce(PathBuf) -> F,
) -> Result<(Written, Embedded), WriteError>
where
    F: Future<Output = Result<(u64, Embedded), ImageIoError>>,
{
    let unwritable = |source: io::Error| {
        tracing::warn!(op = "export", path = %destination.display(), %source, "an export could not be written");

        WriteError::Unwritable { destination: destination.to_path_buf(), source }
    };

    let requested = destination.to_path_buf();
    let (create_parent, overwrite) = (options.create_parent, options.overwrite);
    // Up to a thousand exclusive creates, on whatever drive the user chose.
    let claim = spawn_blocking::<_, WriteError, _>(move || {
        if create_parent && let Some(parent) = requested.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }

        claim(&requested, overwrite)
    })
    .await?
    .map_err(unwritable)?;

    // A failure drops the claim, which gives its name back.
    let (bytes, embedded) = save(claim.path().to_path_buf()).await?;

    // Overwriting, this is where the destination is replaced, and not before: a failed save above left it untouched.
    let path = spawn_blocking::<_, WriteError, _>(move || claim.commit()).await?.map_err(unwritable)?;

    Ok((Written { path, bytes, losses: Vec::new() }, embedded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> Picture {
        Picture::new("source.png", Arc::new(::image::DynamicImage::new_rgb8(4, 3)), "0123456789abcdef")
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("listed")
            .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn the_picture_is_written_and_its_bytes_answered() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("out.png");

        let written = write(&picture(), &out, ImageFormat::Png, WriteOptions::default()).await.expect("written");

        assert_eq!(written.path, out);
        assert_eq!(written.bytes, std::fs::metadata(&out).expect("on disk").len());
        assert!(image::probe_blocking(&out).is_ok(), "the file is an image");
    }

    #[tokio::test]
    async fn an_existing_file_is_kept_and_the_result_numbered() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("out.png");
        std::fs::write(&out, "earlier").expect("written");

        let written = write(&picture(), &out, ImageFormat::Png, WriteOptions::default()).await.expect("written");

        assert_eq!(written.path, dir.path().join("out_1.png"));
        assert_eq!(std::fs::read(&out).expect("kept"), b"earlier");
    }

    #[tokio::test]
    async fn overwrite_replaces_the_file_and_leaves_nothing_beside_it() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("out.png");
        std::fs::write(&out, "earlier").expect("written");
        let options = WriteOptions { overwrite: true, ..Default::default() };

        let written = write(&picture(), &out, ImageFormat::Png, options).await.expect("written");

        assert_eq!(written.path, out);
        assert!(image::probe_blocking(&out).is_ok(), "the file was replaced by an image");
        assert_eq!(names(dir.path()), ["out.png"]);
    }

    #[tokio::test]
    async fn a_missing_directory_is_unwritable_naming_the_destination() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("missing/out.png");

        let error = write(&picture(), &out, ImageFormat::Png, WriteOptions::default())
            .await
            .expect_err("no directory");

        assert!(matches!(&error, WriteError::Unwritable { destination, .. } if *destination == out), "{error:?}");
        assert!(error.to_string().starts_with(&format!("cannot write {}: ", out.display())), "{error}");
        assert!(!dir.path().join("missing").exists());
    }

    #[tokio::test]
    async fn create_parent_creates_the_missing_directory() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let out = dir.path().join("2024/06/out.png");
        let options = WriteOptions { create_parent: true, ..Default::default() };

        let written = write(&picture(), &out, ImageFormat::Png, options).await.expect("written");

        assert_eq!(written.path, out);
    }

    #[tokio::test]
    async fn a_failed_save_leaves_nothing_behind() {
        for overwrite in [false, true] {
            let dir = tempfile::tempdir().expect("a temporary directory");
            let out = dir.path().join("out.png");
            std::fs::write(&out, "earlier").expect("written");
            let options = WriteOptions { overwrite, ..Default::default() };

            let error = write_with(&out, options, async |path: PathBuf| {
                std::fs::write(&path, "half").expect("the claim is writable");
                Err(ImageIoError::Write { path, source: io::Error::other("the encoder failed") })
            })
            .await
            .expect_err("the save failed");

            assert!(matches!(error, WriteError::Save(_)), "{error:?}");
            assert_eq!(std::fs::read(&out).expect("kept"), b"earlier", "overwrite: {overwrite}");
            assert_eq!(names(dir.path()), ["out.png"], "overwrite: {overwrite}");
        }
    }

    #[tokio::test]
    async fn the_exif_xmp_and_iptc_are_written_and_the_profile_is_kept() {
        use crate::image::test_support::{DISPLAY_P3, IPTC, written_described};

        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("described.jpg");
        std::fs::write(&source, written_described(ImageFormat::Jpeg)).expect("written");
        let loaded = image::load_blocking(&source).expect("decodable");
        assert!(
            loaded.metadata().exif.is_some() && loaded.metadata().xmp.is_some() && loaded.metadata().iptc.is_some()
        );

        for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP] {
            let out = dir.path().join(format!("out.{}", format.extension()));
            let written = write(&loaded, &out, format, WriteOptions::default()).await.expect("written");

            let read = rust_sak::image::metadata_bytes(&std::fs::read(&written.path).expect("readable"));
            let software = read
                .exif
                .as_deref()
                .and_then(|exif| rust_sak::image::exif_field(exif, rust_sak::image::ExifIfd::Primary, 0x0131));
            assert!(
                matches!(&software, Some(rust_sak::image::ExifValue::Ascii(name)) if name.starts_with("Open Photo AI ")),
                "{format:?}: {software:?}"
            );
            // IPTC has a place only in JPEG; the XMP, rewritten, in all three.
            assert_eq!(read.iptc.as_deref(), (format == ImageFormat::Jpeg).then_some(IPTC), "{format:?}");
            let xmp = String::from_utf8(read.xmp.expect("XMP")).expect("UTF-8");
            assert!(xmp.contains("<rdf:li>hawk</rdf:li>") && xmp.contains("Open Photo AI"), "{format:?}: {xmp}");
            assert_eq!(read.icc_profile.as_deref(), Some(DISPLAY_P3), "{format:?}");
        }
    }

    /// The MP Entries of the JPEG `file`'s MPF index, big-endian as `rust-sak` writes it: `(size, file offset)` of the
    /// primary and of the gain map.
    fn mpf_entries(file: &[u8]) -> [(usize, usize); 2] {
        let at = file.windows(4).position(|w| w == b"MPF\0").expect("an MPF index") + 4;
        let u32_at =
            |offset: usize| u32::from_be_bytes(file[offset..offset + 4].try_into().expect("four bytes")) as usize;
        let table = at + 50;
        let offset = |entry: usize| match u32_at(table + 16 * entry + 8) {
            0 => 0,
            relative => at + relative,
        };
        [(u32_at(table + 4), offset(0)), (u32_at(table + 20), offset(1))]
    }

    /// Whether the JPEG `file`'s XMP, extended XMP merged, still holds the 70,000-character `pad:Pad`.
    fn read_xmp_kept_padding(file: &[u8]) -> bool {
        let xmp = rust_sak::image::metadata_bytes(file).xmp.unwrap_or_default();
        String::from_utf8_lossy(&xmp).contains(&"x".repeat(70_000))
    }

    /// Writes `picture` as `format` into `dir` and answers what was written and the file's bytes.
    async fn write_to(picture: &Picture, dir: &Path, name: &str, format: ImageFormat) -> (Written, Vec<u8>) {
        let written = write(picture, &dir.join(name), format, WriteOptions::default()).await.expect("written");
        let bytes = std::fs::read(&written.path).expect("readable");
        (written, bytes)
    }

    #[tokio::test]
    async fn a_profile_too_large_for_jpeg_is_reported_as_lost_as_the_file_is_written() {
        use crate::image::test_support::{sample_image, srgb_profile};

        // 16,740,000 bytes: a PNG holds it, but a JPEG's 255 `APP2` segments cannot, and `rust-sak` leaves it out.
        let dir = tempfile::tempdir().expect("a temporary directory");
        let mut metadata = rust_sak::image::Metadata::default();
        metadata.icc_profile = Some(srgb_profile(16_740_000));
        let source = dir.path().join("big.png");
        image::save_blocking(&sample_image(), &metadata, &source, ImageFormat::Png, None).expect("written");
        let loaded = image::load_blocking(&source).expect("decodable");
        assert_eq!(super::super::losses(&loaded, ImageFormat::Jpeg), [], "the prediction cannot know");

        let written = write(&loaded, &dir.path().join("out.jpg"), ImageFormat::Jpeg, WriteOptions::default())
            .await
            .expect("written");
        let read = rust_sak::image::metadata_bytes(&std::fs::read(&written.path).expect("readable"));
        assert_eq!(read.icc_profile, None);
        let words: Vec<String> = written.losses.iter().map(ToString::to_string).collect();
        assert_eq!(words, ["sRGB IEC61966-2.1 profile not embedded in JPEG: the encoder could not write it"]);

        // As PNG it is kept, and nothing is lost.
        let written = write(&loaded, &dir.path().join("out.png"), ImageFormat::Png, WriteOptions::default())
            .await
            .expect("written");
        assert_eq!(written.losses, []);
    }

    #[tokio::test]
    async fn an_ultra_hdr_jpeg_written_as_jpeg_stays_ultra_hdr_and_reports_no_loss() {
        use crate::image::Hdr;
        use crate::image::test_support::{
            FIXTURE_SIZE, HAWK_GAIN_MAP_ISO, HAWK_GAIN_MAP_XMP, ISO_VERSION, written_jpeg, written_ultra_hdr,
        };

        let dir = tempfile::tempdir().expect("a temporary directory");
        let hawk = dir.path().join("hawk.jpg");
        std::fs::write(&hawk, written_ultra_hdr()).expect("written");
        let loaded = image::load_blocking(&hawk).expect("decodable");

        let (written, file) = write_to(&loaded, dir.path(), "out.jpg", ImageFormat::Jpeg).await;

        assert_eq!(written.losses, []);
        assert_eq!(rust_sak::image::hdr_bytes(&file), Some(Hdr::GainMap));
        let read = rust_sak::image::metadata_bytes(&file);
        assert_eq!(read.iso_gain_map.as_deref(), Some(ISO_VERSION), "the primary's version marker is kept");
        let xmp = String::from_utf8(read.xmp.expect("XMP")).expect("UTF-8");
        assert!(xmp.contains("hdrgm:Version") && xmp.contains("http://ns.adobe.com/hdr-gain-map/1.0/"), "{xmp}");
        let gain_map = read.gain_map.expect("the gain map is written");
        let decoded = rust_sak::image::decode_bytes(&gain_map).expect("the gain map decodes");
        assert_eq!((decoded.width(), decoded.height()), FIXTURE_SIZE);
        let own = rust_sak::image::metadata_bytes(&gain_map);
        assert_eq!(own.xmp.as_deref(), Some(HAWK_GAIN_MAP_XMP));
        assert_eq!(own.iso_gain_map.as_deref(), Some(HAWK_GAIN_MAP_ISO));
        // The index locates both images exactly.
        let [(primary, zero), (size, offset)] = mpf_entries(&file);
        assert_eq!((primary, zero), (file.len() - gain_map.len(), 0));
        assert_eq!((size, offset), (gain_map.len(), primary));
        assert_eq!(&file[offset..], &gain_map[..]);

        // An SDR JPEG has nothing to carry and loses nothing.
        let plain = dir.path().join("plain.jpg");
        std::fs::write(&plain, written_jpeg()).expect("written");
        let loaded = image::load_blocking(&plain).expect("decodable");
        let (written, file) = write_to(&loaded, dir.path(), "plain-out.jpg", ImageFormat::Jpeg).await;
        assert_eq!(written.losses, []);
        assert_eq!(rust_sak::image::metadata_bytes(&file).gain_map, None);
    }

    #[tokio::test]
    async fn an_upscaled_ultra_hdr_jpeg_keeps_its_gain_map_at_the_sources_size() {
        use crate::image::test_support::{FIXTURE_SIZE, written_ultra_hdr};

        let dir = tempfile::tempdir().expect("a temporary directory");
        let hawk = dir.path().join("hawk.jpg");
        std::fs::write(&hawk, written_ultra_hdr()).expect("written");
        let loaded = image::load_blocking(&hawk).expect("decodable");
        let (width, height) = FIXTURE_SIZE;
        let doubled = loaded.pixels().resize_exact(width * 2, height * 2, ::image::imageops::FilterType::Triangle);
        let upscaled = loaded.derive(doubled, "0123456789abcdef");

        let (written, file) = write_to(&upscaled, dir.path(), "up.jpg", ImageFormat::Jpeg).await;

        assert_eq!(written.losses, []);
        let base = rust_sak::image::decode_bytes(&file).expect("decodable");
        assert_eq!((base.width(), base.height()), (width * 2, height * 2));
        let gain_map = rust_sak::image::metadata_bytes(&file).gain_map.expect("written");
        let decoded = rust_sak::image::decode_bytes(&gain_map).expect("decodable");
        assert_eq!((decoded.width(), decoded.height()), FIXTURE_SIZE, "never enlarged");
        let [_, (size, offset)] = mpf_entries(&file);
        assert_eq!((size, &file[offset..offset + 2]), (gain_map.len(), &[0xFF, 0xD8][..]));
    }

    #[tokio::test]
    async fn png_and_webp_exports_have_neither_the_gain_map_nor_its_xmp() {
        use crate::image::Hdr;
        use crate::image::test_support::written_ultra_hdr;

        let dir = tempfile::tempdir().expect("a temporary directory");
        let hawk = dir.path().join("hawk.jpg");
        std::fs::write(&hawk, written_ultra_hdr()).expect("written");
        let loaded = image::load_blocking(&hawk).expect("decodable");

        for format in [ImageFormat::Png, ImageFormat::WebP] {
            let (written, file) = write_to(&loaded, dir.path(), &format!("out.{}", format.extension()), format).await;

            assert_eq!(
                written.losses,
                [Loss::Hdr { hdr: Hdr::GainMap, format, cause: super::super::HdrCause::Format }]
            );
            assert_eq!(rust_sak::image::hdr_bytes(&file), None, "{format:?}");
            let read = rust_sak::image::metadata_bytes(&file);
            assert_eq!((read.gain_map, read.iso_gain_map), (None, None), "{format:?}");
            let xmp = String::from_utf8(read.xmp.expect("XMP")).expect("UTF-8");
            assert!(!xmp.contains("hdrgm"), "{format:?}: {xmp}");
        }
    }

    #[tokio::test]
    async fn exporting_an_export_writes_the_gain_map_back_byte_for_byte() {
        use crate::image::test_support::written_ultra_hdr;

        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = written_ultra_hdr();
        let original = rust_sak::image::metadata_bytes(&source).gain_map.expect("a gain map");
        let hawk = dir.path().join("hawk.jpg");
        std::fs::write(&hawk, &source).expect("written");

        let mut picture = image::load_blocking(&hawk).expect("decodable");
        for pass in 1..=2 {
            let (written, file) = write_to(&picture, dir.path(), &format!("pass{pass}.jpg"), ImageFormat::Jpeg).await;
            assert_eq!(written.losses, [], "pass {pass}");
            let gain_map = rust_sak::image::metadata_bytes(&file).gain_map.expect("written");
            assert_eq!(gain_map, original, "pass {pass}: not re-encoded");
            picture = image::load_blocking(&written.path).expect("decodable");
        }
    }

    #[test]
    fn a_gain_map_that_cannot_be_carried_still_writes_an_sdr_jpeg_and_says_so() {
        use crate::image::test_support::{FIXTURE_SIZE, truncated_ultra_hdr, written_ultra_hdr};
        use crate::image::{GainMap, Hdr, Metadata};

        let dir = tempfile::tempdir().expect("a temporary directory");
        let not_carried =
            [Loss::Hdr { hdr: Hdr::GainMap, format: ImageFormat::Jpeg, cause: super::super::HdrCause::NotCarried }];

        // Read at load, refused at export: its ISO payload is too large for the one segment it must fit in.
        let hawk = dir.path().join("hawk.jpg");
        std::fs::write(&hawk, written_ultra_hdr()).expect("written");
        let loaded = image::load_blocking(&hawk).expect("decodable");
        let mut oversized = Metadata::default();
        oversized.iso_gain_map = Some(vec![1; 70_000]);
        let refused: GainMap = loaded.gain_map().expect("carried").with_metadata(oversized);
        let refused = loaded.clone().with_gain_map(Some(refused));
        // Not read at load: cut short inside the gain map.
        let cut = dir.path().join("cut.jpg");
        std::fs::write(&cut, truncated_ultra_hdr()).expect("written");
        let cut = image::load_blocking(&cut).expect("the photograph still loads");
        // Its primary's XMP, padded past what a JPEG's standard XMP segment holds by a property the rewrite keeps:
        // `hdrgm:Version` would be moved into extended XMP with the rest, where readers would not find it.
        let mut padded = loaded.metadata().clone();
        let padding = "x".repeat(70_000);
        let xmp = String::from_utf8(padded.xmp.take().expect("XMP")).expect("UTF-8");
        padded.xmp = Some(
            xmp.replacen("hdrgm:Version=", &format!("xmlns:pad=\"urn:pad\" pad:Pad=\"{padding}\" hdrgm:Version="), 1)
                .into_bytes(),
        );
        let padded = loaded.clone().with_metadata(padded);
        assert!(padded.gain_map().is_some());

        // A secondary image the MPF index calls a gain map, with no gain-map metadata of its own: refused at load.
        let bare = {
            use crate::image::test_support::{ISO_VERSION, ULTRA_HDR_XMP, display_p3, ramp_image};
            let mut gain_map = Vec::new();
            rust_sak::image::encode_writer(&ramp_image(FIXTURE_SIZE), &mut gain_map, ImageFormat::Jpeg, None)
                .expect("encodable");
            let mut metadata = display_p3();
            metadata.xmp = Some(ULTRA_HDR_XMP.to_vec());
            metadata.iso_gain_map = Some(ISO_VERSION.to_vec());
            metadata.gain_map = Some(gain_map);
            let mut bytes = Vec::new();
            rust_sak::image::encode_writer_with(
                &ramp_image(FIXTURE_SIZE),
                &mut bytes,
                ImageFormat::Jpeg,
                None,
                &metadata,
            )
            .expect("encodable");
            let path = dir.path().join("bare.jpg");
            std::fs::write(&path, bytes).expect("written");
            assert_eq!(
                super::super::predict_blocking(&path, ImageFormat::Jpeg).expect("readable"),
                not_carried,
                "the prediction refuses it too"
            );
            image::load_blocking(&path).expect("the photograph still loads")
        };
        assert!(bare.gain_map().is_none());

        let pictures = [("refused.jpg", refused), ("cut-out.jpg", cut), ("padded.jpg", padded), ("bare-out.jpg", bare)];
        for (name, picture) in pictures {
            let (log, result) = crate::logging::records_of_blocking("info", || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("a runtime")
                    .block_on(write_to(&picture, dir.path(), name, ImageFormat::Jpeg))
            });
            let (written, file) = result;

            assert_eq!(written.losses, not_carried, "{name}");
            assert_eq!(written.losses[0].to_string(), "HDR gain map dropped: it could not be carried");
            assert_eq!(rust_sak::image::hdr_bytes(&file), None, "{name}: an SDR JPEG, claiming nothing");
            let read = rust_sak::image::metadata_bytes(&file);
            assert_eq!((read.gain_map, read.iso_gain_map), (None, None), "{name}");
            assert!(!String::from_utf8(read.xmp.expect("XMP")).expect("UTF-8").contains("hdrgm"), "{name}");
            assert!(rust_sak::image::decode_bytes(&file).is_ok(), "{name}");
            if name == "padded.jpg" {
                assert!(read_xmp_kept_padding(&file), "the rest of the XMP is still written");
            }
            if name == "refused.jpg" {
                let records = crate::logging::records(&log, "HDR gain map dropped: it could not be carried");
                assert_eq!(records.len(), 1, "{log}");
            }
        }
    }

    #[tokio::test]
    async fn a_google_container_directory_gets_the_written_gain_maps_length() {
        use crate::image::test_support::{display_p3, gain_map_jpeg, ramp_image};

        // As Android writes Ultra HDR: a directory of the primary and the gain map, the gain map's length stale here.
        let packet = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" xmlns:Container="http://ns.google.com/photos/1.0/container/" xmlns:Item="http://ns.google.com/photos/1.0/container/item/" hdrgm:Version="1.0"><Container:Directory><rdf:Seq><rdf:li rdf:parseType="Resource"><Container:Item Item:Semantic="Primary" Item:Mime="image/jpeg"/></rdf:li><rdf:li rdf:parseType="Resource"><Container:Item Item:Semantic="GainMap" Item:Mime="image/jpeg" Item:Length="12345"/></rdf:li></rdf:Seq></Container:Directory></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let mut metadata = display_p3();
        metadata.xmp = Some(packet.to_vec());
        metadata.gain_map = Some(gain_map_jpeg((16, 12), 1));
        let mut bytes = Vec::new();
        rust_sak::image::encode_writer_with(&ramp_image((32, 24)), &mut bytes, ImageFormat::Jpeg, None, &metadata)
            .expect("encodable");
        let dir = tempfile::tempdir().expect("a temporary directory");
        let source = dir.path().join("pixel.jpg");
        std::fs::write(&source, bytes).expect("written");
        let loaded = image::load_blocking(&source).expect("decodable");

        let (written, file) = write_to(&loaded, dir.path(), "out.jpg", ImageFormat::Jpeg).await;

        assert_eq!(written.losses, []);
        let read = rust_sak::image::metadata_bytes(&file);
        let length = read.gain_map.expect("written").len();
        let xmp = String::from_utf8(read.xmp.expect("XMP")).expect("UTF-8");
        assert!(xmp.contains(&format!(r#"Item:Length="{length}""#)), "{xmp}");
        assert!(!xmp.contains("12345") && xmp.contains(r#"Item:Semantic="Primary""#), "{xmp}");
        assert_eq!(mpf_entries(&file)[1].0, length);
    }
}
