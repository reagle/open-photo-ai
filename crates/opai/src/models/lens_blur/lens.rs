//! The photograph's own lens in 35 mm full-frame-equivalent terms, and how much blur a wider aperture adds to it.
//!
//! All of it is in equivalent terms: the focal length a full-frame camera would need for the same field of view, and
//! the f-number times the crop factor. The blur a lens adds is then the same fraction of the frame on every camera at
//! the same equivalent settings, so the maths needs no sensor size, and f/1.4 looks the same on a phone's photograph as
//! on a camera's. The thin-lens circle of confusion in equivalent terms is the real one scaled by the crop factor, so
//! this is exact rather than an approximation.

use serde::Serialize;

use crate::image::LensExif;
use crate::models::f_number::FNumber;

/// The full-frame diagonal, in millimetres: what the output's diagonal in pixels stands for.
const FULL_FRAME_DIAGONAL_MM: f32 = 43.267;

/// The focal length assumed where neither the EXIF nor the depth model says: a normal lens.
const ASSUMED_FOCAL_35MM: f32 = 50.0;

/// The photograph's own equivalent f-number where the EXIF does not give one, or no crop factor to scale it by.
///
/// A phone photograph is typically f/6 to f/10 equivalent and a kit lens's f/6 to f/9, so assuming f/8 adds a little
/// more blur than the truth to a deep-focus photograph with no EXIF, and never less than was asked for to a shallow
/// one.
pub(crate) const ASSUMED_F_NUMBER: f32 = 8.0;

/// The crop factors believed of a real camera, from a large-format back to the smallest phone sensor. One outside is
/// taken as the product of a garbled field, or of the depth model's estimate gone wrong.
const CROPS: std::ops::RangeInclusive<f32> = 0.3..=12.0;

/// The nearest a lens can be focused, as a multiple of its focal length: a closer focus point is moved out to here.
const NEAREST_FOCUS: f32 = 1.5;

/// The largest radius a lens blur adds, as a fraction of the output's diagonal. A foreground a few centimetres from
/// the lens would otherwise ask for discs larger than the photograph.
const MAX_RADIUS: f32 = 0.06;

/// The widest aperture a blur amount of 100 stands for: f/1.4 in full-frame-equivalent terms, as wide as a fast prime
/// opened all the way and about as shallow a depth of field as a photograph is made at. See
/// [`OwnLens::f_number_for_amount`].
pub(crate) const AMOUNT_WIDEST_F_NUMBER: f64 = 1.4;

/// The range a [`Lens::depth_scale`] is held to: a ratio of the EXIF's focal length to the depth model's estimate
/// outside it is taken as a garbled field or an estimate gone wrong, not as a scene four times nearer or eight times
/// farther than the model read it.
const DEPTH_SCALES: std::ops::RangeInclusive<f32> = 0.25..=8.0;

/// Where a [`Lens`]'s numbers came from: what a front end says beside them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LensOrigin {
    /// The focal length and the f-number are both the photograph's EXIF.
    Exif,
    /// The f-number is the EXIF's, scaled by a crop factor the depth model's focal estimate implied.
    Estimated,
    /// The photograph's own f-number is not known and is assumed, to f/8 equivalent.
    Assumed,
}

/// The lens the photograph was taken with, in 35 mm full-frame-equivalent terms.
///
/// Rounded on construction — the focal length to a tenth of a millimetre, the f-number to a hundredth — which is the
/// precision it is folded into a lens blur's cache tag at, so two lenses that tag alike blur alike.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Lens {
    /// The 35 mm-equivalent focal length, in millimetres.
    pub focal_35mm: f32,
    /// The equivalent f-number the photograph was taken at: how much blur it already has.
    pub own_f_number: f32,
    /// Where the two came from.
    pub origin: LensOrigin,
    /// How many times farther everything is than the depth map says: the focal length over the depth model's own
    /// estimate of it where the focal length is the EXIF's, else 1.
    ///
    /// The depth model reads metric distances consistent with the focal length it estimates for the picture, not the
    /// lens's. Under a pinhole camera a point at `(X, Z)` lands at `x = f · X / Z` on the sensor, so the same object
    /// the same size in the picture is at `Z ∝ f`: a scene the model takes for a 52 mm view is read at 52/181 of its
    /// distance from a 181 mm lens, and every disparity is `181/52` times too large — and so is every radius, since a
    /// radius is linear in the disparity's distance from the focus's. Scaling every distance by `f_exif / f_depth`
    /// puts the scene back where the lens saw it. Rounded to a thousandth, and held to 0.25..=8.
    pub depth_scale: f32,
}

impl Lens {
    /// A lens of `focal_35mm` at `own_f_number`, both equivalent, rounded as the cache tag spells them.
    pub fn new(focal_35mm: f32, own_f_number: f32, origin: LensOrigin) -> Self {
        Self {
            focal_35mm: (focal_35mm * 10.0).round() / 10.0,
            own_f_number: (own_f_number * 100.0).round() / 100.0,
            origin,
            depth_scale: 1.0,
        }
    }

