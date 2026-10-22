//! Fixtures the image IO tests share, compiled only under `cfg(test)`.

// A file directly under `image/` because every one of load, save, probe and the round trip needs the same encoded
// bytes, and fixtures that differed between them would make their results incomparable — the round trip's claim that
// both forms of an operation agree rests on all of them describing the *same* picture.
//
// The fixtures are **encoded here rather than committed**. Eight formats times a file each is eight binaries in the
// repository that nobody can read in a diff, and their provenance would be a commit message; generated, the picture
// is the twenty lines below and every format is encoded by the same codec the test is about to exercise.

use std::path::{Path, PathBuf};

use ::image::{DynamicImage, RgbImage};
use rust_sak::image::{EncodeOptions, ImageFormat, Metadata, encode_writer, encode_writer_with};

// Above 16x16 deliberately. `rust-sak`'s `image` module records that the bundled AVIF encoder **hangs** on sub-16px
// frames rather than returning an error, so a fixture below that turns a failing test into a CI job that never
// finishes. 32x24 is comfortably clear of it and still encodes in milliseconds.
/// The size every fixture is generated at.
pub(crate) const FIXTURE_SIZE: (u32, u32) = (32, 24);

/// The picture every fixture holds.
///
/// Deliberately detailed rather than flat: a solid colour compresses to the same handful of bytes at every quality,
/// which would make "a lower quality produces a smaller file" unprovable.
pub(crate) fn sample_image() -> DynamicImage {
    // A diagonal gradient crossed with a high-frequency checker, which gives a lossy encoder both something smooth to
    // keep and something awkward to throw away.
    let (width, height) = FIXTURE_SIZE;

    DynamicImage::ImageRgb8(RgbImage::from_fn(width, height, |x, y| {
        let gradient = ((x * 255 / width.max(1)) + (y * 255 / height.max(1))) / 2;
        let checker = if (x / 2 + y / 2) % 2 == 0 { 60 } else { 0 };

        image::Rgb([
            (gradient as u8).saturating_add(checker),
            ((255 - gradient) as u8).saturating_sub(checker),
            ((x * 7 + y * 13) % 256) as u8,
        ])
    }))
}

/// [`sample_image`] encoded as `format`, with `options` or the format's defaults.
///
/// # Panics
///
/// Panics, naming the format, if the fixture cannot be encoded.
pub(crate) fn encoded(format: ImageFormat, options: Option<EncodeOptions>) -> Vec<u8> {
    let mut bytes = Vec::new();
    encode_writer(&sample_image(), &mut bytes, format, options)
        .unwrap_or_else(|err| panic!("the {format:?} fixture could not be encoded: {err}"));
    bytes
}

/// [`sample_image`] as PNG. The default fixture: lossless, native, and needing no downloaded codec binaries.
pub(crate) fn written_png() -> Vec<u8> {
    encoded(ImageFormat::Png, None)
}

/// [`sample_image`] as JPEG, for the tests about content outranking a file's name.
pub(crate) fn written_jpeg() -> Vec<u8> {
    encoded(ImageFormat::Jpeg, None)
}

/// [`sample_image`] as TIFF, for the tests about RAW: a RAW file is a TIFF as far as its magic bytes go.
pub(crate) fn written_tiff() -> Vec<u8> {
    encoded(ImageFormat::Tiff, None)
}

/// A Display P3 ICC profile: `DisplayP3-v4.icc` from saucecontrol/Compact-ICC-Profiles (CC0 1.0), 480 bytes,
/// committed at the workspace root's `fixtures/` beside the live photograph so every crate's tests reach the same file.
///
/// The one fixture that is committed rather than generated, because a profile is not something these tests can
/// produce: what they assert is that a real one travels byte for byte.
pub(crate) const DISPLAY_P3: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/DisplayP3-v4.icc"));

/// Metadata carrying [`DISPLAY_P3`].
pub(crate) fn display_p3() -> Metadata {
    let mut metadata = Metadata::default();
    metadata.icc_profile = Some(DISPLAY_P3.to_vec());
    metadata
}

