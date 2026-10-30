//! The fields that state facts about the source's pixels, made true of the exported ones.
//!
//! | Field | Where | What it becomes |
//! |---|---|---|
//! | SubjectArea `0x9214`, SubjectLocation `0xA214` | Exif IFD, and `exif:` in XMP | moved with the pixels: turned upright, into a crop, scaled by an upscale; removed where the subject is no longer in the frame or the position no longer fits the field |
//! | FocalPlaneXResolution `0xA20E`, FocalPlaneYResolution `0xA20F` | Exif IFD, and `exif:` in XMP | scaled by the pixels' growth (an upscale), so a reader that works out the sensor from them still finds the real one; a crop keeps them, since it leaves the pixel pitch as it was |
//!
//! Everything the camera measured — FNumber, FocalLength, FocalLengthIn35mmFilm, exposure, ISO, the dates, GPS — is
//! not a fact about pixels and is never rewritten here.
//!
//! **The EXIF's own frame.** A camera writes the subject's position in the pixels as stored, before its Orientation
//! turns them, measured against the PixelXDimension and PixelYDimension it records. The decode turns them upright and
//! may develop a RAW at another size; a crop, a turn and an upscale move them again ([`Edits::frame`]). [`Geometry`]
//! composes the four into one transform from the EXIF's frame to the exported pixels.
//!
//! **In place.** The EXIF rewrite has already patched the block without moving a kept byte; these fields keep their
//! type and count, so their new values are written over the old ones, and a field removed is taken out of its
//! directory by moving the entries after it up — no value moves, and the removed value's bytes are zeroed.

use rust_sak::image::{ExifIfd, ExifValue, exif_field};

use crate::image::{Affine, Edits};

pub(crate) const SUBJECT_AREA: u16 = 0x9214;
pub(crate) const SUBJECT_LOCATION: u16 = 0xA214;
pub(crate) const FOCAL_PLANE_X_RESOLUTION: u16 = 0xA20E;
pub(crate) const FOCAL_PLANE_Y_RESOLUTION: u16 = 0xA20F;
const ORIENTATION: u16 = 0x0112;
const PIXEL_X_DIMENSION: u16 = 0xA002;
const PIXEL_Y_DIMENSION: u16 = 0xA003;
const EXIF_POINTER: u16 = 0x8769;

/// How a point of the source's EXIF frame lands on the exported pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geometry {
    /// The EXIF's frame, as stored, to the exported pixels.
    to_export: Affine,
    /// The exported pixels' size.
    size: (u32, u32),
}

impl Geometry {
    /// The move from the frame `exif` describes to the `size` pixels `edits` made, or `None` where it moves nothing —
    /// an upright source exported whole at its own size — and there is nothing to rewrite.
    ///
    /// The source's decoded size is the frame's where a frame moved, and the export's otherwise. Where the EXIF records
    /// no pixel size, it is taken to describe the decoded pixels, as stored.
    pub(crate) fn of(exif: Option<&[u8]>, edits: &Edits, size: (u32, u32)) -> Option<Self> {
        let number = |tag| match exif.and_then(|exif| exif_field(exif, ExifIfd::Exif, tag)) {
            Some(ExifValue::Short(value)) => Some(u32::from(value)),
            Some(ExifValue::Long(value)) => Some(value),
            _ => None,
        };
        let orientation = match exif.and_then(|exif| exif_field(exif, ExifIfd::Primary, ORIENTATION)) {
            Some(ExifValue::Short(value @ 1..=8)) => value,
            _ => 1,
        };
        let frame = edits.frame();
        let decoded = frame.map_or(size, |frame| frame.source);
        let turned = orientation >= 5;
        let stored = match (number(PIXEL_X_DIMENSION), number(PIXEL_Y_DIMENSION)) {
            (Some(width), Some(height)) if width > 0 && height > 0 => (width, height),
            _ if turned => (decoded.1, decoded.0),
            _ => decoded,
        };

        let (w, h) = (f64::from(stored.0), f64::from(stored.1));
        let upright = match orientation {
            2 => Affine::new(-1.0, 0.0, w, 0.0, 1.0, 0.0),
            3 => Affine::new(-1.0, 0.0, w, 0.0, -1.0, h),
            4 => Affine::new(1.0, 0.0, 0.0, 0.0, -1.0, h),
            5 => Affine::new(0.0, 1.0, 0.0, 1.0, 0.0, 0.0),
            6 => Affine::new(0.0, -1.0, h, 1.0, 0.0, 0.0),
            7 => Affine::new(0.0, -1.0, h, -1.0, 0.0, w),
            8 => Affine::new(0.0, 1.0, 0.0, -1.0, 0.0, w),
            _ => Affine::IDENTITY,
        };
        let (upright_w, upright_h) = if turned { (h, w) } else { (w, h) };
        let developed = Affine::scale(f64::from(decoded.0) / upright_w, f64::from(decoded.1) / upright_h);
        let moved = frame.map_or(Affine::IDENTITY, |frame| frame.to_pixels);

        let to_export = upright.then(developed).then(moved);
        (!to_export.is_identity() || size != stored).then_some(Self { to_export, size })
    }

