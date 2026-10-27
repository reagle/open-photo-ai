//! The lens a photograph was taken with, as its EXIF records it: the fields a lens blur needs to know how much blur the
//! photograph already has and how a lens of its focal length would blur it more.
//!
//! Read from the file rather than from the decoded picture, because nothing the decoder hands back carries EXIF fields;
//! a crop does not change the lens, so the source's path is the right place to read it whatever framing is enhanced.

// Bounded and fallible, never an error: the lens is a hint a lens blur falls back from (to the depth model's focal
// estimate, and to an assumed f/8 equivalent), so a missing, garbled or implausible field is a field that is absent,
// and a file the reader cannot open at all is a lens nothing is known about. A run never fails over EXIF.

use std::io::BufReader;
use std::path::Path;

use exif::{In, Reader, Tag, Value};

/// The smallest and largest f-number a lens is believed to have been set to. Outside, the field is taken as garbled.
const F_NUMBERS: std::ops::RangeInclusive<f32> = 0.7..=64.0;

/// The shortest and longest focal length, in millimetres, believed of a real lens, real or 35 mm equivalent.
const FOCAL_LENGTHS: std::ops::RangeInclusive<f32> = 1.0..=2000.0;

/// The shortest and longest sensor diagonal, in millimetres, a focal-plane resolution is believed to describe: a small
/// phone sensor to a large-format back.
const SENSOR_DIAGONALS: std::ops::RangeInclusive<f32> = 2.0..=100.0;

/// How far the EXIF's image diagonal may be from the file's own, as a fraction, before the EXIF is taken to describe
/// other pixels than the file holds. Slack for a camera that records the size of a frame it then trims by a few pixels.
const DIAGONAL_TOLERANCE: f32 = 0.01;

/// What a photograph's EXIF says about the lens it was taken with. Every field is `None` where the EXIF does not say,
/// or says something no lens could be.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LensExif {
    /// The focal length the lens was at, in millimetres (`FocalLength`).
    pub focal_mm: Option<f32>,
    /// Its 35 mm-film equivalent, in millimetres (`FocalLengthIn35mmFilm`).
    pub focal_35mm: Option<f32>,
    /// The f-number the lens was set to (`FNumber`).
    pub f_number: Option<f32>,
    /// The sensor's diagonal in millimetres, from the focal-plane resolutions and the image's size in pixels
    /// (`FocalPlaneXResolution`, `FocalPlaneYResolution`, `FocalPlaneResolutionUnit`, `PixelXDimension` and
    /// `PixelYDimension`): what the crop factor is worked out from when the 35 mm equivalent is missing. A diagonal
    /// rather than a width, so the sensor's shape (3:2, 4:3, 16:9) is read from the file and never assumed.
    pub sensor_diagonal_mm: Option<f32>,
    /// The diagonal, in pixels, of the image the EXIF says it describes (`PixelXDimension` and `PixelYDimension`): the
    /// pixels the focal-plane resolution counts, which [`checked_against`](Self::checked_against) compares with the
    /// file's own.
    pub diagonal_px: Option<f32>,
}

impl LensExif {
    /// This lens with the sensor it names dropped where the EXIF's image is not the file's: a photograph resized or
    /// converted by software that left the focal-plane resolution as it was, whose sensor size would come out as the
    /// ratio of the two. `file_diagonal_px` is the diagonal of the file's own pixels, as the file's header gives it.
    ///
    /// Orientation does not matter, a diagonal being the same turned. A file with no pixel size in its EXIF is kept as
    /// it is: there is nothing to contradict.
    #[must_use]
    pub fn checked_against(self, file_diagonal_px: f32) -> Self {
        let agrees = self
            .diagonal_px
            .is_none_or(|exif| file_diagonal_px > 0.0 && (exif / file_diagonal_px - 1.0).abs() <= DIAGONAL_TOLERANCE);

        if agrees { self } else { Self { sensor_diagonal_mm: None, ..self } }
    }
}

/// The lens the photograph at `path` records, or an empty [`LensExif`] where it records none.
///
/// Any container `kamadak-exif` reads: JPEG, TIFF and the TIFF-based RAW formats (NEF, ARW, DNG, ORF, PEF and others),
/// PNG, WebP and HEIF. A RAW format that is not TIFF-based (CR3, RAF) yields nothing. Never fails: a file that cannot
/// be opened or parsed, or a picture with no file at all, is a lens nothing is known about.
pub fn read_lens(path: &Path) -> LensExif {
    let Ok(file) = std::fs::File::open(path) else {
        return LensExif::default();
    };

    match Reader::new().read_from_container(&mut BufReader::new(file)) {
        Ok(exif) => lens_of(&exif),
        Err(error) => {
            // `debug`: a photograph with no EXIF is ordinary, and what it costs is a fallback the caller reports.
            tracing::debug!(%error, "no lens could be read from the photograph's EXIF");
            LensExif::default()
        }
    }
}