/// An RGB ICC v2 profile that names itself `description`, `len` bytes long (at least its 220 bytes of header, tag
/// table and `desc` tag, zero-padded beyond): enough for what this crate reads of a profile — its colour space and its
/// name — and for a profile of any size, since no committed sRGB profile is free to ship.
pub(crate) fn rgb_profile_named(description: &str, len: usize) -> Vec<u8> {
    let mut desc = b"desc\0\0\0\0".to_vec();
    desc.extend(u32::try_from(description.len() + 1).expect("short").to_be_bytes());
    desc.extend(description.as_bytes());
    desc.push(0);
    // No Unicode text, no ScriptCode text.
    desc.extend([0; 4 + 4 + 2 + 1 + 67]);
    let offset = 128 + 4 + 12;
    let mut profile = vec![0; 128];
    profile[8..12].copy_from_slice(&[2, 0x10, 0, 0]);
    profile[12..16].copy_from_slice(b"mntr");
    profile[16..20].copy_from_slice(b"RGB ");
    profile[20..24].copy_from_slice(b"XYZ ");
    profile[36..40].copy_from_slice(b"acsp");
    profile.extend(1u32.to_be_bytes());
    profile.extend(b"desc");
    profile.extend(u32::try_from(offset).expect("small").to_be_bytes());
    profile.extend(u32::try_from(desc.len()).expect("small").to_be_bytes());
    profile.extend(desc);
    profile.resize(len.max(profile.len()), 0);
    let size = u32::try_from(profile.len()).expect("under 4 GB").to_be_bytes();
    profile[0..4].copy_from_slice(&size);
    profile
}

/// The plain sRGB profile most cameras and editors embed, as [`rgb_profile_named`] makes it, `len` bytes long.
pub(crate) fn srgb_profile(len: usize) -> Vec<u8> {
    rgb_profile_named("sRGB IEC61966-2.1", len)
}

/// [`sample_image`] as `format` with [`DISPLAY_P3`] embedded: a wide-gamut photograph as a camera or a phone writes it.
///
/// # Panics
///
/// Panics if the fixture cannot be encoded, or if the format did not take the profile.
pub(crate) fn written_p3(format: ImageFormat) -> Vec<u8> {
    let mut bytes = Vec::new();
    let embedded = encode_writer_with(&sample_image(), &mut bytes, format, None, &display_p3())
        .unwrap_or_else(|err| panic!("the P3 {format:?} fixture could not be encoded: {err}"));
    assert!(embedded.icc_profile, "{format:?} did not embed the profile");
    bytes
}

/// `jpeg` with an `APP2` segment holding `data` inserted straight after `SOI`.
pub(crate) fn with_app2(jpeg: &[u8], data: &[u8]) -> Vec<u8> {
    let length = u16::try_from(data.len() + 2).expect("one segment").to_be_bytes();
    let mut out = jpeg[..2].to_vec();
    out.extend([0xFF, 0xE2, length[0], length[1]]);
    out.extend(data);
    out.extend(&jpeg[2..]);
    out
}

/// An Ultra HDR / ISO 21496-1 JPEG as a phone writes its primary image: [`DISPLAY_P3`], XMP declaring the `hdrgm`
/// namespace, and the ISO 21496-1 version marker. The gain map image itself is not appended: nothing reads past the
/// primary image's first scan.
pub(crate) fn written_ultra_hdr() -> Vec<u8> {
    let mut metadata = display_p3();
    metadata.xmp = Some(
        br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" hdrgm:Version="1.0"/></rdf:RDF></x:xmpmeta>"#
            .to_vec(),
    );
    let mut bytes = Vec::new();
    encode_writer_with(&sample_image(), &mut bytes, ImageFormat::Jpeg, None, &metadata).expect("encodable");
    with_app2(&bytes, b"urn:iso:std:iso:ts:21496:-1\0\0\0\0\0")
}

/// A minimal big-endian EXIF block: `Make` = `Cam` and `Orientation` = 6 (rotate 90° clockwise to view) in IFD0.
///
/// Orientation 6 rather than 1 so a test can tell a rendition that carried it from one that did not.
pub(crate) const EXIF: &[u8] = b"MM\0\x2a\0\0\0\x08\0\x02\
    \x01\x0f\0\x02\0\0\0\x04Cam\0\
    \x01\x12\0\x03\0\0\0\x01\0\x06\0\0\
    \0\0\0\0";

/// A small XMP packet naming two keywords.
pub(crate) const XMP: &[u8] = b"<?xpacket begin='\xef\xbb\xbf' id='W5M0MpCehiHzreSzNTczkc9d'?>\
<x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\
<rdf:Description rdf:about='' xmlns:dc='http://purl.org/dc/elements/1.1/'>\
<dc:subject><rdf:Bag><rdf:li>hawk</rdf:li><rdf:li>bird</rdf:li></rdf:Bag></dc:subject>\
</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>";