    /// This lens, with the depth map's distances scaled by `scale` (see [`depth_scale`](Self::depth_scale)): rounded
    /// to a thousandth and held to its range, and 1 for a scale that is not a number.
    pub(crate) fn with_depth_scale(self, scale: f32) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            (scale.clamp(*DEPTH_SCALES.start(), *DEPTH_SCALES.end()) * 1000.0).round() / 1000.0
        } else {
            1.0
        };

        Self { depth_scale: scale, ..self }
    }

    /// The lens `exif` records for a photograph framed as the file was shot or cut from it, with what the EXIF leaves
    /// out filled in from the depth model's focal estimate (`depth_focal_35mm`) and then from assumptions.
    ///
    /// `framing` is how many times the file's diagonal is the photograph's, in pixels: 1 for the whole frame, 2 for a
    /// crop to half its width. The EXIF describes the whole frame, so what it gives is scaled to the photograph.
    ///
    /// - **Crop factor**: the 35 mm equivalent over the focal length; else 36 mm over the sensor's width from the
    ///   focal-plane resolution; else, where only the focal length is known, the depth model's equivalent over it.
    /// - **Focal length**: the EXIF's 35 mm equivalent; else the focal length times the crop factor; else the depth
    ///   model's estimate; else 50 mm.
    /// - **Own f-number**: the EXIF's times the crop factor; else an assumed f/8 equivalent.
    ///
    /// # A crop is a smaller sensor
    ///
    /// Cutting the photograph to `1/k` of its diagonal (`k` is `framing`) is a sensor `1/k` the size behind the same
    /// lens, so its crop factor is `k` times larger and the equivalent focal length and f-number, the real ones times
    /// the crop factor, are both `k` times larger. What [`Blur::new`] makes of that is a radius proportional to
    /// `F² / N` and to the output's diagonal in pixels: the crop's `F → kF`, `N → kN` and diagonal `→ D / k` leave
    /// `k² / k / k = 1`, so the same lens at the same physical aperture blurs the same scene content by the same number
    /// of pixels cropped or whole. Without the factor the crop would be rendered as the whole frame, at `1/k` of the
    /// blur and with a cutoff `k` times too low, so a photograph opened wide enough to blur would be left alone.
    ///
    /// The f-number asked of a blur is in the equivalent terms of the photograph as framed, so asking `f/N` of a crop
    /// is asking `f/(N/k)` of the whole frame: wider, as the narrower view of the same lens would be.
    ///
    /// The depth model's own estimates are of the cropped picture and are not scaled again.
    ///
    /// # The depth's scale
    ///
    /// Where the focal length is the EXIF's (its 35 mm equivalent, or its focal length times a measured crop factor)
    /// and the depth model estimated its own, the depth map's distances are scaled by the one over the other: see
    /// [`depth_scale`](Self::depth_scale). Where the focal length is the depth model's, or neither says, they are
    /// taken as read.
    pub fn resolve(exif: &LensExif, depth_focal_35mm: Option<f32>, framing: f32) -> Self {
        let framing = sane_framing(framing);
        let measured_crop = measured_crop(exif);
        let estimated_crop = || {
            exif.focal_mm
                .zip(depth_focal_35mm)
                .map(|(focal, equivalent)| equivalent / focal)
                .filter(|crop| CROPS.contains(crop))
        };
        // The crop of the photograph, not of the file: the EXIF's scaled by the framing, and the estimate's, which
        // came from the framed picture and is already that.
        let (crop, estimated) = match measured_crop {
            Some(crop) => (Some(crop * framing), false),
            None => (estimated_crop(), true),
        };

        // The focal length the EXIF settles alone, then one from the depth model's crop (which is its own estimate
        // again), then its estimate, then a normal lens.
        let exif_focal = exif
            .focal_35mm
            .map(|equivalent| equivalent * framing)
            .or_else(|| exif.focal_mm.zip(measured_crop).map(|(focal, crop)| focal * crop * framing));
        let focal_35mm = exif_focal
            .or_else(|| exif.focal_mm.zip(crop).map(|(focal, crop)| focal * crop))
            .or(depth_focal_35mm)
            .unwrap_or(ASSUMED_FOCAL_35MM);

        let lens = match exif.f_number.zip(crop) {
            Some((f_number, crop)) => {
                Self::new(focal_35mm, f_number * crop, if estimated { LensOrigin::Estimated } else { LensOrigin::Exif })
            }
            None => Self::new(focal_35mm, ASSUMED_F_NUMBER * framing, LensOrigin::Assumed),
        };

        // The depth model's distances are consistent with its own focal estimate: scaled to the EXIF's lens.
        match exif_focal.zip(depth_focal_35mm.filter(|focal| focal.is_finite() && *focal > 0.0)) {
            Some((exif_focal, depth_focal)) => {
                let scale = exif_focal / depth_focal;
                if !DEPTH_SCALES.contains(&scale) {
                    tracing::debug!(exif_focal, depth_focal, scale, "the depth's scale is out of range; clamped");
                }
                let lens = lens.with_depth_scale(scale);
                tracing::debug!(exif_focal, depth_focal, depth_scale = lens.depth_scale, "the depth's scale");
                lens
            }
            None => lens,
        }
    }

    /// The lens as `exif` alone settles it, where nothing the depth model says could change the photograph's own
    /// f-number — which is all a blur that adds nothing needs to know — and `None` where it could.
    ///
    /// The own f-number leans on the depth model in one case: the EXIF has a focal length and an f-number but no 35 mm
    /// equivalent or sensor size, so the crop factor is the depth model's estimate over the focal length.
    pub(crate) fn without_depth(exif: &LensExif, framing: f32) -> Option<Self> {
        let leans_on_depth = exif.f_number.is_some() && exif.focal_mm.is_some() && measured_crop(exif).is_none();

        (!leans_on_depth).then(|| Self::resolve(exif, None, framing))
    }

    /// How this lens is spelled in a cache tag: `l36-6`, focal length and own f-number with no trailing zeros.
    ///
    /// A depth scale other than 1 adds `-z` and the scale, `l181-8.38-z3.454`, so the tag of a lens whose distances are
    /// taken as read is what it always was.
    pub(crate) fn tag(&self) -> String {
        let tag = format!("l{}-{}", trimmed(self.focal_35mm, 1), trimmed(self.own_f_number, 2));

        if self.depth_scale.to_bits() == 1.0_f32.to_bits() {
            tag
        } else {
            format!("{tag}-z{}", trimmed(self.depth_scale, 3))
        }
    }
}