    /// A SubjectArea or SubjectLocation moved with the pixels: a point `(x, y)`, a circle `(x, y, diameter)` or a
    /// rectangle `(x, y, width, height)` about its centre, as EXIF writes them. `None` where the centre has left the
    /// exported frame, or the value is not one of the three shapes and cannot be moved.
    pub(crate) fn subject(&self, values: &[u32]) -> Option<Vec<u32>> {
        let &[x, y, ref rest @ ..] = values else {
            return None;
        };
        let (cx, cy) = self.to_export.apply((f64::from(x), f64::from(y)));
        let (width, height) = (f64::from(self.size.0), f64::from(self.size.1));
        if !(0.0..width).contains(&cx) || !(0.0..height).contains(&cy) {
            return None;
        }
        let round = |value: f64| value.round().max(0.0) as u32;
        let mut moved = vec![round(cx).min(self.size.0 - 1), round(cy).min(self.size.1 - 1)];
        let [[a, b], [d, e]] = self.to_export.linear();
        match *rest {
            [] => {}
            // A circle stays a circle: its diameter grows as areas do, by the square root of the determinant.
            [diameter] => moved.push(round(f64::from(diameter) * (a * e - b * d).abs().sqrt())),
            // A rectangle turned becomes the box around it, kept inside the frame.
            [w, h] => {
                let (w, h) = (f64::from(w), f64::from(h));
                moved.push(round(a.abs() * w + b.abs() * h).min(self.size.0));
                moved.push(round(d.abs() * w + e.abs() * h).min(self.size.1));
            }
            _ => return None,
        }
        Some(moved)
    }

    /// How many times the pixel pitch along the EXIF's own x and y axes grew: what the focal-plane resolutions are
    /// multiplied by. `(1, 1)` for a crop or a turn alone.
    pub(crate) fn pitch(&self) -> (f64, f64) {
        self.to_export.stretch()
    }
}

/// A focal-plane resolution `numerator/denominator` multiplied by `by`, as a rational of the same kind: the
/// denominator kept where the numerator stays exact to a millionth, and refined by a thousand where it would not and
/// still fits. `None` where it cannot be written (a zero denominator, or a value past what 32 bits hold).
pub(crate) fn scaled_rational(numerator: u32, denominator: u32, by: f64) -> Option<(u32, u32)> {
    if denominator == 0 || !by.is_finite() || by <= 0.0 {
        return None;
    }
    let value = f64::from(numerator) / f64::from(denominator) * by;
    let mut denominator = u64::from(denominator);
    let exact = |denominator: u64| {
        let numerator = (value * denominator as f64).round();
        ((numerator / denominator as f64 - value).abs() <= value * 1e-6).then_some(numerator)
    };
    while exact(denominator).is_none() && denominator * 1000 <= u64::from(u32::MAX) {
        denominator *= 1000;
    }
    let numerator = (value * denominator as f64).round();
    (numerator <= f64::from(u32::MAX)).then_some((numerator as u32, denominator as u32))
}