/// A Photoshop resource block holding one IPTC-IIM resource (`0x0404`) with one keyword dataset (2:25 `hawk`).
pub(crate) const IPTC: &[u8] = b"8BIM\x04\x04\0\0\0\0\0\x09\x1c\x02\x19\0\x04hawk\0";

/// Metadata carrying [`DISPLAY_P3`] and the three descriptive blocks, [`EXIF`], [`XMP`] and [`IPTC`].
pub(crate) fn described() -> Metadata {
    let mut metadata = display_p3();
    metadata.exif = Some(EXIF.to_vec());
    metadata.xmp = Some(XMP.to_vec());
    metadata.iptc = Some(IPTC.to_vec());
    metadata
}

/// [`sample_image`] as `format` with [`described`] embedded: a photograph as a camera and a cataloguing tool leave it.
///
/// # Panics
///
/// Panics if the fixture cannot be encoded, or if the format did not take every block it can hold.
pub(crate) fn written_described(format: ImageFormat) -> Vec<u8> {
    let mut bytes = Vec::new();
    let embedded = encode_writer_with(&sample_image(), &mut bytes, format, None, &described())
        .unwrap_or_else(|err| panic!("the described {format:?} fixture could not be encoded: {err}"));
    assert!(embedded.icc_profile && embedded.exif && embedded.xmp, "{format:?} did not embed {embedded:?}");
    assert_eq!(embedded.iptc, format == ImageFormat::Jpeg, "{format:?}");
    bytes
}

/// Where the parts of [`camera_exif`] are.
pub(crate) struct CameraExif {
    /// The block.
    pub(crate) exif: Vec<u8>,
    /// The MakerNote's bytes, whose one entry points at its data by an offset from the start of the block.
    pub(crate) maker_note: std::ops::Range<usize>,
}

/// The data [`camera_exif`]'s MakerNote points at.
pub(crate) const MAKER_NOTE_DATA: &[u8; 8] = b"SECRET!!";

/// A big-endian EXIF block as a camera writes one: IFD0 (Make `Canon`, Orientation 6, Software, DateTime), an Exif IFD
/// (DateTimeOriginal, OffsetTimeOriginal, a Canon-style MakerNote whose pointer is absolute, PixelX/YDimension 4000 ×
/// 3000), a GPS IFD (latitude 42° N) and IFD1 with a 64-byte thumbnail last. Laid out byte by byte, so the tests do not
/// depend on the code they test to build it.
pub(crate) fn camera_exif() -> CameraExif {
    let mut b = vec![0_u8; 392];
    let put = |b: &mut Vec<u8>, at: usize, bytes: &[u8]| b[at..at + bytes.len()].copy_from_slice(bytes);
    let table = |b: &mut Vec<u8>, at: usize, entries: &[(u16, u16, u32, [u8; 4])], next: u32| {
        put(b, at, &u16::try_from(entries.len()).expect("few").to_be_bytes());
        for (index, (tag, kind, count, raw)) in entries.iter().enumerate() {
            let entry = at + 2 + 12 * index;
            put(b, entry, &tag.to_be_bytes());
            put(b, entry + 2, &kind.to_be_bytes());
            put(b, entry + 4, &count.to_be_bytes());
            put(b, entry + 8, raw);
        }
        put(b, at + 2 + 12 * entries.len(), &next.to_be_bytes());
    };
    let at = u32::to_be_bytes;
    let short = |value: u16| {
        let mut raw = [0; 4];
        raw[..2].copy_from_slice(&value.to_be_bytes());
        raw
    };

    put(&mut b, 0, b"MM\0\x2a\0\0\0\x08");
    table(
        &mut b,
        8,
        &[
            (0x010F, 2, 6, at(86)),   // Make
            (0x0112, 3, 1, short(6)), // Orientation
            (0x0131, 2, 11, at(92)),  // Software
            (0x0132, 2, 20, at(104)), // DateTime
            (0x8769, 4, 1, at(124)),  // Exif IFD
            (0x8825, 4, 1, at(244)),  // GPS IFD
        ],
        298,
    );
    put(&mut b, 86, b"Canon\0");
    put(&mut b, 92, b"Cam FW 1.0\0");
    put(&mut b, 104, b"2026:10:18 16:03:50\0");
    table(
        &mut b,
        124,
        &[
            (0x9003, 2, 20, at(190)),    // DateTimeOriginal
            (0x9011, 2, 7, at(210)),     // OffsetTimeOriginal
            (0x927C, 7, 26, at(218)),    // MakerNote
            (0xA002, 3, 1, short(4000)), // PixelXDimension
            (0xA003, 3, 1, short(3000)), // PixelYDimension
        ],
        0,
    );
    put(&mut b, 190, b"2026:10:18 16:03:50\0");
    put(&mut b, 210, b"-04:00\0");
    table(&mut b, 218, &[(0x0001, 7, 8, at(236))], 0);
    put(&mut b, 236, MAKER_NOTE_DATA);
    table(&mut b, 244, &[(0x0001, 2, 2, *b"N\0\0\0"), (0x0002, 5, 3, at(274))], 0);
    put(
        &mut b,
        274,
        &[42_u32, 1, 22, 1, 12, 1].iter().flat_map(|value| value.to_be_bytes()).collect::<Vec<u8>>(),
    );
    table(&mut b, 298, &[(0x0201, 4, 1, at(328)), (0x0202, 4, 1, at(64))], 0);
    put(&mut b, 328, &[0xD8; 64]);

    CameraExif { exif: b, maker_note: 218..244 }
}