/// The lens `exif` records, every field bounded.
fn lens_of(exif: &exif::Exif) -> LensExif {
    let number = |tag| exif.get_field(tag, In::PRIMARY).and_then(|field| first_number(&field.value));

    // `PixelXDimension` and `PixelYDimension` only: the first directory's `ImageWidth` is often a RAW's embedded
    // preview, which would make the sensor a fraction of its size.
    let diagonal_px = number(Tag::PixelXDimension)
        .zip(number(Tag::PixelYDimension))
        .map(|(width, height)| width.hypot(height))
        .filter(|diagonal| *diagonal > 0.0);
    // The unit defaults to inches, as the standard says of a missing one; 4 and 5 are millimetres and micrometres, as
    // DNG extends it.
    let millimetres_per_unit = match exif
        .get_field(Tag::FocalPlaneResolutionUnit, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .unwrap_or(2)
    {
        2 => Some(25.4),
        3 => Some(10.0),
        4 => Some(1.0),
        5 => Some(0.001),
        _ => None,
    };
    // Each axis by its own resolution, the vertical one taken as the horizontal's where a camera writes only that: the
    // two are the pixel pitch, and square pixels are what nearly every sensor has.
    let x_resolution = number(Tag::FocalPlaneXResolution);
    let y_resolution = number(Tag::FocalPlaneYResolution).or(x_resolution);
    let sensor_diagonal_mm = match (
        number(Tag::PixelXDimension).zip(number(Tag::PixelYDimension)),
        x_resolution.zip(y_resolution),
        millimetres_per_unit,
    ) {
        (Some((width, height)), Some((per_x, per_y)), Some(unit)) if per_x > 0.0 && per_y > 0.0 => {
            Some((width / per_x).hypot(height / per_y) * unit)
        }
        _ => None,
    };

    LensExif {
        focal_mm: number(Tag::FocalLength).filter(|focal| FOCAL_LENGTHS.contains(focal)),
        // Zero is how the standard writes "unknown" here, which the bound refuses with everything else implausible.
        focal_35mm: number(Tag::FocalLengthIn35mmFilm).filter(|focal| FOCAL_LENGTHS.contains(focal)),
        f_number: number(Tag::FNumber).filter(|f_number| F_NUMBERS.contains(f_number)),
        sensor_diagonal_mm: sensor_diagonal_mm.filter(|diagonal| SENSOR_DIAGONALS.contains(diagonal)),
        diagonal_px,
    }
}

/// The diagonal, in pixels, of the photograph at `path` as its file holds it, read from the header without decoding, or
/// `None` where the file cannot be described: no file, no such format, or a header that does not parse.
///
/// What a framing is measured against: a crop is a picture whose diagonal is shorter than its file's, and orientation
/// does not change a diagonal. A camera RAW is described by the dimensions it decodes to, which reads the whole file.
pub(crate) fn read_frame_diagonal(path: &Path) -> Option<f32> {
    // No error is worth recording for a picture with no file, as a sweep or a test makes: the lens blur is still
    // rendered, at the framing the picture has.
    if !path.is_file() {
        return None;
    }

    let (width, height) = if super::is_raw(path) {
        let info = super::probe_raw_blocking(path).ok()?;
        (info.width, info.height)
    } else {
        let info = super::probe_blocking(path).ok()?;
        (info.width, info.height)
    };

    #[allow(clippy::cast_precision_loss, reason = "a pixel count a diagonal is compared to a percent of")]
    let diagonal = (width as f32).hypot(height as f32);

    (diagonal > 0.0).then_some(diagonal)
}

/// The first number `value` holds, whichever numeric type the camera wrote it as, if it is finite.
#[allow(
    clippy::cast_possible_truncation,
    reason = "an EXIF number read to the precision a lens is described at"
)]
fn first_number(value: &Value) -> Option<f32> {
    let number = match value {
        Value::Rational(values) => values.first().map(exif::Rational::to_f64),
        Value::SRational(values) => values.first().map(exif::SRational::to_f64),
        Value::Float(values) => values.first().copied().map(f64::from),
        Value::Double(values) => values.first().copied(),
        other => other.get_uint(0).map(f64::from),
    };

    number.map(|number| number as f32).filter(|number| number.is_finite())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Cursor;

    use exif::experimental::Writer;
    use exif::{Field, Rational};

    use super::*;

    /// A field of the primary image.
    fn field(tag: Tag, value: Value) -> Field {
        Field { tag, ifd_num: In::PRIMARY, value }
    }

    /// A rational field holding `value` in hundredths.
    fn rational(tag: Tag, value: f64) -> Field {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "test values are small and positive"
        )]
        let num = (value * 100.0).round() as u32;

        field(tag, Value::Rational(vec![Rational { num, denom: 100 }]))
    }

    /// The EXIF a camera writes for `focal` mm at f/`f_number`, `focal_35mm` mm equivalent, on an image `width` by
    /// `height` pixels whose sensor is `sensor_mm` wide; each `None` left out.
    pub(crate) fn fields(
        focal: Option<f64>,
        focal_35mm: Option<u16>,
        f_number: Option<f64>,
        sensor: Option<(u32, u32, f64)>,
    ) -> Vec<Field> {
        // Never empty: a TIFF needs one field in its first directory, and a camera always writes an orientation.
        let mut fields = vec![field(Tag::Orientation, Value::Short(vec![1]))];

        fields.extend(focal.map(|focal| rational(Tag::FocalLength, focal)));
        fields.extend(focal_35mm.map(|focal| field(Tag::FocalLengthIn35mmFilm, Value::Short(vec![focal]))));
        fields.extend(f_number.map(|f_number| rational(Tag::FNumber, f_number)));
        if let Some((width, height, sensor_mm)) = sensor {
            // Pixels per centimetre, as many cameras write it, the same on both axes.
            let per_centimetre = f64::from(width) / (sensor_mm / 10.0);
            fields.push(field(Tag::PixelXDimension, Value::Long(vec![width])));
            fields.push(field(Tag::PixelYDimension, Value::Long(vec![height])));
            fields.push(rational(Tag::FocalPlaneXResolution, per_centimetre));
            fields.push(rational(Tag::FocalPlaneYResolution, per_centimetre));
            fields.push(field(Tag::FocalPlaneResolutionUnit, Value::Short(vec![3])));
        }

        fields
    }

    /// `fields` as a little-endian TIFF, which is also what a JPEG's EXIF segment carries.
    pub(crate) fn tiff(fields: &[Field]) -> Vec<u8> {
        let mut writer = Writer::new();
        for field in fields {
            writer.push_field(field);
        }

        let mut bytes = Cursor::new(Vec::new());
        writer.write(&mut bytes, true).expect("the fields encode");
        bytes.into_inner()
    }

    /// `fields` as the smallest JPEG a reader finds them in: the start marker, the EXIF segment, the end marker.
    pub(crate) fn jpeg(fields: &[Field]) -> Vec<u8> {
        let payload = [b"Exif\0\0".as_slice(), &tiff(fields)].concat();
        let length = u16::try_from(payload.len() + 2).expect("a small segment");

        [&[0xFF, 0xD8, 0xFF, 0xE1][..], &length.to_be_bytes(), &payload, &[0xFF, 0xD9]].concat()
    }

    /// The lens `bytes` record, read through a file as a photograph's would be.
    fn read(bytes: &[u8], extension: &str) -> LensExif {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join(format!("photo.{extension}"));
        std::fs::write(&path, bytes).expect("the fixture is written");

        read_lens(&path)
    }

    #[test]
    fn every_field_is_read_from_a_jpeg_and_from_a_tiff() {
        // A Sony a6700 at 24 mm f/4, 36 mm equivalent, 6192 x 4128 pixels across a 23.5 mm sensor.
        let recorded = fields(Some(24.0), Some(36), Some(4.0), Some((6192, 4128, 23.5)));

        for (bytes, extension) in [(jpeg(&recorded), "jpg"), (tiff(&recorded), "tif")] {
            let lens = read(&bytes, extension);

            assert_eq!(lens.focal_mm, Some(24.0), "{extension}");
            assert_eq!(lens.focal_35mm, Some(36.0), "{extension}");
            assert_eq!(lens.f_number, Some(4.0), "{extension}");
            let diagonal = lens.sensor_diagonal_mm.expect("the focal-plane resolution was read");
            assert!((diagonal - 23.5_f32.hypot(15.67)).abs() < 0.05, "{extension}: {diagonal}");
            assert!((lens.diagonal_px.expect("the image size was read") - 6192_f32.hypot(4128.0)).abs() < 0.5);
        }
    }

    #[test]
    fn each_missing_field_is_absent_and_the_others_still_read() {
        let lens = read(&jpeg(&fields(None, Some(36), Some(4.0), None)), "jpg");
        assert_eq!(lens, LensExif { focal_35mm: Some(36.0), f_number: Some(4.0), ..LensExif::default() });

        let lens = read(&jpeg(&fields(Some(6.9), None, None, None)), "jpg");
        assert_eq!(lens, LensExif { focal_mm: Some(6.9), ..LensExif::default() });

        let lens = read(&jpeg(&fields(Some(24.0), None, None, Some((6192, 4128, 23.5)))), "jpg");
        assert_eq!((lens.focal_mm, lens.focal_35mm, lens.f_number), (Some(24.0), None, None));
        assert!(lens.sensor_diagonal_mm.is_some());
    }

    #[test]
    fn an_implausible_value_is_absent_rather_than_believed() {
        // Zero is how "unknown" is written for the 35 mm equivalent; the rest are garbled.
        // The sensor works out at 0.6 mm across its diagonal, which is no sensor; the image's own size is still read.
        let lens = read(&jpeg(&fields(Some(5000.0), Some(0), Some(0.2), Some((6192, 4128, 0.5)))), "jpg");
        assert_eq!(LensExif { diagonal_px: None, ..lens }, LensExif::default());

        let lens = read(&jpeg(&fields(Some(0.5), Some(3000), Some(90.0), None)), "jpg");
        assert_eq!(lens, LensExif::default());
    }

    #[test]
    fn a_file_with_no_exif_a_non_image_and_no_file_are_each_a_lens_nothing_is_known_about() {
        assert_eq!(read(&[0xFF, 0xD8, 0xFF, 0xD9], "jpg"), LensExif::default());
        assert_eq!(read(b"not a photograph at all", "txt"), LensExif::default());
        assert_eq!(read_lens(Path::new("")), LensExif::default());
        assert_eq!(read_lens(Path::new("/nowhere/at/all.jpg")), LensExif::default());
    }

    #[test]
    fn the_sensor_diagonal_follows_the_sensors_own_shape_where_it_is_not_three_by_two() {
        // A 4:3 sensor, 7.6 mm across and so 9.5 mm on the diagonal, as a phone's; a width and a 3:2 assumed would have
        // made it 9.1.
        let lens = read(&jpeg(&fields(None, None, None, Some((4000, 3000, 7.6)))), "jpg");

        let diagonal = lens.sensor_diagonal_mm.expect("the focal-plane resolution was read");
        assert!((diagonal - 9.5).abs() < 0.02, "{diagonal}");
    }

    #[test]
    fn a_raw_previews_width_is_not_taken_for_the_images() {
        // IFD0's `ImageWidth` and `ImageLength` are a RAW's embedded preview; without `PixelXDimension` there is no
        // sensor, rather than one a fraction of its size.
        let mut recorded = fields(Some(24.0), None, Some(4.0), None);
        recorded.push(field(Tag::ImageWidth, Value::Long(vec![1616])));
        recorded.push(field(Tag::ImageLength, Value::Long(vec![1080])));
        recorded.push(rational(Tag::FocalPlaneXResolution, 2634.0));
        recorded.push(field(Tag::FocalPlaneResolutionUnit, Value::Short(vec![3])));

        let lens = read(&tiff(&recorded), "tif");

        assert_eq!((lens.sensor_diagonal_mm, lens.diagonal_px), (None, None));
    }

    #[test]
    fn a_sensor_is_dropped_where_its_exif_describes_other_pixels_than_the_file_holds() {
        let lens = read(&jpeg(&fields(Some(24.0), Some(36), Some(4.0), Some((6192, 4128, 23.5)))), "jpg");
        let diagonal = 6192_f32.hypot(4128.0);

        assert_eq!(lens.checked_against(diagonal), lens, "the file's own size contradicted its EXIF");
        // A file a hundredth off, as a camera that trims a few pixels from the frame it recorded, is the same file.
        assert_eq!(lens.checked_against(diagonal * 1.005), lens);
        // Resized by software that left the resolution as it was: the sensor it would give is wrong.
        let resized = lens.checked_against(diagonal / 2.0);
        assert_eq!(resized.sensor_diagonal_mm, None);
        assert_eq!((resized.focal_mm, resized.focal_35mm, resized.f_number), (Some(24.0), Some(36.0), Some(4.0)));
        // No size in the EXIF, nothing to contradict.
        let unsized_exif = LensExif { diagonal_px: None, ..lens };
        assert_eq!(unsized_exif.checked_against(100.0), unsized_exif);
    }

    #[test]
    fn the_frames_diagonal_is_the_files_own_and_absent_for_no_file() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = dir.path().join("frame.png");
        ::image::RgbImage::new(300, 400).save(&path).expect("the fixture is written");

        assert_eq!(read_frame_diagonal(&path), Some(500.0));
        assert_eq!(read_frame_diagonal(Path::new("")), None);
        assert_eq!(read_frame_diagonal(Path::new("/nowhere/at/all.jpg")), None);
        assert_eq!(read_frame_diagonal(dir.path()), None);
    }
}