/// `exif`, already rewritten, with the fields of the module's table made true of `geometry`. Unchanged where its
/// structure cannot be walked: the rewrite before this has already checked it.
pub(crate) fn rewrite_exif(mut exif: Vec<u8>, geometry: &Geometry) -> Vec<u8> {
    let Some(ifd) = Tiff::parse(&exif).and_then(|tiff| tiff.exif_ifd(&exif).map(|at| (tiff, at))) else {
        return exif;
    };
    let (tiff, at) = ifd;

    for tag in [SUBJECT_AREA, SUBJECT_LOCATION] {
        let Some(entry) = tiff.find(&exif, at, tag) else {
            continue;
        };
        let values = entry.integers(&tiff, &exif);
        let moved = values.as_deref().and_then(|values| geometry.subject(values));
        match moved.filter(|moved| moved.len() == entry.count as usize) {
            Some(moved) if entry.fits(&moved) => entry.write_integers(&tiff, &mut exif, &moved),
            _ => tiff.remove(&mut exif, at, &entry),
        }
    }

    let (x, y) = geometry.pitch();
    for (tag, by) in [(FOCAL_PLANE_X_RESOLUTION, x), (FOCAL_PLANE_Y_RESOLUTION, y)] {
        if (by - 1.0).abs() < 1e-6 {
            continue;
        }
        let Some(entry) = tiff.find(&exif, at, tag).filter(|entry| entry.kind == RATIONAL && entry.count == 1) else {
            continue;
        };
        let Some((numerator, denominator)) = entry.rational(&tiff, &exif) else {
            continue;
        };
        match scaled_rational(numerator, denominator, by) {
            Some(scaled) => entry.write_rational(&tiff, &mut exif, scaled),
            // A resolution that can no longer be written true is a sensor size worked out wrong: it goes.
            None => tiff.remove(&mut exif, at, &entry),
        }
    }
    exif
}

const SHORT: u16 = 3;
const LONG: u16 = 4;
const RATIONAL: u16 = 5;

/// A TIFF stream's byte order, and where its IFD0 is.
struct Tiff {
    little: bool,
    ifd0: usize,
}

/// One directory entry: where it is, its type and count, and where its value is.
#[derive(Debug, Clone, Copy)]
struct Entry {
    at: usize,
    kind: u16,
    count: u32,
    value: usize,
}

impl Tiff {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let little = match bytes.get(..4)? {
            b"II\x2a\0" => true,
            b"MM\0\x2a" => false,
            _ => return None,
        };
        let tiff = Self { little, ifd0: 0 };
        let ifd0 = tiff.u32(bytes, 4)? as usize;
        Some(Self { ifd0, ..tiff })
    }

    fn u16(&self, bytes: &[u8], at: usize) -> Option<u16> {
        let raw: [u8; 2] = bytes.get(at..at + 2)?.try_into().ok()?;
        Some(if self.little { u16::from_le_bytes(raw) } else { u16::from_be_bytes(raw) })
    }

    fn u32(&self, bytes: &[u8], at: usize) -> Option<u32> {
        let raw: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
        Some(if self.little { u32::from_le_bytes(raw) } else { u32::from_be_bytes(raw) })
    }

    fn put_u16(&self, bytes: &mut [u8], at: usize, value: u16) {
        let raw = if self.little { value.to_le_bytes() } else { value.to_be_bytes() };
        bytes[at..at + 2].copy_from_slice(&raw);
    }

    fn put_u32(&self, bytes: &mut [u8], at: usize, value: u32) {
        let raw = if self.little { value.to_le_bytes() } else { value.to_be_bytes() };
        bytes[at..at + 4].copy_from_slice(&raw);
    }

    /// Where the Exif IFD is, by IFD0's pointer to it.
    fn exif_ifd(&self, bytes: &[u8]) -> Option<usize> {
        let pointer = self.find(bytes, self.ifd0, EXIF_POINTER)?;
        Some(self.u32(bytes, pointer.at + 8)? as usize)
    }

    /// The entry for `tag` in the directory at `ifd`, if it is there and its value lies inside the block.
    fn find(&self, bytes: &[u8], ifd: usize, tag: u16) -> Option<Entry> {
        let entries = usize::from(self.u16(bytes, ifd)?);
        (0..entries).find_map(|index| {
            let at = ifd + 2 + index * 12;
            if self.u16(bytes, at)? != tag {
                return None;
            }
            let kind = self.u16(bytes, at + 2)?;
            let count = self.u32(bytes, at + 4)?;
            let size = match kind {
                SHORT => 2,
                LONG => 4,
                RATIONAL => 8,
                _ => return None,
            } * count as usize;
            let value = if size <= 4 { at + 8 } else { self.u32(bytes, at + 8)? as usize };
            (value + size <= bytes.len()).then_some(Entry { at, kind, count, value })
        })
    }

    /// Takes `entry` out of the directory at `ifd`: the entries after it, and the next-directory offset, move up one,
    /// and the freed entry and an out-of-line value are zeroed. No value moves.
    fn remove(&self, bytes: &mut [u8], ifd: usize, entry: &Entry) {
        let Some(entries) = self.u16(bytes, ifd) else {
            return;
        };
        let end = ifd + 2 + usize::from(entries) * 12 + 4;
        if end > bytes.len() || entries == 0 {
            return;
        }
        let size = entry.size();
        if size > 4 {
            bytes[entry.value..entry.value + size].fill(0);
        }
        bytes.copy_within(entry.at + 12..end, entry.at);
        bytes[end - 12..end].fill(0);
        self.put_u16(bytes, ifd, entries - 1);
    }
}