/// A Photoshop resource block as Lightroom writes one: resolution info, the IPTC-IIM record (keyword `hawk`), a
/// thumbnail resource and the IIM's digest.
pub(crate) fn photoshop_block() -> Vec<u8> {
    let resource = |id: u16, data: &[u8]| {
        let mut bytes = b"8BIM".to_vec();
        bytes.extend(id.to_be_bytes());
        bytes.extend([0, 0]);
        bytes.extend(u32::try_from(data.len()).expect("small").to_be_bytes());
        bytes.extend(data);
        if data.len() % 2 == 1 {
            bytes.push(0);
        }
        bytes
    };
    [
        resource(0x03ED, &[1; 16]),
        IPTC.to_vec(),
        resource(0x040C, &[0xFF; 40]),
        resource(0x0425, &[7; 16]),
    ]
    .concat()
}

/// Writes `bytes` into `dir` under `name` and hands back the path.
pub(crate) fn write_fixture(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    // The name is the test's to choose, because half of what these tests assert is that the name does *not* decide
    // the format — a JPEG called `.png` has to be as easy to write as one called `.jpg`.
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap_or_else(|err| panic!("the fixture {name} could not be written: {err}"));
    path
}

/// Runs `work` against an async runtime that has already shut down, which is what a process on its way down leaves
/// behind, and hands back what it reported.
pub(crate) fn on_a_dead_runtime<T>(work: impl Future<Output = T>) -> T {
    // The runtime is shut down *before* the work is handed to it rather than racing a shutdown against a task already
    // in flight, so the outcome does not depend on which won. `spawn_blocking` targets whichever runtime is current,
    // which is why the dead one is entered for the call while a live current-thread runtime drives the future — a dead
    // runtime cannot poll anything itself.
    let shut_down = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let handle = shut_down.handle().clone();
    shut_down.shutdown_timeout(std::time::Duration::ZERO);

    tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async {
        let _entered = handle.enter();
        work.await
    })
}

// ── The DNG fixture ───────────────────────────────────────────────────────────────────────────────────────────────
//
// Ported from `rust-sak`'s own `src/image/tests.rs`, where the same builder synthesises the fixture its RAW tests
// use. It is duplicated rather than shared because there is no way to share it: it lives in a `#[cfg(test)]`
// module, so it is unreachable from here, and publishing it would mean a `test-support` feature and a `rust-sak`
// release.
//
// What makes the duplication tolerable is that this encodes a *file format* rather than an API. DNG 1.x tag layout
// does not move when `rust-sak` releases, so the two copies cannot drift apart the way two copies of a call site
// would; if `rawler` ever tightens what it accepts, both fail the same way and the pin is a git tag that moves only
// deliberately.
//
// Built rather than committed, for the reason the eight encoded fixtures are: a 10-30 MB camera file in the
// repository is a binary nobody can read in a diff. Building it also makes the tests *say* something — the sensor
// values are known, so an assertion can be "a red CFA develops to a red picture" rather than "it did not error".
//
// `rawler`'s DNG decoder reads each of these tags from the IFD and falls back to a default where one is absent, so
// a minimal file is a supported file rather than a lucky one.

