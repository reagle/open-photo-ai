//! A mask: the part of a photograph a run's enhancements are for.
//!
//! White is enhanced, black is left as it was, and grey mixes the two. A [`Mask`] is brush strokes over an optional
//! base weight image, optionally turned over, and it is **not pixels**: strokes are kept as paths in fractions of the
//! framed photograph and rasterised at whatever size a run needs ([`Mask::rasterised`]), so a preview and an export of
//! the same mask agree without resampling a bitmap. A run is given one through
//! [`ProcessOptions::mask`](crate::ProcessOptions::mask), and blends the chain's result with the photograph through it.
//!
//! The command line's `--mask` file is a base with no strokes ([`Mask::from_image`]); the GUI's brush is strokes with
//! no base. Both are this one type.

// A top-level module beside `image` and `export` rather than under `models`: a mask is neither a model nor a step of
// the chain. It never reaches a model, and it is not part of any step's cache key — a run applies it once, after the
// last step, which is what lets a brush stroke redo a blend rather than the models (see `inference::process`).
//
// The pixel work is `imaging`'s: `stroke::painted` draws one stroke and `mix::masked` blends. What is here is what
// those do not know: fractions of the photograph, the order strokes apply in, inversion, the wire shape and the
// signature the result's identity folds in.

use std::sync::Arc;

use image::DynamicImage;
use imaging::stroke::painted;
use rust_sak::crypto::{xxh3_bytes, xxh3_string};
use serde::{Deserialize, Serialize};

/// The single-channel `f32` plane a mask is rasterised to, re-exported so a caller need not name `imaging`.
pub use imaging::plane::Plane;

/// The smallest radius a stroke may have, as a fraction of the framed photograph's longer side.
pub const RADIUS_MIN: f32 = 0.0005;

/// The largest radius a stroke may have: half the longer side, a brush that covers the photograph from its middle.
pub const RADIUS_MAX: f32 = 0.5;

/// The part of a photograph the enhancements are for: strokes painted and erased in order over an optional base.
///
/// Serialised in camelCase as `{ strokes: [{ erase, radius, softness, points }], inverted }`. The base never crosses
/// the wire: it is built in-process, from a file or a selection.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mask {
    /// A weight image the strokes are painted over, or `None` to start from black.
    #[serde(skip)]
    pub base: Option<Base>,
    /// Painted and erased in order.
    #[serde(default)]
    pub strokes: Vec<Stroke>,
    /// Whether the finished weight is turned over, `1 - w`.
    #[serde(default)]
    pub inverted: bool,
}

/// One brush stroke: a path, a radius and a softness, painting or erasing.
///
/// Deserialised leniently: a value out of range is clamped into it (one that is not a number is 0) and an odd trailing
/// coordinate is dropped, never refused, since nothing but a fault can send one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "Unchecked")]
pub struct Stroke {
    /// Whether this stroke takes weight away rather than adding it.
    pub erase: bool,
    /// The radius, as a fraction of the framed photograph's longer side, from [`RADIUS_MIN`] to [`RADIUS_MAX`].
    pub radius: f32,
    /// 0 is a hard edge, anti-aliased over a pixel only; 1 feathers all the way from the centre out.
    pub softness: f32,
    /// The path as `x0, y0, x1, y1, …`, each a fraction of the framed photograph's width or height. One point is a dot.
    pub points: Vec<f32>,
}

/// A [`Stroke`] as it arrived, before its values are brought into range.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Unchecked {
    #[serde(default)]
    erase: bool,
    radius: f32,
    softness: f32,
    points: Vec<f32>,
}

impl From<Unchecked> for Stroke {
    fn from(stroke: Unchecked) -> Self {
        Self::new(stroke.erase, stroke.radius, stroke.softness, stroke.points)
    }
}

impl Stroke {
    /// A stroke with its values clamped into range, a NaN among them 0, and an odd trailing coordinate dropped.
    pub fn new(erase: bool, radius: f32, softness: f32, mut points: Vec<f32>) -> Self {
        points.truncate(points.len() & !1);
        points.iter_mut().for_each(|value| *value = unit(*value));

        // A radius that is not a number is the smallest, as a coordinate or a softness that is not one is 0.
        let radius = if radius.is_nan() { RADIUS_MIN } else { radius.clamp(RADIUS_MIN, RADIUS_MAX) };

        Self { erase, radius, softness: unit(softness), points }
    }
}

