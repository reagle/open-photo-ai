//! The refine brush of a lens blur: strokes that say what to keep sharp and what to blur where the depth and the matte
//! got it wrong, as Lightroom's Lens Blur Refine does.
//!
//! A **focus** stroke is the subject. Where the trimap calls a painted pixel surely background (a stray hair the depth
//! read at the sky's distance, a part of the subject nearer than the band) it is left unknown, so the matting model is
//! asked about it; and the matte is then raised to at least the stroke's weight there, so what is painted is kept
//! sharp whatever the matting model answered. The hair the trimap loses is faint, and asked about it the matting model
//! answers only a little more than nothing, so leaving it unknown alone would not keep it sharp. The render also holds
//! what a focus stroke covers in focus, as a mask kept sharp does, so the brush works where nothing was matted too.
//!
//! A **blur** stroke is the background: surely background in the trimap, no subject in the matte, and blurred at
//! least as much as the farthest of the scene would be, in proportion to its weight, so a part the depth put at the
//! subject's distance is blurred all the same.
//!
//! An **erase** stroke takes either away. Strokes apply in order, and each takes away the other kind under it: a focus
//! stroke over a blur stroke is focus. They are [`Stroke`]s, the brush a mask is painted with, in fractions of the
//! framed photograph, so the same strokes are the same refinement at any size.
//!
//! The command line's `--lb-refine` file is a base with no strokes ([`Refine::from_image`]): white is focus, black is
//! blur, and the mid-grey 128 is neither.

// Two masks, one for each kind, so a refinement is painted, rasterised, resampled and signed exactly as a mask is: a
// focus stroke paints the focus mask and erases the blur mask along the same path, a blur stroke the other way round,
// and an erase erases both.

use std::sync::Arc;

use image::DynamicImage;
use imaging::plane::Plane;
use rust_sak::crypto::xxh3_string;
use serde::{Deserialize, Serialize};

use crate::mask::{Base, Mask, Stroke};
use crate::models::Matte;

/// The grey level that is neither focus nor blur in a refinement image: 128 of 255.
const NEUTRAL: f32 = 128.0 / 255.0;

/// The focus weight above which a pixel the trimap calls surely background is left unknown: the edge of a soft brush,
/// where it starts to show.
const FOCUSED: f32 = 0.05;

/// The blur weight at and above which a pixel is surely background in the trimap. Below it, in a soft brush's edge,
/// the matte is still taken down in proportion to the weight.
const BLURRED: f32 = 0.5;

/// One stroke of the refine brush: a mask's [`Stroke`], painting focus, or blur where `blur` is set; an erase takes
/// either away.
///
/// Serialised as a stroke is, with `blur` beside its fields where it is set: `{ "erase": false, "blur": true,
/// "radius": 0.02, "softness": 0.5, "points": [0.4, 0.5] }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RefineStroke {
    /// Whether this stroke paints blur rather than focus. Ignored for an erase, which takes either away.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blur: bool,
    /// The path, the brush and whether it erases.
    #[serde(flatten)]
    pub stroke: Stroke,
}

impl RefineStroke {
    /// `stroke` painting focus, or blur where `blur` is set.
    pub fn new(blur: bool, stroke: Stroke) -> Self {
        Self { blur, stroke }
    }

    /// Whether this stroke paints focus.
    fn focuses(&self) -> bool {
        !self.stroke.erase && !self.blur
    }

    /// Whether this stroke paints blur.
    fn blurs(&self) -> bool {
        !self.stroke.erase && self.blur
    }
}

/// What a lens blur is told to keep sharp and to blur: refine strokes in order, over an optional base image.
///
/// Serialised as `{ "strokes": [...] }`; the base never crosses the wire, as a mask's does not. Compared, hashed and
/// spelled in a cache tag by its [`signature`](Self::signature), taken once.
#[derive(Clone, Serialize, Deserialize)]
#[serde(from = "Wire")]
pub struct Refine {
    /// Painted and erased in order.
    strokes: Vec<RefineStroke>,
    /// Where focus is painted: each focus stroke a paint, every other stroke an erase.
    #[serde(skip)]
    focus: Mask,
    /// Where blur is painted, likewise.
    #[serde(skip)]
    blur: Mask,
    /// XXH3-64 of both masks' signatures.
    #[serde(skip)]
    signature: u64,
}

/// A [`Refine`] as it arrives, before its masks are built.
#[derive(Deserialize)]
struct Wire {
    #[serde(default)]
    strokes: Vec<RefineStroke>,
}

impl From<Wire> for Refine {
    fn from(wire: Wire) -> Self {
        Self::new(wire.strokes)
    }
}