/// TIFF field types, by their on-disk codes.
const BYTE: u16 = 1;
const ASCII: u16 = 2;
const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;
const SRATIONAL: u16 = 10;

/// CFA colour codes, as `CFAPattern` spells them.
pub(crate) const RED: u8 = 0;
pub(crate) const GREEN: u8 = 1;
pub(crate) const BLUE: u8 = 2;

/// The sensor value written where the CFA says "this photosite saw the bright channel".
const BRIGHT: u16 = 60_000;
/// The sensor value written everywhere else.
const DIM: u16 = 2_000;

// Its own constant rather than `FIXTURE_SIZE` because a demosaic has edge handling that a 32x24 frame is small enough
// to let dominate the channel means the tests assert on.
/// The size the DNG fixture is generated at, even in both directions so the 2x2 CFA of a Bayer sensor tiles.
pub(crate) const DNG_FIXTURE_SIZE: (u32, u32) = (64, 64);

/// The make and model the fixture records, which probing reads back out of it.
pub(crate) const DNG_MAKE: &str = "rust-sak";
pub(crate) const DNG_MODEL: &str = "Synthetic DNG";

/// One IFD entry, with its payload not yet placed.
struct Field {
    tag: u16,
    kind: u16,
    count: u32,
    payload: Vec<u8>,
}

fn field(tag: u16, kind: u16, count: u32, payload: Vec<u8>) -> Field {
    Field { tag, kind, count, payload }
}

fn shorts(tag: u16, values: &[u16]) -> Field {
    let payload = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    field(tag, SHORT, values.len() as u32, payload)
}

fn longs(tag: u16, values: &[u32]) -> Field {
    let payload = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    field(tag, LONG, values.len() as u32, payload)
}

fn bytes_field(tag: u16, values: &[u8]) -> Field {
    field(tag, BYTE, values.len() as u32, values.to_vec())
}

fn ascii(tag: u16, text: &str) -> Field {
    let mut payload = text.as_bytes().to_vec();
    payload.push(0);
    let count = payload.len() as u32;
    field(tag, ASCII, count, payload)
}

/// Unsigned rationals, written as numerator/denominator pairs.
fn rationals(tag: u16, values: &[(u32, u32)]) -> Field {
    let payload = values.iter().flat_map(|(n, d)| [n.to_le_bytes(), d.to_le_bytes()]).flatten().collect();
    field(tag, RATIONAL, values.len() as u32, payload)
}

/// Signed rationals, as `ColorMatrix1` needs — a camera matrix has negative terms.
fn srationals(tag: u16, values: &[(i32, i32)]) -> Field {
    let payload = values.iter().flat_map(|(n, d)| [n.to_le_bytes(), d.to_le_bytes()]).flatten().collect();
    field(tag, SRATIONAL, values.len() as u32, payload)
}