/// `value` bounded to `[0, 1]`, where one that is not a number is 0: `clamp` hands NaN on.
fn unit(value: f32) -> f32 {
    if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) }
}

/// A weight image a mask's strokes are painted over, with the signature of its pixels taken once.
#[derive(Clone)]
pub struct Base {
    /// The weights, at whatever size they arrived; rasterising resamples them bilinearly.
    plane: Arc<Plane>,
    /// XXH3-64 of the plane's size and values.
    signature: u64,
}

impl Base {
    /// `plane` as a base, its values bounded to `[0, 1]` and any that is not a number 0.
    pub fn new(mut plane: Plane) -> Self {
        plane.as_mut_slice().iter_mut().for_each(|value| *value = unit(*value));

        // Taken here, once: a 24-megapixel base is 96 MB of values, and every run's identity asks for it.
        let mut bytes = Vec::with_capacity(8 + 4 * plane.as_slice().len());
        bytes.extend(plane.width().to_le_bytes());
        bytes.extend(plane.height().to_le_bytes());
        bytes.extend(plane.as_slice().iter().flat_map(|value| value.to_le_bytes()));
        let signature = parse_hex(&xxh3_bytes(&bytes));

        Self { plane: Arc::new(plane), signature }
    }

    /// The weights.
    pub fn plane(&self) -> &Plane {
        &self.plane
    }

    /// XXH3-64 of the weights and their size.
    pub fn signature(&self) -> u64 {
        self.signature
    }
}

impl std::fmt::Debug for Base {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The size and the signature rather than millions of values.
        f.debug_struct("Base")
            .field("width", &self.plane.width())
            .field("height", &self.plane.height())
            .field("signature", &format_args!("{:016x}", self.signature))
            .finish()
    }
}

impl PartialEq for Base {
    fn eq(&self, other: &Self) -> bool {
        self.signature == other.signature
    }
}

impl Mask {
    /// A mask whose base is `image`'s luminance and which has no strokes: white applies the enhancements.
    ///
    /// The luminance is Rec. 709's over the decoded values, at the image's own depth, so a 16-bit mask keeps its 16
    /// bits. **An alpha channel is ignored**: a grey PNG saved with transparency is read for its grey, not for how
    /// opaque it is.
    pub fn from_image(image: &DynamicImage) -> Self {
        Self { base: Some(Base::new(luminance(image))), ..Self::default() }
    }

    /// Whether this mask is no mask: no base and no strokes, whether or not it is inverted.
    ///
    /// A run given an empty mask is exactly the run given none, identity included.
    pub fn is_empty(&self) -> bool {
        self.base.is_none() && self.strokes.is_empty()
    }

    /// The weight at every pixel of a `width` x `height` picture: the base resampled to that size (or 0), each stroke
    /// painted or erased over it in order, then turned over if inverted.
    ///
    /// A stroke's radius in pixels is its fraction of the longer side, at least half a pixel, and its hard core is
    /// `radius * (1 - softness)`. The same mask rasterised at two sizes gives the same weight at the same fraction of
    /// the picture, to within the edge's anti-aliasing.
    pub fn rasterised(&self, width: u32, height: u32) -> Plane {
        let mut plane = match &self.base {
            Some(base) => base.plane.resized(width, height),
            None => Plane::new(width, height, 0.0),
        };

        let longer = width.max(height) as f32;
        let (sx, sy) = (width as f32, height as f32);

        for stroke in &self.strokes {
            let radius = (stroke.radius * longer).max(0.5);
            let core = radius * (1.0 - stroke.softness);
            let points: Vec<f32> =
                stroke.points.as_chunks::<2>().0.iter().flat_map(|[x, y]| [x * sx, y * sy]).collect();

            painted(&mut plane, &points, radius, core, stroke.erase);
        }

        if self.inverted {
            plane.as_mut_slice().iter_mut().for_each(|value| *value = 1.0 - *value);
        }

        plane
    }

