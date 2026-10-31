//! What a photograph's file says about how it was taken — camera, lens, exposure, when and whether where — and what
//! Open Photo AI recorded of an earlier rendering of it: the facts an information card shows.
//!
//! Read from the file, never the decoded picture, as [`read_lens`](super::read_lens) is, and bounded and infallible
//! the same way: a field missing, garbled or implausible is absent, and a file that cannot be read is one nothing is
//! known about.

use std::io::{BufReader, Read};
use std::path::Path;

use exif::{In, Reader, Tag, Value};
use rust_sak::image::{Hdr, hdr_bytes, metadata_bytes};

use crate::export::metadata::record::NAMESPACE;
use crate::export::metadata::xmp::edit::Xmp;

/// How much of a file is read for its XMP and its HDR declaration: every JPEG segment ahead of the first scan, and
/// a PNG's or WebP's chunks ahead of its pixels, in all but freak files.
const PREFIX: u64 = 8 << 20;

/// The shortest and longest exposure, in seconds, believed of a camera. Outside, the field is taken as garbled.
const EXPOSURES: std::ops::RangeInclusive<f64> = 1e-6..=3600.0;

/// What a photograph's file records of how it was taken, and of what Open Photo AI rendered into it. Every field is
/// `None`, empty or `false` where the file does not say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capture {
    /// The camera's maker and model (`Make`, `Model`).
    pub make: Option<String>,
    pub model: Option<String>,
    /// The lens (`LensModel`).
    pub lens_model: Option<String>,
    /// The focal length in millimetres, its 35 mm equivalent, and the f-number, as [`read_lens`](super::read_lens)
    /// reads them.
    pub focal_mm: Option<f32>,
    pub focal_35mm: Option<f32>,
    pub f_number: Option<f32>,
    /// The exposure time in seconds (`ExposureTime`).
    pub exposure_s: Option<f64>,
    /// The sensitivity (`PhotographicSensitivity`, once `ISOSpeedRatings`).
    pub iso: Option<u32>,
    /// When it was taken, `YYYY-MM-DD HH:MM:SS` (`DateTimeOriginal`), and the UTC offset then, `+02:00`
    /// (`OffsetTimeOriginal`).
    pub taken: Option<String>,
    pub taken_offset: Option<String>,
    /// Whether the file records where it was taken (`GPSLatitude`). Only whether: the place is not read.
    pub located: bool,
    /// How the file declares HDR, if it does: a gain map, PQ or HLG.
    pub hdr: Option<Hdr>,
    /// The f-number an earlier Open Photo AI lens blur simulated (`opai:LensBlurFNumber`): the photograph is already
    /// blurred.
    pub rendered_f_number: Option<f32>,
    /// The enhancements an earlier Open Photo AI export recorded (`opai:Enhancements`), in order.
    pub rendered: Vec<String>,
}

/// What the photograph at `path` records; see [`Capture`]. Never fails.
pub fn read_capture(path: &Path) -> Capture {
    let mut capture = std::fs::File::open(path)
        .ok()
        .and_then(|file| Reader::new().read_from_container(&mut BufReader::new(file)).ok())
        .map(|exif| capture_of(&exif))
        .unwrap_or_default();

    let mut prefix = Vec::new();
    if std::fs::File::open(path).and_then(|file| file.take(PREFIX).read_to_end(&mut prefix)).is_ok() {
        capture.hdr = hdr_bytes(&prefix);
        let xmp = metadata_bytes(&prefix).xmp.and_then(|packet| Xmp::parse(&packet).ok());
        if let Some(xmp) = xmp {
            capture.rendered_f_number =
                xmp.get(NAMESPACE, "LensBlurFNumber").and_then(|value| value.trim().parse().ok());
            capture.rendered = xmp.seq_values(NAMESPACE, "Enhancements").unwrap_or_default();
        }
    }
    capture
}