/// Builds a complete little-endian DNG: an uncompressed 16-bit Bayer CFA image whose bright photosites are the ones
/// `cfa` labels with `bright_channel`.
pub(crate) fn minimal_dng(width: u32, height: u32, cfa: [u8; 4], bright_channel: u8) -> Vec<u8> {
    // The sensor readings themselves: a photosite of the bright channel is bright, everything else is dim, plus a
    // gentle gradient so the picture is not one flat colour and a transposed row would show.
    let mut strip = Vec::with_capacity((width * height * 2) as usize);
    for y in 0..height {
        for x in 0..width {
            let colour = cfa[((y % 2) * 2 + (x % 2)) as usize];
            let base = if colour == bright_channel { BRIGHT } else { DIM };
            let gradient = ((x + y) * 8) as u16;
            strip.extend(base.saturating_add(gradient).to_le_bytes());
        }
    }

    // The layout is the plain one the TIFF spec describes — 8-byte header, then IFD0, then the payloads too large to
    // sit inside an entry, then the strip. Entries must be written in ascending tag order, which is why this list is
    // sorted rather than grouped by meaning.
    let fields = vec![
        longs(254, &[0]),                  // NewSubfileType: the full-resolution image
        longs(256, &[width]),              // ImageWidth
        longs(257, &[height]),             // ImageLength
        shorts(258, &[16]),                // BitsPerSample
        shorts(259, &[1]),                 // Compression: none
        shorts(262, &[32803]),             // PhotometricInterpretation: CFA
        ascii(271, DNG_MAKE),              // Make
        ascii(272, DNG_MODEL),             // Model
        longs(273, &[0]),                  // StripOffsets — patched once the layout is known
        shorts(277, &[1]),                 // SamplesPerPixel
        longs(278, &[height]),             // RowsPerStrip: the whole image in one strip
        longs(279, &[strip.len() as u32]), // StripByteCounts
        shorts(284, &[1]),                 // PlanarConfiguration: chunky
        shorts(33421, &[2, 2]),            // CFARepeatPatternDim
        bytes_field(33422, &cfa),          // CFAPattern
        bytes_field(50706, &[1, 4, 0, 0]), // DNGVersion 1.4.0.0
        bytes_field(50707, &[1, 1, 0, 0]), // DNGBackwardVersion
        ascii(50708, "opai synthetic"),    // UniqueCameraModel
        shorts(50714, &[0]),               // BlackLevel
        longs(50717, &[65535]),            // WhiteLevel
        // ColorMatrix1 maps XYZ to camera. Identity keeps the fixture's arithmetic legible: whatever the demosaic
        // produces per channel is what the picture shows, so a red CFA cannot come out red by way of a matrix that
        // happened to boost red.
        srationals(50721, &[(1, 1), (0, 1), (0, 1), (0, 1), (1, 1), (0, 1), (0, 1), (0, 1), (1, 1)]),
        // AsShotNeutral of 1:1:1 means no white-balance scaling, for the same reason.
        rationals(50728, &[(1, 1), (1, 1), (1, 1)]),
        shorts(50778, &[21]), // CalibrationIlluminant1: D65
    ];

    // A payload of four bytes or fewer lives inside the entry; anything larger is placed after the IFD and
    // referenced by offset.
    const HEADER: usize = 8;
    let ifd_len = 2 + fields.len() * 12 + 4;
    let heap_start = HEADER + ifd_len;

    let mut heap = Vec::new();
    let mut entries = Vec::with_capacity(fields.len() * 12);
    let mut strip_offset_position = None;

    for f in &fields {
        entries.extend(f.tag.to_le_bytes());
        entries.extend(f.kind.to_le_bytes());
        entries.extend(f.count.to_le_bytes());

        if f.tag == 273 {
            // StripOffsets points at the strip, which sits after the heap — so remember where to patch it.
            strip_offset_position = Some(entries.len());
            entries.extend(0_u32.to_le_bytes());
        } else if f.payload.len() <= 4 {
            let mut inline = f.payload.clone();
            inline.resize(4, 0);
            entries.extend(inline);
        } else {
            entries.extend(((heap_start + heap.len()) as u32).to_le_bytes());
            heap.extend(&f.payload);
            // Offsets must be even, which a trailing odd-length ASCII payload can break.
            if heap.len() % 2 != 0 {
                heap.push(0);
            }
        }
    }

    let strip_start = heap_start + heap.len();
    let position = strip_offset_position.expect("StripOffsets is in the field list");
    entries[position..position + 4].copy_from_slice(&(strip_start as u32).to_le_bytes());

    let mut dng = Vec::with_capacity(strip_start + strip.len());
    dng.extend(b"II"); // little-endian
    dng.extend(42_u16.to_le_bytes()); // the TIFF magic number
    dng.extend((HEADER as u32).to_le_bytes()); // IFD0 starts immediately
    dng.extend((fields.len() as u16).to_le_bytes());
    dng.extend(&entries);
    dng.extend(0_u32.to_le_bytes()); // no IFD1
    dng.extend(&heap);
    dng.extend(&strip);
    dng
}

/// A red-dominant RGGB DNG — the fixture every RAW test wants, and the counterpart of [`written_png`].
pub(crate) fn written_dng() -> Vec<u8> {
    let (width, height) = DNG_FIXTURE_SIZE;
    minimal_dng(width, height, [RED, GREEN, GREEN, BLUE], RED)
}

/// The mean of each channel across the whole picture.
pub(crate) fn channel_means(image: &DynamicImage) -> [f64; 3] {
    let rgb = image.as_rgb16().expect("a developed raw is 16-bit rgb");
    let mut totals = [0.0_f64; 3];
    for pixel in rgb.pixels() {
        for (total, sample) in totals.iter_mut().zip(pixel.0) {
            *total += f64::from(sample);
        }
    }
    let count = rgb.pixels().len() as f64;
    totals.map(|total| total / count)
}