/// The crop factor `exif` records: the 35 mm equivalent over the focal length, else the full frame's diagonal over the
/// sensor's, if believable.
fn measured_crop(exif: &LensExif) -> Option<f32> {
    match (exif.focal_35mm, exif.focal_mm, exif.sensor_diagonal_mm) {
        (Some(equivalent), Some(focal), _) => Some(equivalent / focal),
        (_, _, Some(diagonal)) => Some(FULL_FRAME_DIAGONAL_MM / diagonal),
        _ => None,
    }
    .filter(|crop| CROPS.contains(crop))
}

/// `framing` where it is a ratio of diagonals a crop can have, else 1: a picture is never larger than its frame.
fn sane_framing(framing: f32) -> f32 {
    if framing.is_finite() && framing >= 1.0 { framing } else { 1.0 }
}

/// What a photograph's EXIF alone says of its own lens, in the same equivalent terms as a [`Lens`] and for the picture as
/// it is framed: what a front end shows before any depth is estimated, so the f-number a slider ends at is the one a run
/// blurs from.
///
/// The lens a run settles from the EXIF with no depth model to lean on, which is every photograph whose EXIF gives a
/// crop factor, and the assumed f/8 for the rest. One case differs from the run, which then asks the depth model: an
/// EXIF with a focal length and an f-number but no 35 mm equivalent or sensor size, whose crop factor only the depth
/// model's focal estimate gives. Here that is the assumed f/8 too, which the run's [`LensOrigin::Estimated`] refines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OwnLens {
    /// The equivalent f-number the picture was taken at, or the f/8 assumed (times the framing) where the EXIF cannot
    /// say.
    pub own_f_number: f32,
    /// The 35 mm-equivalent focal length, in millimetres, where the EXIF gives it or a focal length and a crop factor
    /// to make it from: `None` where only the depth model could say.
    pub focal_35mm: Option<f32>,
    /// Where the f-number came from: [`LensOrigin::Exif`] or [`LensOrigin::Assumed`], never `Estimated`.
    pub origin: LensOrigin,
    /// The crop factor of the picture as it is framed, where the EXIF gives one to measure (the 35 mm equivalent over
    /// the focal length, or the sensor's size) times the framing: what a front end divides an equivalent f-number by
    /// to say the real one. `None` where only the depth model could say, so no real f-number is claimed.
    pub crop_factor: Option<f32>,
}

impl OwnLens {
    /// What `exif` says of the lens of a picture `framing` times smaller across than its file: 1 for the whole frame, 2
    /// for a crop to half of it. See [`Lens::resolve`].
    pub fn from_exif(exif: &LensExif, framing: f32) -> Self {
        let lens = Lens::without_depth(exif, framing).unwrap_or_else(|| Lens::resolve(exif, None, framing));
        let focal_known = exif.focal_35mm.is_some() || exif.focal_mm.zip(measured_crop(exif)).is_some();

        Self {
            own_f_number: lens.own_f_number,
            focal_35mm: focal_known.then_some(lens.focal_35mm),
            origin: lens.origin,
            crop_factor: measured_crop(exif).map(|crop| (crop * sane_framing(framing) * 1000.0).round() / 1000.0),
        }
    }

    /// Whether a blur can be added to this photograph at all: its own f-number is wider than the one a blur amount of
    /// 100 stands for. A lens already at or past f/1.4 equivalent cannot be blurred more than it was.
    pub fn can_blur_more(&self) -> bool {
        // In hundredths, which is what an own f-number is rounded to, so f/1.4 held as an `f32` is not wider than f/1.4.
        let hundredths = |f_number: f64| (f_number * 100.0).round();

        hundredths(f64::from(self.own_f_number)) > hundredths(AMOUNT_WIDEST_F_NUMBER)
    }

    /// The equivalent f-number a blur `amount` from 0 to 100 stands for on this photograph: 0 is the photograph as
    /// taken (its own f-number, which adds nothing) and 100 is [`AMOUNT_WIDEST_F_NUMBER`].
    ///
    /// **Even in blur, not in stops.** A lens blurs by a disc whose diameter goes as `1 / N`, so the amount is laid
    /// out in `1 / N` between the photograph's own and the widest: `1/N = 1/N0 + (a/100) (1/1.4 - 1/N0)`. Halfway is
    /// then half the blur of the widest aperture over what the photograph has, which is what a slider is felt to do,
    /// where an even walk through stops would spend most of its length at apertures that blur little.
    ///
    /// Where the photograph is already as wide as the widest, there is nothing to add and this is its own f-number,
    /// which adds nothing. Rounded to the thousandths [`FNumber`] keeps, and held to its range: an own f-number past
    /// f/32, which a heavy crop can have, comes out as f/32.
    pub fn f_number_for_amount(&self, amount: f64) -> f64 {
        // Held to the range an f-number is taken in, so a heavy crop's "as taken" can be written.
        let own = f64::from(self.own_f_number).min(FNumber::MAX);
        let amount = if amount.is_nan() { 0.0 } else { amount.clamp(0.0, 100.0) };

        let f_number = if self.can_blur_more() {
            1.0 / (1.0 / own + amount / 100.0 * (1.0 / AMOUNT_WIDEST_F_NUMBER - 1.0 / own))
        } else {
            own
        };

        ((f_number * 1000.0).round() / 1000.0).clamp(FNumber::MIN, FNumber::MAX)
    }
}

// By the bits of the three rounded numbers, which is what the cache tag spells: two lenses are one when they blur
// alike. The origin is not in it, since it changes no pixel.
impl PartialEq for Lens {
    fn eq(&self, other: &Self) -> bool {
        self.focal_35mm.to_bits() == other.focal_35mm.to_bits()
            && self.own_f_number.to_bits() == other.own_f_number.to_bits()
            && self.depth_scale.to_bits() == other.depth_scale.to_bits()
    }
}