/// The capture fields of `exif`.
fn capture_of(exif: &exif::Exif) -> Capture {
    let text = |tag| {
        let field = exif.get_field(tag, In::PRIMARY)?;
        let Value::Ascii(values) = &field.value else {
            return None;
        };
        let text = String::from_utf8_lossy(values.first()?).trim_matches(char::from(0)).trim().to_owned();
        (!text.is_empty()).then_some(text)
    };
    let lens = super::lens::lens_of(exif);
    let exposure_s = exif.get_field(Tag::ExposureTime, In::PRIMARY).and_then(|field| match &field.value {
        Value::Rational(values) => values.first().map(exif::Rational::to_f64),
        _ => None,
    });
    let iso = exif
        .get_field(Tag::PhotographicSensitivity, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .filter(|&iso| iso > 0);
    // `2026:10:18 16:03:50`, as EXIF writes it, as `2026-10-18 16:03:50`.
    let taken = text(Tag::DateTimeOriginal)
        .filter(|date| date.len() == 19 && date.is_ascii())
        .map(|date| format!("{}-{}-{}", &date[..4], &date[5..7], &date[8..]));
    let taken_offset = text(Tag::OffsetTimeOriginal).filter(|offset| offset.len() == 6);

    Capture {
        make: text(Tag::Make),
        model: text(Tag::Model),
        lens_model: text(Tag::LensModel),
        focal_mm: lens.focal_mm,
        focal_35mm: lens.focal_35mm,
        f_number: lens.f_number,
        exposure_s: exposure_s.filter(|exposure| EXPOSURES.contains(exposure)),
        iso,
        taken,
        taken_offset,
        located: exif.get_field(Tag::GPSLatitude, In::PRIMARY).is_some(),
        ..Capture::default()
    }
}

#[cfg(test)]
mod tests {
    use exif::{Field, Rational};

    use super::*;
    use crate::image::lens::tests::{fields, jpeg};

    fn ascii(tag: Tag, text: &str) -> Field {
        Field { tag, ifd_num: In::PRIMARY, value: Value::Ascii(vec![text.as_bytes().to_vec()]) }
    }

    fn read(bytes: &[u8]) -> Capture {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("photo.jpg");
        std::fs::write(&path, bytes).expect("the fixture is written");
        read_capture(&path)
    }

    #[test]
    fn the_camera_the_lens_the_exposure_and_the_time_are_read_and_the_place_only_as_present() {
        let mut camera = fields(Some(5.7), Some(26), Some(1.8), None);
        camera.extend([
            ascii(Tag::Make, "Apple"),
            ascii(Tag::Model, "iPhone 15 Pro"),
            ascii(Tag::LensModel, "iPhone 15 Pro back camera 6.86mm f/1.78"),
            ascii(Tag::DateTimeOriginal, "2026:10:18 16:03:50"),
            ascii(Tag::OffsetTimeOriginal, "+02:00"),
            Field {
                tag: Tag::ExposureTime,
                ifd_num: In::PRIMARY,
                value: Value::Rational(vec![Rational { num: 1, denom: 120 }]),
            },
            Field { tag: Tag::PhotographicSensitivity, ifd_num: In::PRIMARY, value: Value::Short(vec![50]) },
        ]);

        let capture = read(&jpeg(&camera));

        assert_eq!(capture.make.as_deref(), Some("Apple"));
        assert_eq!(capture.model.as_deref(), Some("iPhone 15 Pro"));
        assert_eq!(capture.lens_model.as_deref(), Some("iPhone 15 Pro back camera 6.86mm f/1.78"));
        assert_eq!(capture.focal_35mm, Some(26.0));
        assert_eq!(capture.f_number, Some(1.8));
        assert!((capture.exposure_s.expect("an exposure") - 1.0 / 120.0).abs() < 1e-12);
        assert_eq!(capture.iso, Some(50));
        assert_eq!(capture.taken.as_deref(), Some("2026-10-18 16:03:50"));
        assert_eq!(capture.taken_offset.as_deref(), Some("+02:00"));
        assert!(!capture.located);
        assert_eq!(capture.hdr, None);
        assert_eq!(capture.rendered_f_number, None);
    }

    #[test]
    fn an_earlier_lens_blur_is_read_from_the_xmp_it_was_recorded_in() {
        let packet = format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:opai="{NAMESPACE}" opai:LensBlurFNumber="1.4"><opai:Enhancements><rdf:Seq><rdf:li>Lens Blur Lisbon (FP16): f_number=1.4</rdf:li></rdf:Seq></opai:Enhancements></rdf:Description></rdf:RDF></x:xmpmeta>"#
        );
        let payload = [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), packet.as_bytes()].concat();
        let length = u16::try_from(payload.len() + 2).expect("a small segment");
        let bytes = [&[0xFF, 0xD8, 0xFF, 0xE1][..], &length.to_be_bytes(), &payload, &[0xFF, 0xD9]].concat();

        let capture = read(&bytes);

        assert_eq!(capture.rendered_f_number, Some(1.4));
        assert_eq!(capture.rendered, vec!["Lens Blur Lisbon (FP16): f_number=1.4".to_owned()]);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_one_nothing_is_known_about() {
        assert_eq!(read_capture(Path::new("/nonexistent/photo.jpg")), Capture::default());
    }
}