impl Entry {
    fn size(&self) -> usize {
        let unit = match self.kind {
            SHORT => 2,
            LONG => 4,
            _ => 8,
        };
        unit * self.count as usize
    }

    fn integers(&self, tiff: &Tiff, bytes: &[u8]) -> Option<Vec<u32>> {
        (0..self.count as usize)
            .map(|index| match self.kind {
                SHORT => tiff.u16(bytes, self.value + index * 2).map(u32::from),
                LONG => tiff.u32(bytes, self.value + index * 4),
                _ => None,
            })
            .collect()
    }

    /// Whether every one of `values` fits the entry's type.
    fn fits(&self, values: &[u32]) -> bool {
        self.kind == LONG || (self.kind == SHORT && values.iter().all(|&value| value <= u32::from(u16::MAX)))
    }

    fn write_integers(&self, tiff: &Tiff, bytes: &mut [u8], values: &[u32]) {
        for (index, &value) in values.iter().enumerate() {
            match self.kind {
                // `fits` was asked first.
                SHORT => tiff.put_u16(bytes, self.value + index * 2, value as u16),
                _ => tiff.put_u32(bytes, self.value + index * 4, value),
            }
        }
    }

    fn rational(&self, tiff: &Tiff, bytes: &[u8]) -> Option<(u32, u32)> {
        Some((tiff.u32(bytes, self.value)?, tiff.u32(bytes, self.value + 4)?))
    }