impl Refine {
    /// The refinement `strokes` paint, over nothing.
    pub fn new(strokes: Vec<RefineStroke>) -> Self {
        Self::built(None, strokes)
    }

    /// The refinement `image` is, with no strokes: its luminance (as a mask file's is read, any alpha ignored) above the
    /// mid-grey 128 is focus, rising to all of it at white, and below it blur, rising to all of it at black.
    pub fn from_image(image: &DynamicImage) -> Self {
        let luminance = Mask::from_image(image).base.expect("a mask read from an image has its luminance as a base");
        let plane = luminance.plane();
        let split = |weight: fn(f32) -> f32| {
            let values = plane.as_slice().iter().map(|&value| weight(value)).collect();
            Base::new(Plane::from_vec(plane.width(), plane.height(), values).expect("one weight a pixel"))
        };
        let focus = split(|value| ((value - NEUTRAL) / (1.0 - NEUTRAL)).clamp(0.0, 1.0));
        let blur = split(|value| ((NEUTRAL - value) / NEUTRAL).clamp(0.0, 1.0));

        Self::built(Some((focus, blur)), Vec::new())
    }

    fn built(bases: Option<(Base, Base)>, strokes: Vec<RefineStroke>) -> Self {
        let (focus_base, blur_base) = bases.unzip();
        let mask = |base: Option<Base>, paints: fn(&RefineStroke) -> bool| Mask {
            base,
            strokes: strokes
                .iter()
                .map(|stroke| Stroke { erase: !paints(stroke), ..stroke.stroke.clone() })
                .collect(),
            ..Mask::default()
        };
        let focus = mask(focus_base, RefineStroke::focuses);
        let blur = mask(blur_base, RefineStroke::blurs);
        let signature = xxh3_string(&format!("{:016x}|{:016x}", focus.signature(), blur.signature()));
        let signature = u64::from_str_radix(&signature, 16).expect("an XXH3-64 is sixteen hex digits");

        Self { strokes, focus, blur, signature }
    }

    /// Whether this refines nothing: no base and no strokes.
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.focus.base.is_none()
    }

    /// The strokes, in order.
    pub fn strokes(&self) -> &[RefineStroke] {
        &self.strokes
    }

    /// The size of the base image this was read from, or `None` for strokes alone, which fit any photograph.
    pub fn base_dimensions(&self) -> Option<(u32, u32)> {
        self.focus.base.as_ref().map(|base| (base.plane().width(), base.plane().height()))
    }

    /// XXH3-64 of what this paints: any change to a stroke, to their order or to the base changes it.
    pub fn signature(&self) -> u64 {
        self.signature
    }

    /// The focus and blur weights at every pixel of a `width` x `height` picture.
    pub(crate) fn rasterised(&self, width: u32, height: u32) -> Refinement {
        Refinement { focus: self.focus.rasterised(width, height), blur: self.blur.rasterised(width, height) }
    }
}

impl PartialEq for Refine {
    fn eq(&self, other: &Self) -> bool {
        self.signature == other.signature
    }
}

impl Eq for Refine {}

impl std::hash::Hash for Refine {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.signature.hash(state);
    }
}

impl std::fmt::Debug for Refine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Refine({} strokes, base {:?}, {:016x})",
            self.strokes.len(),
            self.base_dimensions(),
            self.signature
        )
    }
}

/// A refinement rasterised at one size: how much each pixel is painted focus and blur, each in `0..=1`.
pub(crate) struct Refinement {
    focus: Plane,
    blur: Plane,
}

impl Refinement {
    /// The refinement rasterised at `trimap`'s size, laid over it: surely background where blur is painted, and
    /// unknown where focus is painted over what it calls surely background. What it calls surely the subject stays so.
    pub(crate) fn trimap(refine: &Refine, trimap: &Plane) -> Plane {
        let Self { focus, blur } = refine.rasterised(trimap.width(), trimap.height());
        let mut laid = trimap.clone();

        for ((value, &focus), &blur) in laid.as_mut_slice().iter_mut().zip(focus.as_slice()).zip(blur.as_slice()) {
            if blur >= BLURRED {
                *value = 0.0;
            } else if focus > FOCUSED && *value < 0.25 {
                *value = 0.5;
            }
        }

        laid
    }

    /// `matte` with the refinement rasterised at its size laid over it: at least the focus weight wherever focus is
    /// painted, then taken down by the blur weight, `max(a, f) * (1 - b)`.
    pub(crate) fn matte(refine: &Refine, matte: &Matte) -> Matte {
        let (width, height) = matte.dimensions();
        let Self { focus, blur } = refine.rasterised(width, height);
        let alpha: Vec<f32> = matte
            .coverage()
            .iter()
            .zip(focus.as_slice())
            .zip(blur.as_slice())
            .map(|((&alpha, &focus), &blur)| alpha.max(focus) * (1.0 - blur))
            .collect();

        Matte::new(width, height, &alpha)
    }