impl Eq for Lens {}

impl std::hash::Hash for Lens {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.focal_35mm.to_bits().hash(state);
        self.own_f_number.to_bits().hash(state);
        self.depth_scale.to_bits().hash(state);
    }
}

/// `value` at `decimals` places with the trailing zeros and a bare decimal point dropped: `36`, `5.91`, `181.5`.
fn trimmed(value: f32, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");

    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        text
    }
}

/// How large a disc a lens blur adds at each depth: linear in disparity (`1 / metres`), signed — positive in front of
/// the plane of focus, negative behind it — and capped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Blur {
    /// Pixels of added radius per unit of disparity (`1 / m`) away from the plane of focus.
    per_disparity: f32,
    /// The disparity of the plane of focus, as the depth map reads it.
    focus_disparity: f32,
    /// The distance focused at, as the lens saw it.
    focus_m: f32,
    /// The largest radius added either way, in pixels.
    cap: f32,
}

impl Blur {
    /// Whether `lens`, opened to `f_number` (equivalent), adds nothing: it is at or above the photograph's own.
    ///
    /// Settled by the two numbers alone, with no depth and no output, so a chain can know a blur is a no-op before it
    /// pays for either.
    pub(crate) fn adds_nothing(lens: &Lens, f_number: f32) -> bool {
        f_number >= lens.own_f_number
    }

    /// The blur `lens`, opened to `f_number` (equivalent) and focused at `focus_m` metres, adds to a photograph
    /// `diagonal_px` across, or `None` where it adds nothing: at or above the photograph's own f-number.
    ///
    /// `focus_m`, and every disparity [`radius`](Self::radius) is asked of, are the depth map's as read; the lens's
    /// [`depth_scale`](Lens::depth_scale) is applied here, so the thin-lens maths sees the distances the lens saw.
    ///
    /// A thin lens of focal length `f` at `N`, focused at `zf`, images a point at `z` as a disc of diameter
    /// `c = f² zf |1/zf − 1/z| / (N (zf − f))` on the full-frame sensor, which is `diagonal_px / 43.27 mm` pixels a
    /// millimetre on the output — so the radius scales with the output's size. The photograph already holds its own
    /// aperture's disc, and discs add roughly in quadrature, so what is added is `r · √(1 − (N / N0)²)`.
    pub(crate) fn new(lens: &Lens, f_number: f32, focus_m: f32, diagonal_px: f32) -> Option<Self> {
        if Self::adds_nothing(lens, f_number) || diagonal_px.is_nan() || diagonal_px <= 0.0 {
            return None;
        }

        let focal = lens.focal_35mm / 1000.0;
        let scale = lens.depth_scale;
        let focus = (focus_m * scale).max(NEAREST_FOCUS * focal);
        let pixels_per_metre = diagonal_px / (FULL_FRAME_DIAGONAL_MM / 1000.0);
        // Half the disc's diameter per unit of disparity, on the sensor and then in pixels.
        let radius = focal * focal * focus / (f_number * (focus - focal)) / 2.0 * pixels_per_metre;
        let added = (1.0 - (f_number / lens.own_f_number).powi(2)).sqrt();

        // In the map's disparity `d`, the true one is `d / scale`: `r = P (d / scale − 1 / focus)` is
        // `(P / scale) (d − scale / focus)`.
        Some(Self {
            per_disparity: radius * added / scale,
            focus_disparity: scale / focus,
            focus_m: focus,
            cap: MAX_RADIUS * diagonal_px,
        })
    }

    /// The distance focused at, in metres as the lens saw it: the depth map's times the lens's depth scale, and no
    /// nearer than the lens can focus.
    pub(crate) fn focus_metres(&self) -> f32 {
        self.focus_m
    }

    /// The radius added to a pixel at `disparity` (`1 / m`, as the depth map reads it), in pixels.
    pub(crate) fn radius(&self, disparity: f32) -> f32 {
        (self.per_disparity * (disparity - self.focus_disparity)).clamp(-self.cap, self.cap)
    }
}