    /// XXH3-64 of the strokes, the inversion and the base: what a masked result's identity folds in.
    ///
    /// Any change to a stroke's mode, radius, softness or points, to their order, to the inversion or to the base's
    /// pixels changes it.
    pub fn signature(&self) -> u64 {
        // The wire shape as the canonical form: it is already exactly the strokes and the inversion, in order, and
        // `serde_json` writes an `f32` the same way every time.
        let shape = serde_json::to_string(self).expect("a mask serialises: it is numbers and booleans");
        let base = self.base.as_ref().map_or_else(|| "none".to_string(), |base| format!("{:016x}", base.signature));

        parse_hex(&xxh3_string(&format!("{shape}|{base}")))
    }
}

/// The 64-bit value of the lowercase hex `rust-sak`'s `crypto` writes a hash as.
fn parse_hex(hex: &str) -> u64 {
    u64::from_str_radix(hex, 16).expect("an XXH3-64 is sixteen hex digits")
}

/// `image`'s Rec. 709 luminance as a plane, from its colour channels alone.
fn luminance(image: &DynamicImage) -> Plane {
    let (width, height) = (image.width(), image.height());

    // Each variant read at its own depth, colour only: `imaging`'s `Sampler` would premultiply alpha against black,
    // which is exactly the reading of a transparent mask this refuses.
    let values: Vec<f32> = match image {
        DynamicImage::ImageLuma8(buffer) => buffer.pixels().map(|pixel| unit8(pixel.0[0])).collect(),
        DynamicImage::ImageLumaA8(buffer) => buffer.pixels().map(|pixel| unit8(pixel.0[0])).collect(),
        DynamicImage::ImageLuma16(buffer) => buffer.pixels().map(|pixel| unit16(pixel.0[0])).collect(),
        DynamicImage::ImageLumaA16(buffer) => buffer.pixels().map(|pixel| unit16(pixel.0[0])).collect(),
        DynamicImage::ImageRgb8(buffer) => buffer.pixels().map(|pixel| rec709(pixel.0.map(unit8))).collect(),
        DynamicImage::ImageRgba8(buffer) => {
            buffer.pixels().map(|pixel| rec709([pixel.0[0], pixel.0[1], pixel.0[2]].map(unit8))).collect()
        }
        DynamicImage::ImageRgb16(buffer) => buffer.pixels().map(|pixel| rec709(pixel.0.map(unit16))).collect(),
        DynamicImage::ImageRgba16(buffer) => {
            buffer.pixels().map(|pixel| rec709([pixel.0[0], pixel.0[1], pixel.0[2]].map(unit16))).collect()
        }
        other => other.to_rgb32f().pixels().map(|pixel| rec709(pixel.0)).collect(),
    };

    Plane::from_vec(width, height, values).expect("one value per pixel of the image")
}

/// An 8-bit channel as a `[0, 1]` fraction.
fn unit8(value: u8) -> f32 {
    f32::from(value) / f32::from(u8::MAX)
}

/// A 16-bit channel as a `[0, 1]` fraction.
fn unit16(value: u16) -> f32 {
    f32::from(value) / f32::from(u16::MAX)
}