    /// `radius` (signed, at the refinement's size) blurred toward `background`, the radius of the farthest of the scene,
    /// by the blur weight: a pixel blurred less than it is moved that share of the way to it.
    pub(crate) fn blur_radius(&self, radius: &mut Plane, background: f32) {
        debug_assert_eq!(
            (radius.width(), radius.height()),
            (self.blur.width(), self.blur.height()),
            "a refinement of the picture's size"
        );

        for (value, &blur) in radius.as_mut_slice().iter_mut().zip(self.blur.as_slice()) {
            if blur > 0.0 && value.abs() < background.abs() {
                *value += blur * (background - *value);
            }
        }
    }

    /// What the render keeps sharp: the focus weight, and `keep_sharp` (at any size, a mask's) where it holds more.
    pub(crate) fn kept_sharp(&self, keep_sharp: Option<&Plane>) -> Plane {
        let mut kept = self.focus.clone();
        if let Some(keep) = keep_sharp {
            let keep = keep.resized(kept.width(), kept.height());
            for (value, &keep) in kept.as_mut_slice().iter_mut().zip(keep.as_slice()) {
                *value = value.max(keep);
            }
        }

        kept
    }

    /// A refinement of `width` x `height` with these weights everywhere, for a test.
    #[cfg(test)]
    pub(crate) fn uniform(width: u32, height: u32, focus: f32, blur: f32) -> Self {
        Self { focus: Plane::new(width, height, focus), blur: Plane::new(width, height, blur) }
    }
}