    fn write_rational(&self, tiff: &Tiff, bytes: &mut [u8], (numerator, denominator): (u32, u32)) {
        tiff.put_u32(bytes, self.value, numerator);
        tiff.put_u32(bytes, self.value + 4, denominator);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::image::Edits;

    /// A little-endian block: IFD0 with Orientation `orientation` and the Exif pointer; an Exif IFD with
    /// PixelX/YDimension 4000x3000, FocalPlaneXResolution and FocalPlaneYResolution 4000/1 (pixels per unit), and a
    /// SubjectArea rectangle `(3000, 1000, 400, 200)` and SubjectLocation `(3000, 1000)`; then a trailing marker.
    pub(crate) fn block(orientation: u16) -> Vec<u8> {
        let mut b = Vec::new();
        let entry = |b: &mut Vec<u8>, tag: u16, kind: u16, count: u32, raw: [u8; 4]| {
            b.extend(tag.to_le_bytes());
            b.extend(kind.to_le_bytes());
            b.extend(count.to_le_bytes());
            b.extend(raw);
        };
        let short = |v: u16| {
            let mut raw = [0; 4];
            raw[..2].copy_from_slice(&v.to_le_bytes());
            raw
        };
        let pair = |x: u16, y: u16| {
            let mut raw = [0; 4];
            raw[..2].copy_from_slice(&x.to_le_bytes());
            raw[2..].copy_from_slice(&y.to_le_bytes());
            raw
        };
        b.extend(b"II\x2a\0");
        b.extend(8_u32.to_le_bytes());
        // IFD0 at 8: two entries, ends at 8 + 2 + 24 + 4 = 38.
        b.extend(2_u16.to_le_bytes());
        entry(&mut b, ORIENTATION, SHORT, 1, short(orientation));
        entry(&mut b, EXIF_POINTER, LONG, 1, 38_u32.to_le_bytes());
        b.extend(0_u32.to_le_bytes());
        // Exif IFD at 38: six entries, ends at 38 + 2 + 72 + 4 = 116; values from 116.
        b.extend(6_u16.to_le_bytes());
        entry(&mut b, SUBJECT_AREA, SHORT, 4, 116_u32.to_le_bytes());
        entry(&mut b, PIXEL_X_DIMENSION, SHORT, 1, short(4000));
        entry(&mut b, PIXEL_Y_DIMENSION, SHORT, 1, short(3000));
        entry(&mut b, FOCAL_PLANE_X_RESOLUTION, RATIONAL, 1, 124_u32.to_le_bytes());
        entry(&mut b, FOCAL_PLANE_Y_RESOLUTION, RATIONAL, 1, 132_u32.to_le_bytes());
        entry(&mut b, SUBJECT_LOCATION, SHORT, 2, pair(3000, 1000));
        b.extend(0_u32.to_le_bytes());
        for value in [3000_u16, 1000, 400, 200] {
            b.extend(value.to_le_bytes());
        }
        for _ in 0..2 {
            b.extend(4000_u32.to_le_bytes());
            b.extend(1_u32.to_le_bytes());
        }
        b.extend(b"END!");
        b
    }

    pub(crate) fn read(exif: &[u8], tag: u16) -> Option<Vec<u32>> {
        let tiff = Tiff::parse(exif)?;
        let at = tiff.exif_ifd(exif)?;
        let entry = tiff.find(exif, at, tag)?;
        match entry.kind {
            RATIONAL => entry.rational(&tiff, exif).map(|(n, d)| vec![n, d]),
            _ => entry.integers(&tiff, exif),
        }
    }

    #[test]
    fn an_upright_photograph_exported_whole_at_its_own_size_has_nothing_to_rewrite() {
        assert_eq!(Geometry::of(Some(&block(1)), &Edits::default(), (4000, 3000)), None);
    }

    #[test]
    fn a_crop_moves_the_subject_into_it_and_keeps_the_focal_plane() {
        let edits = Edits::default().framed((4000, 3000), Affine::translation(-2000.0, -500.0));
        let geometry = Geometry::of(Some(&block(1)), &edits, (1500, 1500)).expect("a move");
        let rewritten = rewrite_exif(block(1), &geometry);

        assert_eq!(read(&rewritten, SUBJECT_AREA), Some(vec![1000, 500, 400, 200]));
        assert_eq!(read(&rewritten, SUBJECT_LOCATION), Some(vec![1000, 500]));
        assert_eq!(read(&rewritten, FOCAL_PLANE_X_RESOLUTION), Some(vec![4000, 1]));
        assert!(rewritten.ends_with(b"END!"));
        assert_eq!(rewritten.len(), block(1).len(), "a byte moved");
    }

    #[test]
    fn an_upscale_scales_the_subject_and_the_focal_plane_resolutions() {
        let edits = Edits::default().framed((4000, 3000), Affine::scale(2.0, 2.0));
        let geometry = Geometry::of(Some(&block(1)), &edits, (8000, 6000)).expect("a move");
        let rewritten = rewrite_exif(block(1), &geometry);

        // 6000 still fits a SHORT; the resolution doubles.
        assert_eq!(read(&rewritten, SUBJECT_AREA), Some(vec![6000, 2000, 800, 400]));
        assert_eq!(read(&rewritten, FOCAL_PLANE_X_RESOLUTION), Some(vec![8000, 1]));
        assert_eq!(read(&rewritten, FOCAL_PLANE_Y_RESOLUTION), Some(vec![8000, 1]));
    }

    #[test]
    fn a_subject_cropped_out_of_the_frame_is_removed_and_its_bytes_zeroed() {
        // A crop of the left half: the subject at x = 3000 is outside it.
        let edits = Edits::default().framed((4000, 3000), Affine::IDENTITY);
        let geometry = Geometry::of(Some(&block(1)), &edits, (2000, 3000)).expect("a move");
        let rewritten = rewrite_exif(block(1), &geometry);

        assert_eq!(read(&rewritten, SUBJECT_AREA), None);
        assert_eq!(read(&rewritten, SUBJECT_LOCATION), None);
        // The rest of the directory is still found, and the removed rectangle's bytes are gone.
        assert_eq!(read(&rewritten, FOCAL_PLANE_X_RESOLUTION), Some(vec![4000, 1]));
        assert_eq!(
            exif_field(&rewritten, ExifIfd::Exif, PIXEL_X_DIMENSION),
            Some(ExifValue::Short(4000)),
            "the entries after a removed one were lost"
        );
        assert_eq!(&rewritten[116..124], &[0; 8]);
    }

    #[test]
    fn a_subject_past_what_a_short_holds_is_removed_rather_than_wrapped() {
        let edits = Edits::default().framed((4000, 3000), Affine::scale(32.0, 32.0));
        let geometry = Geometry::of(Some(&block(1)), &edits, (128_000, 96_000)).expect("a move");
        let rewritten = rewrite_exif(block(1), &geometry);
        assert_eq!(read(&rewritten, SUBJECT_AREA), None);
        assert_eq!(read(&rewritten, FOCAL_PLANE_X_RESOLUTION), Some(vec![128_000, 1]));
    }

    #[test]
    fn the_orientation_turns_the_subject_upright_with_the_pixels() {
        // Orientation 6: stored 4000x3000, shown 3000x4000 turned a quarter clockwise. A stored (3000, 1000) is
        // shown at (3000 - 1000, 3000) = (2000, 3000), and the rectangle's sides swap.
        let geometry = Geometry::of(Some(&block(6)), &Edits::default(), (3000, 4000)).expect("a move");
        assert_eq!(geometry.subject(&[3000, 1000, 400, 200]), Some(vec![2000, 3000, 200, 400]));
        assert_eq!(geometry.pitch(), (1.0, 1.0));
    }

    #[test]
    fn a_raw_developed_at_another_size_is_measured_against_its_exif_size() {
        // The EXIF says 4000x3000; the developer made 2000x1500 of it.
        let geometry = Geometry::of(Some(&block(1)), &Edits::default(), (2000, 1500)).expect("a move");
        assert_eq!(geometry.subject(&[3000, 1000]), Some(vec![1500, 500]));
        assert_eq!(geometry.subject(&[3000, 1000, 100]), Some(vec![1500, 500, 50]));
        assert_eq!(geometry.pitch(), (0.5, 0.5));
    }

    #[test]
    fn a_shape_exif_does_not_define_cannot_be_moved() {
        let geometry =
            Geometry::of(None, &Edits::default().framed((10, 10), Affine::scale(2.0, 2.0)), (20, 20)).expect("a move");
        assert_eq!(geometry.subject(&[1]), None);
        assert_eq!(geometry.subject(&[1, 2, 3, 4, 5]), None);
    }

    #[test]
    fn a_resolution_keeps_its_denominator_where_it_can_and_refines_it_where_it_cannot() {
        assert_eq!(scaled_rational(4000, 1, 2.0), Some((8000, 1)));
        assert_eq!(scaled_rational(3_443_946, 1000, 4.0), Some((13_775_784, 1000)));
        assert_eq!(scaled_rational(10, 1, 1.0 / 3.0), Some((3_333_333, 1_000_000)));
        assert_eq!(scaled_rational(1, 0, 2.0), None);
        assert_eq!(scaled_rational(u32::MAX, 1, 2.0), None);
    }
}