/// Rec. 709's luma weights over `[r, g, b]`.
fn rec709([r, g, b]: [f32; 3]) -> f32 {
    0.0722f32.mul_add(b, 0.2126f32.mul_add(r, 0.7152 * g))
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Luma, Rgb, Rgba};

    use super::*;

    /// A soft paint stroke across the middle of the picture.
    fn soft() -> Stroke {
        Stroke::new(false, 0.08, 0.6, vec![0.2, 0.3, 0.7, 0.6, 0.8, 0.4])
    }

    /// A mask of `strokes` alone.
    fn painted_with(strokes: Vec<Stroke>) -> Mask {
        Mask { strokes, ..Mask::default() }
    }

    /// The weight at the fraction `(fx, fy)` of `plane`: the pixel whose centre is nearest it.
    fn at(plane: &Plane, fx: f32, fy: f32) -> f32 {
        let x = ((fx * plane.width() as f32).floor() as u32).min(plane.width() - 1);
        let y = ((fy * plane.height() as f32).floor() as u32).min(plane.height() - 1);

        plane.get(x, y)
    }

    #[test]
    fn a_mask_rasterised_at_two_sizes_has_the_same_weight_at_the_same_fractions() {
        let mask = painted_with(vec![soft()]);
        let (small, large) = (mask.rasterised(1000, 750), mask.rasterised(4000, 3000));

        for i in 0..10 {
            for j in 0..10 {
                let (fx, fy) = ((i as f32 + 0.5) / 10.0, (j as f32 + 0.5) / 10.0);
                let (a, b) = (at(&small, fx, fy), at(&large, fx, fy));

                assert!((a - b).abs() < 0.02, "({fx}, {fy}) is {a} at 1000 x 750 and {b} at 4000 x 3000");
            }
        }

        // And the grid is not all on one side of the stroke.
        assert!(small.as_slice().contains(&1.0) && small.as_slice().contains(&0.0));
    }

    #[test]
    fn an_erase_along_the_same_path_leaves_nothing() {
        let erase = Stroke { erase: true, ..soft() };
        let plane = painted_with(vec![soft(), erase]).rasterised(400, 300);

        assert!(plane.as_slice().iter().all(|w| *w == 0.0), "the erase left some of the paint");
    }

    #[test]
    fn painting_the_same_stroke_twice_is_painting_it_once() {
        assert_eq!(
            painted_with(vec![soft(), soft()]).rasterised(400, 300),
            painted_with(vec![soft()]).rasterised(400, 300)
        );
    }

    #[test]
    fn an_inverted_mask_is_one_minus_the_mask() {
        let mask = painted_with(vec![soft()]);
        let inverted = Mask { inverted: true, ..mask.clone() };

        let (plain, turned) = (mask.rasterised(320, 200), inverted.rasterised(320, 200));

        for (w, v) in plain.as_slice().iter().zip(turned.as_slice()) {
            assert_eq!(*v, 1.0 - w);
        }
    }

    #[test]
    fn strokes_paint_over_the_base_resampled_to_the_target() {
        let base = Plane::from_vec(2, 1, vec![0.0, 1.0]).unwrap();
        let dot = Stroke::new(false, 0.05, 0.0, vec![0.25, 0.5]);
        let mask = Mask { base: Some(Base::new(base)), strokes: vec![dot], inverted: false };

        let plane = mask.rasterised(200, 100);

        assert_eq!(plane.get(199, 50), 1.0, "the base's white half is not white");
        assert_eq!(plane.get(5, 5), 0.0, "the base's black half is not black");
        assert_eq!(plane.get(50, 50), 1.0, "the dot was not painted over the black half");
    }

    #[test]
    fn values_out_of_range_are_clamped_on_the_wire_and_an_odd_coordinate_dropped() {
        let stroke: Stroke =
            serde_json::from_str(r#"{"erase":true,"radius":9,"softness":-1,"points":[0.5,1.5,-0.2]}"#).unwrap();
        assert_eq!(stroke, Stroke { erase: true, radius: RADIUS_MAX, softness: 0.0, points: vec![0.5, 1.0] });

        let tiny: Stroke = serde_json::from_str(r#"{"radius":0,"softness":3,"points":[]}"#).unwrap();
        assert_eq!((tiny.erase, tiny.radius, tiny.softness), (false, RADIUS_MIN, 1.0));
    }

    #[test]
    fn a_value_that_is_not_a_number_is_zero_rather_than_handed_on() {
        let stroke = Stroke::new(false, f32::NAN, f32::NAN, vec![f32::NAN, 0.5, f32::INFINITY, f32::NEG_INFINITY]);
        assert_eq!(
            stroke,
            Stroke { erase: false, radius: RADIUS_MIN, softness: 0.0, points: vec![0.0, 0.5, 1.0, 0.0] }
        );

        let base = Base::new(Plane::from_vec(2, 1, vec![f32::NAN, 0.5]).unwrap());
        assert_eq!(base.plane().as_slice(), [0.0, 0.5]);

        // And so a rasterised mask never carries one.
        let mask = Mask { base: Some(base), strokes: vec![stroke], inverted: true };
        assert!(mask.rasterised(20, 10).as_slice().iter().all(|w| (0.0..=1.0).contains(w)));
    }

    #[test]
    fn a_mask_survives_a_round_trip_through_its_wire_shape_without_its_base() {
        let mask = Mask { inverted: true, ..painted_with(vec![soft(), Stroke { erase: true, ..soft() }]) };

        let json = serde_json::to_string(&mask).unwrap();
        assert!(json.contains(r#""inverted":true"#) && json.contains(r#""softness""#), "{json}");
        assert_eq!(serde_json::from_str::<Mask>(&json).unwrap(), mask);

        // The base is built in-process and never sent.
        let based = Mask { base: Some(Base::new(Plane::new(2, 2, 1.0))), ..Mask::default() };
        assert!(!serde_json::to_string(&based).unwrap().contains("base"));
        assert_eq!(serde_json::from_str::<Mask>("{}").unwrap(), Mask::default());
    }

    #[test]
    fn the_signature_changes_with_every_part_of_the_mask() {
        let mask = painted_with(vec![soft()]);
        let signature = mask.signature();
        assert_eq!(signature, mask.clone().signature(), "the same mask signed twice differently");

        let mut changed = vec![
            painted_with(vec![Stroke { erase: true, ..soft() }]),
            painted_with(vec![Stroke { radius: 0.09, ..soft() }]),
            painted_with(vec![Stroke { softness: 0.5, ..soft() }]),
            painted_with(vec![Stroke { points: vec![0.2, 0.3, 0.7, 0.6, 0.8, 0.41], ..soft() }]),
            painted_with(vec![soft(), soft()]),
            Mask { inverted: true, ..mask.clone() },
            Mask { base: Some(Base::new(Plane::new(4, 3, 0.5))), ..mask.clone() },
            Mask { base: Some(Base::new(Plane::new(4, 3, 0.25))), ..mask.clone() },
            Mask { base: Some(Base::new(Plane::new(3, 4, 0.5))), ..mask.clone() },
        ]
        .into_iter()
        .map(|mask| mask.signature())
        .collect::<Vec<_>>();
        changed.push(signature);

        let mut distinct = changed.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), changed.len(), "two different masks share a signature: {changed:?}");
    }

    #[test]
    fn a_mask_with_nothing_in_it_is_empty_even_inverted() {
        assert!(Mask::default().is_empty());
        assert!(Mask { inverted: true, ..Mask::default() }.is_empty());
        assert!(!painted_with(vec![soft()]).is_empty());
        assert!(!Mask::from_image(&DynamicImage::ImageLuma8(ImageBuffer::new(2, 2))).is_empty());
    }

    #[test]
    fn an_images_luminance_is_the_weight_and_its_alpha_is_ignored() {
        // White, black, and pure green, whose Rec. 709 luma is 0.7152; every pixel fully transparent, which a reading
        // that premultiplied would take for black.
        let colours = [[255, 255, 255], [0, 0, 0], [0, 255, 0]];
        let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(3, 1, |x, _| {
            let [r, g, b] = colours[x as usize];
            Rgba([r, g, b, 0])
        }));

        let base = Mask::from_image(&image).base.unwrap();

        assert!((base.plane().get(0, 0) - 1.0).abs() < 1e-6);
        assert_eq!(base.plane().get(1, 0), 0.0);
        assert!((base.plane().get(2, 0) - 0.7152).abs() < 1e-6);
    }

    #[test]
    fn a_sixteen_bit_mask_keeps_its_sixteen_bits() {
        // Two greys one 16-bit level apart, which an 8-bit reading would make one.
        let image = DynamicImage::ImageLuma16(ImageBuffer::from_fn(2, 1, |x, _| Luma([30_000 + x as u16])));
        let colour = DynamicImage::ImageRgb16(ImageBuffer::from_fn(2, 1, |x, _| Rgb([30_000 + x as u16; 3])));

        for image in [image, colour] {
            let base = Mask::from_image(&image).base.unwrap();
            assert!(base.plane().get(1, 0) > base.plane().get(0, 0), "the low byte was lost");
        }
    }
}