/// A shared refinement, as a lens blur holds it: `None` for one that refines nothing, which is then no refinement at
/// all, its cache tag included.
pub(crate) fn shared(refine: Arc<Refine>) -> Option<Arc<Refine>> {
    (!refine.is_empty()).then_some(refine)
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Luma};

    use super::*;

    /// A dot of the refine brush at `(x, y)`, hard, a tenth of the longer side across.
    fn dot(blur: bool, erase: bool, x: f32, y: f32) -> RefineStroke {
        RefineStroke::new(blur, Stroke::new(erase, 0.05, 0.0, vec![x, y]))
    }

    /// The focus and blur weights at the pixel `(x, y)` of `refine` rasterised at 200 x 100.
    fn weights(refine: &Refine, x: u32, y: u32) -> (f32, f32) {
        let Refinement { focus, blur } = refine.rasterised(200, 100);
        (focus.get(x, y), blur.get(x, y))
    }

    #[test]
    fn a_focus_stroke_paints_focus_and_a_blur_stroke_paints_blur() {
        let refine = Refine::new(vec![dot(false, false, 0.25, 0.5), dot(true, false, 0.75, 0.5)]);

        assert_eq!(weights(&refine, 50, 50), (1.0, 0.0));
        assert_eq!(weights(&refine, 150, 50), (0.0, 1.0));
        assert_eq!(weights(&refine, 100, 10), (0.0, 0.0));
    }

    #[test]
    fn a_later_stroke_takes_the_other_kind_away_and_an_erase_takes_either() {
        let over = Refine::new(vec![dot(true, false, 0.5, 0.5), dot(false, false, 0.5, 0.5)]);
        assert_eq!(weights(&over, 100, 50), (1.0, 0.0), "focus over blur is focus");

        let under = Refine::new(vec![dot(false, false, 0.5, 0.5), dot(true, false, 0.5, 0.5)]);
        assert_eq!(weights(&under, 100, 50), (0.0, 1.0), "blur over focus is blur");

        for blur in [false, true] {
            let erased = Refine::new(vec![dot(blur, false, 0.5, 0.5), dot(!blur, true, 0.5, 0.5)]);
            assert_eq!(weights(&erased, 100, 50), (0.0, 0.0), "an erase took nothing away (blur {blur})");
        }
    }

    #[test]
    fn an_image_is_focus_above_mid_grey_blur_below_and_neither_at_it() {
        let levels = [255_u8, 128, 0, 192];
        let image = DynamicImage::ImageLuma8(ImageBuffer::from_fn(4, 1, |x, _| Luma([levels[x as usize]])));
        let refine = Refine::from_image(&image);
        let Refinement { focus, blur } = refine.rasterised(4, 1);

        assert_eq!(focus.as_slice()[..3], [1.0, 0.0, 0.0]);
        assert_eq!(blur.as_slice()[..3], [0.0, 0.0, 1.0]);
        assert!((focus.as_slice()[3] - 64.0 / 127.0).abs() < 1e-4, "{}", focus.as_slice()[3]);
        assert_eq!(blur.as_slice()[3], 0.0);
        assert!(!refine.is_empty());
        assert_eq!(refine.base_dimensions(), Some((4, 1)));
    }

    #[test]
    fn the_signature_changes_with_any_stroke_and_not_with_the_way_it_is_built() {
        let strokes = vec![dot(false, false, 0.25, 0.5), dot(true, false, 0.75, 0.5)];
        let refine = Refine::new(strokes.clone());

        assert_eq!(Refine::new(strokes.clone()), refine);
        assert_eq!(Refine::new(strokes.clone()).signature(), refine.signature());

        let others = [
            Refine::new(vec![strokes[1].clone(), strokes[0].clone()]),
            Refine::new(vec![strokes[0].clone()]),
            Refine::new(vec![dot(true, false, 0.25, 0.5), strokes[1].clone()]),
            Refine::new(Vec::new()),
        ];
        for other in others {
            assert_ne!(other.signature(), refine.signature(), "{other:?}");
        }
    }

    #[test]
    fn strokes_cross_the_wire_with_blur_beside_the_fields_of_a_stroke() {
        let json = r#"{"strokes":[{"blur":true,"erase":false,"radius":0.02,"softness":0.5,"points":[0.4,0.5]},{"erase":false,"radius":0.02,"softness":0.5,"points":[0.1,0.2]}]}"#;
        let refine: Refine = serde_json::from_str(json).expect("deserializes");

        assert_eq!(refine.strokes().len(), 2);
        assert!(refine.strokes()[0].blur && !refine.strokes()[1].blur);
        assert_eq!(serde_json::to_string(&refine).expect("serializes"), json);

        let empty: Refine = serde_json::from_str("{}").expect("deserializes");
        assert!(empty.is_empty());
        assert!(shared(Arc::new(empty)).is_none());
    }

    #[test]
    fn a_trimap_is_background_under_blur_and_unknown_under_focus_over_background() {
        // Background, unknown, subject; focus over the left half, blur over the last two pixels.
        let trimap = Plane::from_vec(8, 1, vec![0.0, 0.5, 1.0, 0.0, 0.0, 0.5, 1.0, 0.0]).expect("eight values");
        let focus = RefineStroke::new(false, Stroke::new(false, 0.25, 0.0, vec![0.25, 0.5]));
        let blur = RefineStroke::new(true, Stroke::new(false, 0.25, 0.0, vec![0.9375, 0.5]));
        let refine = Refine::new(vec![focus, blur]);
        let laid = Refinement::trimap(&refine, &trimap);

        assert_eq!(laid.as_slice(), [0.5, 0.5, 1.0, 0.5, 0.0, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn a_matte_is_raised_to_focus_and_taken_down_by_blur() {
        let matte = Matte::new(3, 1, &[0.02, 0.9, 0.6]);
        let laid = Refinement::matte(&Refine::new(Vec::new()), &matte);
        assert_eq!(laid, matte, "no stroke, no change");

        let Refinement { focus, blur } = Refinement::uniform(3, 1, 0.8, 0.5);
        let alpha: Vec<f32> = matte
            .coverage()
            .iter()
            .zip(focus.as_slice())
            .zip(blur.as_slice())
            .map(|((&a, &f), &b)| a.max(f) * (1.0 - b))
            .collect();
        assert!((alpha[0] - 0.4).abs() < 1e-3 && (alpha[1] - 0.45).abs() < 1e-3, "{alpha:?}");
    }

    #[test]
    fn blur_moves_a_radius_toward_the_background_and_never_lessens_one() {
        let mut radius = Plane::from_vec(4, 1, vec![0.0, -2.0, -30.0, 5.0]).expect("four values");
        Refinement::uniform(4, 1, 0.0, 0.5).blur_radius(&mut radius, -20.0);

        assert_eq!(radius.as_slice(), [-10.0, -11.0, -30.0, -7.5]);
    }

    #[test]
    fn what_is_kept_sharp_is_the_larger_of_focus_and_the_mask() {
        let refinement = Refinement::uniform(4, 2, 0.3, 0.0);
        let keep = Plane::from_vec(2, 1, vec![0.0, 1.0]).expect("two values");

        assert_eq!(refinement.kept_sharp(None).as_slice(), [0.3; 8]);
        let kept = refinement.kept_sharp(Some(&keep));
        assert_eq!(kept.get(0, 0), 0.3);
        assert_eq!(kept.get(3, 1), 1.0);
    }
}