/// The radius, in pixels and signed, a lens blur adds to a pixel `depth_m` metres away when `lens` is opened to
/// `f_number` and focused at `focus_m`, on an output `diagonal_px` across. See [`Blur::new`].
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the per-pixel form of `Blur`, which the render precomputes instead")
)]
pub(crate) fn added_radius(lens: &Lens, f_number: f32, focus_m: f32, depth_m: f32, diagonal_px: f32) -> f32 {
    Blur::new(lens, f_number, focus_m, diagonal_px).map_or(0.0, |blur| blur.radius(depth_m.recip()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a camera's EXIF holds, as `read_lens` hands it over.
    fn exif(focal: Option<f32>, focal_35mm: Option<f32>, f_number: Option<f32>, sensor: Option<f32>) -> LensExif {
        LensExif { focal_mm: focal, focal_35mm, f_number, sensor_diagonal_mm: sensor, diagonal_px: None }
    }

    #[test]
    fn the_three_test_photographs_resolve_to_their_full_frame_equivalents() {
        // The values the test photographs' EXIF holds, not the photographs: an a6700 at 24 mm f/4 (crop 1.5), a Pixel
        // at 6.9 mm f/1.7 (24 mm equivalent), and the a6700 at 121 mm f/5.6 (181 mm equivalent).
        for (recorded, focal, f_number) in [
            (exif(Some(24.0), Some(36.0), Some(4.0), None), 36.0, 6.0),
            (exif(Some(6.9), Some(24.0), Some(1.7), None), 24.0, 5.91),
            (exif(Some(121.0), Some(181.0), Some(5.6), None), 181.0, 8.38),
        ] {
            let lens = Lens::resolve(&recorded, Some(28.9), 1.0);

            assert_eq!((lens.focal_35mm, lens.own_f_number), (focal, f_number), "{recorded:?}");
            assert_eq!(lens.origin, LensOrigin::Exif);
            assert!((lens.depth_scale - focal / 28.9).abs() < 1e-3, "{recorded:?}: {}", lens.depth_scale);
        }
    }

    #[test]
    fn the_depth_is_scaled_by_the_exif_focal_length_over_the_depth_models() {
        // A 100 mm equivalent the depth model took for 50 mm: it read everything at half its distance.
        let recorded = exif(Some(100.0), Some(100.0), Some(2.0), None);
        let lens = Lens::resolve(&recorded, Some(50.0), 1.0);
        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (100.0, 2.0, LensOrigin::Exif));
        assert_eq!(lens.depth_scale, 2.0);
        assert_eq!(lens.tag(), "l100-2-z2");

        // The same lens with the map's distances doubled by hand: one blur, radius for radius.
        let as_read = Lens::new(100.0, 2.0, LensOrigin::Exif);
        let diagonal = 6000.0_f32.hypot(4000.0);
        let scaled = Blur::new(&lens, 1.4, 5.0, diagonal).expect("adds a blur");
        let doubled = Blur::new(&as_read, 1.4, 10.0, diagonal).expect("adds a blur");
        let unscaled = Blur::new(&as_read, 1.4, 5.0, diagonal).expect("adds a blur");
        assert_eq!(scaled.focus_metres(), 10.0);
        assert_eq!(unscaled.focus_metres(), 5.0);
        for map_metres in [2.5_f32, 4.0, 5.0, 8.0, 20.0] {
            let (map, true_disparity) = (map_metres.recip(), (2.0 * map_metres).recip());
            assert!((scaled.radius(map) - doubled.radius(true_disparity)).abs() < 1e-3, "{map_metres} m");
            // Twice as far, half the disparity from the focus's: half the radius, but for the focal length's small part
            // of the focus distance (100 mm of 5 m against 10 m).
            let ratio = scaled.radius(map) / unscaled.radius(map);
            if unscaled.radius(map) != 0.0 {
                assert!((ratio - 0.5).abs() < 0.02, "{map_metres} m: {ratio}");
            }
        }
        assert_eq!(scaled.radius(0.2), 0.0, "the focus is still the point clicked");
    }

    #[test]
    fn the_depth_is_taken_as_read_where_the_focal_length_is_the_depth_models_own() {
        // No EXIF: the depth model's focal length and its distances, as before.
        let lens = Lens::resolve(&LensExif::default(), Some(28.94), 1.0);
        assert_eq!((lens.depth_scale, lens.tag().as_str()), (1.0, "l28.9-8"));
        // Its crop factor from the depth model's estimate: the focal length is the estimate again.
        let lens = Lens::resolve(&exif(Some(24.0), None, Some(4.0), None), Some(36.7), 1.0);
        assert_eq!((lens.origin, lens.depth_scale), (LensOrigin::Estimated, 1.0));
        // An EXIF focal length but no depth estimate to compare it with.
        assert_eq!(Lens::resolve(&exif(Some(24.0), Some(36.0), Some(4.0), None), None, 1.0).depth_scale, 1.0);
        // And a blur of an unscaled lens is the one it always was.
        let lens = Lens::new(36.0, 6.0, LensOrigin::Exif);
        let blur = Blur::new(&lens, 1.4, 2.0, 7211.1).expect("adds a blur");
        assert_eq!(blur.radius(0.125), added_radius(&lens, 1.4, 2.0, 8.0, 7211.1));
    }

    #[test]
    fn the_depth_scale_is_held_to_a_believable_range_and_in_the_lenss_identity() {
        let recorded = exif(Some(121.0), Some(181.0), Some(5.6), None);
        assert_eq!(Lens::resolve(&recorded, Some(10.0), 1.0).depth_scale, 8.0);
        assert_eq!(Lens::resolve(&exif(Some(10.0), Some(10.0), Some(2.0), None), Some(80.0), 1.0).depth_scale, 0.25);
        // Over a crop, the EXIF's focal length is the crop's, and the depth model's estimate already was.
        assert_eq!(Lens::resolve(&exif(Some(24.0), Some(36.0), Some(4.0), None), Some(36.0), 2.0).depth_scale, 2.0);

        let as_read = Lens::new(181.0, 8.38, LensOrigin::Exif);
        let scaled = Lens::resolve(&recorded, Some(52.4), 1.0);
        assert_eq!(scaled.depth_scale, 3.454);
        assert_eq!(scaled.tag(), "l181-8.38-z3.454");
        assert_ne!(scaled, as_read);
        assert_eq!(scaled, as_read.with_depth_scale(3.4542));
        assert_eq!(as_read.with_depth_scale(f32::NAN).depth_scale, 1.0);
    }

    #[test]
    fn the_crop_comes_from_the_sensor_diagonal_where_the_35mm_equivalent_is_missing() {
        // A 24 mm wide sensor of 3:2 is 28.84 mm across, a crop of 1.5.
        let lens = Lens::resolve(&exif(Some(24.0), None, Some(4.0), Some(28.845)), Some(80.0), 1.0);

        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (36.0, 6.0, LensOrigin::Exif));
    }

    #[test]
    fn a_focal_length_alone_takes_its_crop_from_the_depth_models_estimate() {
        let lens = Lens::resolve(&exif(Some(24.0), None, Some(4.0), None), Some(36.0), 1.0);

        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (36.0, 6.0, LensOrigin::Estimated));
    }

    #[test]
    fn a_photograph_with_no_exif_is_assumed_at_f8_with_the_depth_models_focal_length() {
        let lens = Lens::resolve(&LensExif::default(), Some(28.94), 1.0);
        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (28.9, ASSUMED_F_NUMBER, LensOrigin::Assumed));

        // And a normal lens where not even the depth model could say.
        let lens = Lens::resolve(&LensExif::default(), None, 1.0);
        assert_eq!((lens.focal_35mm, lens.own_f_number), (ASSUMED_FOCAL_35MM, ASSUMED_F_NUMBER));

        // An f-number with nothing to scale it by is as good as none.
        let lens = Lens::resolve(&exif(None, None, Some(1.7), None), Some(27.0), 1.0);
        assert_eq!((lens.own_f_number, lens.origin), (ASSUMED_F_NUMBER, LensOrigin::Assumed));
    }

    #[test]
    fn a_crop_scales_what_the_exif_says_of_the_whole_frame_and_nothing_the_depth_model_said_of_the_crop() {
        // The a6700 at 24 mm f/4, cut to half its width and height: a sensor half the size, a crop factor of 3.
        let lens = Lens::resolve(&exif(Some(24.0), Some(36.0), Some(4.0), None), Some(70.0), 2.0);
        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (72.0, 12.0, LensOrigin::Exif));

        // From the sensor alone, as the 35 mm equivalent's absence leaves it.
        let lens = Lens::resolve(&exif(Some(24.0), None, Some(4.0), Some(28.845)), None, 2.0);
        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (72.0, 12.0, LensOrigin::Exif));

        // The depth model saw the crop, so its estimate is the crop's already: 72 mm over 24 mm is a crop factor of 3.
        let lens = Lens::resolve(&exif(Some(24.0), None, Some(4.0), None), Some(72.0), 2.0);
        assert_eq!((lens.focal_35mm, lens.own_f_number, lens.origin), (72.0, 12.0, LensOrigin::Estimated));
        let lens = Lens::resolve(&LensExif::default(), Some(57.8), 2.0);
        assert_eq!(
            (lens.focal_35mm, lens.own_f_number, lens.origin),
            (57.8, 2.0 * ASSUMED_F_NUMBER, LensOrigin::Assumed)
        );

        // No framing a crop could have is the whole frame.
        for framing in [1.0, 0.5, 0.0, -2.0, f32::NAN, f32::INFINITY] {
            let lens = Lens::resolve(&exif(Some(24.0), Some(36.0), Some(4.0), None), None, framing);
            assert_eq!((lens.focal_35mm, lens.own_f_number), (36.0, 6.0), "{framing}");
        }
    }

    #[test]
    fn a_lens_is_known_without_the_depth_unless_its_crop_factor_would_come_from_it() {
        let whole = |recorded: &LensExif| Lens::without_depth(recorded, 1.0);

        assert!(whole(&exif(Some(24.0), Some(36.0), Some(4.0), None)).is_some());
        assert!(whole(&exif(Some(24.0), None, Some(4.0), Some(28.845))).is_some());
        assert!(whole(&LensExif::default()).is_some());
        // An f-number and a focal length alone: the crop is the depth model's to say.
        assert_eq!(whole(&exif(Some(24.0), None, Some(4.0), None)), None);
        // Nothing to scale or nothing to scale it by is the assumed f/8, whatever the depth model says.
        assert_eq!(whole(&exif(None, None, Some(4.0), None)).map(|lens| lens.own_f_number), Some(ASSUMED_F_NUMBER));
        assert_eq!(
            Lens::without_depth(&exif(Some(24.0), Some(36.0), Some(4.0), None), 2.0),
            Some(Lens::new(72.0, 12.0, LensOrigin::Exif))
        );
    }

    #[test]
    fn a_crop_blurs_the_same_scene_by_the_same_pixels_and_moves_the_cutoff_with_it() {
        let exif = exif(Some(24.0), Some(36.0), Some(4.0), None);
        let whole = Lens::resolve(&exif, None, 1.0);
        let cropped = Lens::resolve(&exif, None, 2.0);
        // 6000 x 4000, and the same photograph cut to half its width and height: the same pixels, a quarter of them.
        let (whole_diagonal, cropped_diagonal) = (6000.0_f32.hypot(4000.0), 3000.0_f32.hypot(2000.0));
        let radius = |lens: &Lens, f_number, diagonal| {
            // Focused at 3 m, a background at 8 m, and a foreground at 1.5 m.
            let blur = Blur::new(lens, f_number, 3.0, diagonal).expect("adds blur");
            (blur.radius(1.0 / 8.0), blur.radius(1.0 / 1.5))
        };

        // The same lens at the same physical aperture: f/2.8 of the whole frame is f/5.6 of the crop's.
        let (far, near) = radius(&whole, 2.8, whole_diagonal);
        let (far_cropped, near_cropped) = radius(&cropped, 5.6, cropped_diagonal);
        assert!(far < -5.0 && near > 5.0, "{far} {near}: too small a blur to tell anything");
        // Within the focal length's own small part of the focus distance, which the two do not share.
        assert!((far_cropped / far - 1.0).abs() < 0.03, "{far} cropped {far_cropped}");
        assert!((near_cropped / near - 1.0).abs() < 0.03, "{near} cropped {near_cropped}");

        // Rendered as the whole frame, the crop would have the blur at the diagonal it has and half of it.
        let (unscaled, _) = radius(&whole, 2.8, cropped_diagonal);
        assert!((unscaled / far - 0.5).abs() < 0.02, "{unscaled} against {far}");

        // The cutoff is the photograph's own f-number, which a crop has twice of: f/6 whole, f/12 cropped.
        assert!(Blur::adds_nothing(&whole, 6.0) && !Blur::adds_nothing(&whole, 5.9));
        assert!(Blur::adds_nothing(&cropped, 12.0) && !Blur::adds_nothing(&cropped, 11.9));
        assert!(
            Blur::new(&cropped, 8.0, 3.0, cropped_diagonal).is_some(),
            "f/8 of the crop is wider than its own"
        );
        assert!(
            Blur::new(&whole, 8.0, 3.0, whole_diagonal).is_none(),
            "f/8 of the whole is narrower than its own"
        );
    }

    #[test]
    fn the_own_lens_is_the_runs_before_the_depth_model_has_spoken() {
        let own = OwnLens::from_exif(&exif(Some(24.0), Some(36.0), Some(4.0), None), 1.0);
        assert_eq!(
            own,
            OwnLens { own_f_number: 6.0, focal_35mm: Some(36.0), origin: LensOrigin::Exif, crop_factor: Some(1.5) }
        );

        // The focal length from the focal length and the sensor's crop where the 35 mm equivalent is missing: a
        // 28.84 mm diagonal is a crop factor of 1.5.
        let own = OwnLens::from_exif(&exif(Some(24.0), None, Some(4.0), Some(28.84)), 1.0);
        assert_eq!((own.own_f_number, own.focal_35mm, own.origin), (6.0, Some(36.0), LensOrigin::Exif));
        assert!(own.crop_factor.is_some_and(|crop| (crop - 1.5).abs() < 0.001), "{:?}", own.crop_factor);
    }

    #[test]
    fn the_own_lens_of_a_crop_is_its_framings_times_the_wholes() {
        let recorded = exif(Some(24.0), Some(36.0), Some(4.0), None);

        // Cut to half the frame's diagonal: a sensor half the size behind the same lens.
        let own = OwnLens::from_exif(&recorded, 2.0);

        // The crop factor is the whole frame's times the framing, so the real f-number (12 / 3) is the EXIF's, f/4.
        assert_eq!(
            own,
            OwnLens { own_f_number: 12.0, focal_35mm: Some(72.0), origin: LensOrigin::Exif, crop_factor: Some(3.0) }
        );
        // Bit for bit what a run resolves from the same EXIF at the same framing.
        let lens = Lens::resolve(&recorded, None, 2.0);
        assert_eq!((own.own_f_number, own.focal_35mm), (lens.own_f_number, Some(lens.focal_35mm)));
    }

    #[test]
    fn the_own_lens_assumes_f8_and_names_no_focal_length_the_exif_cannot_give() {
        let own = OwnLens::from_exif(&LensExif::default(), 1.0);
        assert_eq!(
            own,
            OwnLens {
                own_f_number: ASSUMED_F_NUMBER,
                focal_35mm: None,
                origin: LensOrigin::Assumed,
                crop_factor: None
            }
        );

        // The assumed f-number is of the frame, so a crop of it is wider-numbered too.
        assert_eq!(OwnLens::from_exif(&LensExif::default(), 1.5).own_f_number, 12.0);

        // A focal length and an f-number with nothing to scale it by: only the depth model could say the crop, so the
        // f-number is assumed and the focal length is not guessed at either.
        let own = OwnLens::from_exif(&exif(Some(6.9), None, Some(1.7), None), 1.0);
        assert_eq!((own.own_f_number, own.focal_35mm, own.origin), (ASSUMED_F_NUMBER, None, LensOrigin::Assumed));
        assert_eq!(own.crop_factor, None, "no real f-number is claimed where the crop is the depth model's to say");

        // The 35 mm equivalent is the EXIF's own, though the f-number is assumed.
        let own = OwnLens::from_exif(&exif(None, Some(28.0), None, None), 1.0);
        assert_eq!(
            (own.own_f_number, own.focal_35mm, own.origin),
            (ASSUMED_F_NUMBER, Some(28.0), LensOrigin::Assumed)
        );
    }

    /// A photograph at `own` equivalent, from nothing but its f-number.
    fn at(own: f32) -> OwnLens {
        OwnLens { own_f_number: own, focal_35mm: None, origin: LensOrigin::Exif, crop_factor: None }
    }

    #[test]
    fn a_blur_amount_runs_from_the_photograph_as_taken_to_f1_4_evenly_in_blur() {
        // The bonsai's f/6 (the a6700 at f/4, crop 1.5): 0 is the photograph, 100 is f/1.4.
        let own = at(6.0);
        assert_eq!(own.f_number_for_amount(0.0), 6.0);
        assert_eq!(own.f_number_for_amount(100.0), 1.4);

        // Halfway in 1/N: 1/N = (1/6 + 1/1.4) / 2 = 0.44048, N = 2.27. The window's `fNumberForAmount` is pinned to the
        // same numbers (`blurAmount.test.ts`).
        assert_eq!(own.f_number_for_amount(50.0), 2.270);
        assert_eq!(own.f_number_for_amount(25.0), 3.294);
        assert_eq!(at(5.91).f_number_for_amount(50.0), 2.264);
        assert_eq!(at(8.0).f_number_for_amount(50.0), 2.383);
        assert_eq!(at(12.0).f_number_for_amount(50.0), 2.507);
        assert_eq!(at(2.8).f_number_for_amount(1.0), 2.772);

        // Each step of the amount adds the same to the disc's size (1/N), whatever the photograph's own aperture.
        for own in [at(2.5), at(6.0), at(12.0), at(32.0)] {
            let disc = |amount| 1.0 / own.f_number_for_amount(amount);
            let step = disc(10.0) - disc(0.0);
            for amount in [20.0, 50.0, 80.0, 100.0] {
                let walked = disc(amount) - disc(0.0);
                assert!(
                    (walked / (step * amount / 10.0) - 1.0).abs() < 0.005,
                    "{} at {amount}: {walked} against {step} a tenth",
                    own.own_f_number
                );
            }
        }
    }

    #[test]
    fn a_blur_amount_of_nothing_adds_nothing_and_every_amount_above_it_does() {
        for own in [at(1.8), at(5.91), at(8.0), at(8.38), at(32.0)] {
            let lens = Lens::new(50.0, own.own_f_number, LensOrigin::Exif);
            let nothing = own.f_number_for_amount(0.0);

            // As the library receives it, a thousandth-quantised f-number, and as it reads the photograph's own.
            let received = FNumber::new(nothing).expect("in range").as_f32();
            assert!(Blur::adds_nothing(&lens, received), "{} at amount 0 is {nothing}", own.own_f_number);
            let a_little = FNumber::new(own.f_number_for_amount(1.0)).expect("in range").as_f32();
            assert!(!Blur::adds_nothing(&lens, a_little), "{} at amount 1 is {a_little}", own.own_f_number);
        }
    }

    #[test]
    fn a_photograph_already_as_wide_as_the_widest_has_nothing_to_add() {
        for own in [at(1.4), at(1.2), at(1.0)] {
            assert!(!own.can_blur_more(), "{}", own.own_f_number);
            assert_eq!(own.f_number_for_amount(100.0), (f64::from(own.own_f_number) * 1000.0).round() / 1000.0);
        }
        assert!(at(1.41).can_blur_more());

        // An own f-number past f/32, which a heavy crop of a small sensor can have, lays its amounts out from f/32.
        assert_eq!(at(48.0).f_number_for_amount(0.0), 32.0);
        assert_eq!(at(48.0).f_number_for_amount(50.0), 2.683);
        // Amounts outside 0..=100 and not-a-number are held to the ends.
        assert_eq!(at(6.0).f_number_for_amount(140.0), 1.4);
        assert_eq!(at(6.0).f_number_for_amount(-5.0), 6.0);
        assert_eq!(at(6.0).f_number_for_amount(f64::NAN), 6.0);
    }

    #[test]
    fn a_lens_is_tagged_with_no_trailing_zeros_and_no_underscore() {
        assert_eq!(Lens::new(36.0, 6.0, LensOrigin::Exif).tag(), "l36-6");
        assert_eq!(Lens::new(24.0, 5.913, LensOrigin::Exif).tag(), "l24-5.91");
        assert_eq!(Lens::new(181.04, 8.4, LensOrigin::Exif).tag(), "l181-8.4");
    }

    #[test]
    fn the_circle_of_confusion_is_the_thin_lens_one_by_hand() {
        // 50 mm at f/2 focused at 2 m: a point at 10 m is a disc of 50² · 2000 · |1/2000 − 1/10000| / (2 · 1950) =
        // 0.51282 mm on the full-frame sensor. On a 6000 x 4000 output, 7211.1 px stand for 43.267 mm, so the radius
        // is 0.25641 · 166.665 = 42.735 px, behind the plane of focus. The own f-number is so narrow that nothing is
        // taken off for the blur the photograph already has (√(1 − (2/2000)²) = 0.9999995).
        let lens = Lens::new(50.0, 2000.0, LensOrigin::Exif);
        let diagonal = 6000.0_f32.hypot(4000.0);

        let far = added_radius(&lens, 2.0, 2.0, 10.0, diagonal);
        assert!((far + 42.735).abs() < 0.01, "{far}");

        // In front, the same distance in disparity is the same radius the other way.
        let near = added_radius(&lens, 2.0, 2.0, 1.0 / (0.5 + 0.4), diagonal);
        assert!((near - 42.735).abs() < 0.01, "{near}");

        // On the plane of focus, nothing.
        assert_eq!(added_radius(&lens, 2.0, 2.0, 2.0, diagonal), 0.0);
    }

    #[test]
    fn the_blur_already_in_the_photograph_is_taken_off_in_quadrature() {
        let wide = Lens::new(50.0, 2000.0, LensOrigin::Exif);
        let own = Lens { own_f_number: 4.0, ..wide };

        let full = added_radius(&wide, 2.0, 2.0, 10.0, 7211.1);
        let added = added_radius(&own, 2.0, 2.0, 10.0, 7211.1);
        let expected = full * (1.0 - 0.25_f32).sqrt();
        assert!((added - expected).abs() < 1e-3, "{added} {expected}");
    }

    #[test]
    fn nothing_is_added_at_or_above_the_photographs_own_f_number() {
        let lens = Lens::new(36.0, 6.0, LensOrigin::Exif);

        for f_number in [6.0, 8.0, 32.0] {
            assert!(Blur::new(&lens, f_number, 2.0, 7211.1).is_none(), "f/{f_number}");
            assert_eq!(added_radius(&lens, f_number, 2.0, 30.0, 7211.1), 0.0, "f/{f_number}");
        }
        assert!(added_radius(&lens, 5.9, 2.0, 30.0, 7211.1) < 0.0);
    }

    #[test]
    fn radii_scale_with_the_output_and_are_capped_at_a_fraction_of_its_diagonal() {
        let lens = Lens::new(36.0, 6.0, LensOrigin::Exif);

        let small = added_radius(&lens, 1.4, 2.0, 8.0, 2000.0);
        let large = added_radius(&lens, 1.4, 2.0, 8.0, 4000.0);
        assert!((large - 2.0 * small).abs() < 1e-3, "{small} {large}");

        // A foreground a few centimetres away asks for more than the cap.
        assert_eq!(added_radius(&lens, 1.0, 2.0, 0.05, 2000.0), MAX_RADIUS * 2000.0);
    }

    #[test]
    fn a_focus_closer_than_the_lens_can_focus_is_moved_out_to_where_it_can() {
        let lens = Lens::new(50.0, 8.0, LensOrigin::Exif);

        // 1 cm is inside the focal length, where the formula has no meaning: the guard focuses at 7.5 cm instead.
        let radius = added_radius(&lens, 2.0, 0.01, 1.0, 7211.1);
        assert!(radius.is_finite() && radius < 0.0, "{radius}");
        assert_eq!(radius, added_radius(&lens, 2.0, 0.075, 1.0, 7211.1));
    }
}
