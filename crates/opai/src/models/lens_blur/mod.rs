//! The lens blur family: a depth of field a real lens would have given the photograph, focused at a point and opened
//! to an f-number, driven by a depth map rather than by a graph of its own.
//!
//! The depth and the photograph's own lens reach an operation by injection, as faces reach a face recovery: a chain
//! holding a lens blur estimates the depth of its input first (once, cached) and reads the lens from the source's
//! EXIF, and hands both to every lens blur in it — see `inference::depth_map`. What a run then does is
//! [`render`]'s: the depth brought to the output's size along the photograph's edges, the radius a thin lens would
//! add at every pixel, and the layered render of `imaging::defocus`.
//!
//! **The subject's edges are matted.** Where the blur is large enough to show it, the chain also builds a trimap of the
//! subject in focus from the depth ([`trimap`]), has the matting model estimate its edge — every strand of hair or
//! fur and every leaf tip that is part sharp subject and part blurred background — and hands the blur the [`Matte`].
//! The render then fills the background in behind the subject, blurs it, and composites the subject over it as one
//! sharp layer, so the street seen between the strands of a fur hood is blurred and the strands are not.

pub(crate) mod gain_map;
pub(crate) mod lens;
pub(crate) mod render;
pub(crate) mod trimap;
pub(crate) mod variant;

use std::sync::Arc;

use imaging::plane::Plane;
use serde::{Deserialize, Serialize};

use super::artifact::{ArtifactId, Family};
use super::depth::{Depth, DepthMap};
use super::f_number::FNumber;
use super::fraction::Fraction;
use super::handle::PlaneHandle;
use super::matting::{Matte, Matting};
use super::operation::Operation;
use super::precision::{FloatPrecision, Precision};

use crate::error::{InferenceError, UnsupportedReason};
use crate::pipeline::{Backend, Shared};
use crate::providers::profile::EpProfile;

use lens::Blur;
pub use lens::{Lens, LensOrigin, OwnLens};
use render::Refined;
pub(crate) use variant::ALL;
pub use variant::LensBlurVariant;

/// The name the catalogue publishes the focus point's horizontal fraction under.
pub const FOCUS_X: &str = "focus_x";

/// The name the catalogue publishes the focus point's vertical fraction under.
pub const FOCUS_Y: &str = "focus_y";

/// How a lens blur treats the edge of the subject in focus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LensBlurEdges {
    /// The subject in focus is matted and composited over the blurred background, so hair, fur and leaf edges stay
    /// sharp with the background between them blurred. Where the blur is too small to show the difference, or the
    /// matting model cannot run, the render is the depth's alone.
    ///
    /// Bilbao's matte of the whole photograph is read first for where the subject has holes — the background seen
    /// between a canopy's leaves or through the loop of a strap, which the depth calls solid — and those are seeded as
    /// background, so they are blurred with it. Where Bilbao is not installed, the subject is matted as [`Solid`](Self::Solid).
    #[default]
    Matte,
    /// The depth alone decides what is sharp, pixel by pixel: the render before matting existed.
    Depth,
    /// Matted as [`Matte`](Self::Matte) is, but taken to be solid: no holes are looked for, and Bilbao is not run.
    Solid,
}

impl LensBlurEdges {
    /// Whether this is [`Matte`](Self::Matte), the default, which a serialized lens blur leaves out.
    fn is_matte(&self) -> bool {
        *self == Self::Matte
    }
}

/// One lens blur: which model's depth drives it, the f-number it opens the lens to, the point it focuses on, and —
/// once a chain has injected them — the depth map and the photograph's own lens.
///
/// The f-number is **35 mm full-frame equivalent**, and the focus point is fractions of the framed photograph (0 the
/// left or top edge). At or above the photograph's own equivalent f-number the photograph is returned unchanged.
///
/// `Clone` rather than `Copy`, as face recovery is: the depth map is a large value, held behind an `Arc` so a clone
/// is a refcount. What is injected is skipped by serde — a serialized lens blur carries its parameters, as a serialized
/// face recovery carries no pixels — and a deserialized one is handed its depth again by the chain it runs in.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LensBlur {
    /// The model and its precision: the depth model the blur is driven by.
    variant: LensBlurVariant,
    /// The equivalent f-number to simulate.
    f_number: FNumber,
    /// The focus point's position across the framed photograph.
    focus_x: Fraction,
    /// And down it.
    focus_y: Fraction,
    /// How the subject's edges are treated. Left out of the serialized form where it is the default, so a front end
    /// that has never heard of it reads and writes lens blurs as it always has.
    #[serde(default, skip_serializing_if = "LensBlurEdges::is_matte")]
    edges: LensBlurEdges,
    /// The depth of the chain's input, injected by the chain.
    #[serde(skip)]
    depth: Option<DepthHandle>,
    /// The photograph's own lens, injected by the chain beside the depth.
    #[serde(skip)]
    lens: Option<Lens>,
    // The masking seam: always `None` and not in the catalogue until masking exists. A brush or a subject mask will
    // hand the render a weight here, and the depth map can be edited before the render without any change to this.
    /// A weight in `0..=1` at any resolution that keeps pixels sharp: each pixel's radius is scaled by one minus it.
    #[serde(skip)]
    keep_sharp: Option<PlaneHandle>,
    /// The subject in focus, matted on the chain's input, injected by the chain after the depth.
    #[serde(skip)]
    matte: Option<MatteHandle>,
}

impl LensBlur {
    /// Lisbon's lens blur at `f_number`, focused at (`focus_x`, `focus_y`), before a chain has handed it a depth map.
    pub fn lisbon(f_number: FNumber, focus_x: Fraction, focus_y: Fraction) -> Operation {
        Operation::LensBlur(Self::new(LensBlurVariant::Lisbon(FloatPrecision::Fp16), f_number, focus_x, focus_y))
    }

    /// The lens blur running `variant` at `f_number`, focused at (`focus_x`, `focus_y`).
    pub fn new(variant: LensBlurVariant, f_number: FNumber, focus_x: Fraction, focus_y: Fraction) -> Self {
        Self {
            variant,
            f_number,
            focus_x,
            focus_y,
            edges: LensBlurEdges::Matte,
            depth: None,
            lens: None,
            keep_sharp: None,
            matte: None,
        }
    }

    /// This blur driven by `map`, the depth of the picture it will run over.
    #[must_use]
    pub fn with_depth(self, map: Arc<DepthMap>) -> Self {
        Self { depth: Some(DepthHandle::new(map)), ..self }
    }

    /// This blur for a photograph taken with `lens`.
    #[must_use]
    pub fn with_lens(self, lens: Lens) -> Self {
        Self { lens: Some(lens), ..self }
    }

    /// This blur treating the subject's edges as `edges` says.
    #[must_use]
    pub fn with_edges(self, edges: LensBlurEdges) -> Self {
        Self { edges, ..self }
    }

    /// This blur compositing the subject `matte` covers, matted on the picture it will run over, over its blurred
    /// background; `refined` is the depth refined to that picture for the trimap, which the render then uses rather
    /// than refining it again. Part of no comparison and no cache key: it is the depth's and the picture's, which are.
    #[must_use]
    pub(crate) fn with_matte(self, matte: Arc<Matte>, refined: Option<Arc<Refined>>) -> Self {
        Self { matte: Some(MatteHandle::new(matte, refined)), ..self }
    }

    /// This blur keeping the pixels `weight` covers sharp, in proportion to it. The masking seam: nothing in the
    /// library sets one yet, and it is not public until something does.
    #[must_use]
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the masking seam: nothing sets a weight until masking exists")
    )]
    pub(crate) fn with_keep_sharp(self, weight: Arc<Plane>) -> Self {
        Self { keep_sharp: Some(PlaneHandle::new(weight)), ..self }
    }

    /// Which model this runs, and at which precision.
    pub const fn variant(&self) -> LensBlurVariant {
        self.variant
    }

    /// The equivalent f-number this simulates.
    pub const fn f_number(&self) -> FNumber {
        self.f_number
    }

    /// The focus point, across and down the framed photograph.
    pub const fn focus(&self) -> (Fraction, Fraction) {
        (self.focus_x, self.focus_y)
    }

    /// Whether this blur adds nothing to a photograph taken with `lens`: it is opened no wider than the photograph's
    /// own aperture. Known before any depth is, so a chain need not estimate one.
    pub(crate) fn adds_nothing_to(&self, lens: &Lens) -> bool {
        Blur::adds_nothing(lens, self.f_number.as_f32())
    }

    /// The depth estimate a chain must run to drive this blur.
    pub fn depth_estimate(&self) -> Depth {
        self.variant.depth()
    }

    /// The depth map this blur was handed, if a chain has handed it one.
    pub fn depth(&self) -> Option<&DepthMap> {
        self.depth.as_ref().map(|handle| &*handle.map)
    }

    /// The photograph's own lens, if a chain has handed it one.
    pub const fn lens(&self) -> Option<Lens> {
        self.lens
    }

    /// How this blur treats the subject's edges.
    pub const fn edges(&self) -> LensBlurEdges {
        self.edges
    }

    /// The matte of the subject this blur was handed, if a chain has handed it one.
    #[cfg(test)]
    pub(crate) fn matte(&self) -> Option<&Matte> {
        self.matte.as_ref().map(|handle| &*handle.matte)
    }

    /// The matting a chain runs to matte this blur's subject: none where it renders from the depth alone.
    pub(crate) fn matting(&self) -> Option<Matting> {
        match self.edges {
            LensBlurEdges::Matte | LensBlurEdges::Solid => Some(self.variant.matting()),
            LensBlurEdges::Depth => None,
        }
    }

    /// Whether a chain looks for the holes in this blur's subject before matting it.
    pub(crate) fn finds_holes(&self) -> bool {
        self.edges == LensBlurEdges::Matte
    }

    /// The model this runs, as a metric is tagged with it: `lb_lisbon_fp16`.
    ///
    /// Its own rather than the depth model's artifact name, which the depth estimate is measured under: this step runs
    /// no graph, and filing its seconds under `dp_lisbon_*` would add a lens blur's render to the depth model's time.
    pub(crate) fn model_tag(&self) -> String {
        format!("{}_{}_{}", Family::LensBlur.prefix(), self.variant.codename(), self.variant.precision())
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(&self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning of the depth model this operation depends on; the blur opens no session.
    pub(crate) fn profile(&self) -> EpProfile {
        self.variant.profile()
    }

    /// The name a user interface shows: `Lisbon (FP16)`. The f-number and the focus are not in it.
    pub fn display_name(&self) -> String {
        self.to_string()
    }

    /// The artifacts that must be on disk before this operation can run: its depth model's graphs, which the chain
    /// opens to estimate the depth this blur is driven by. The blur itself opens none.
    pub fn required_artifacts(&self) -> Vec<ArtifactId> {
        self.variant.depth().required_artifacts()
    }

    /// The pipeline this operation runs.
    ///
    /// # Errors
    ///
    /// [`InferenceError::Unsupported`] with [`UnsupportedReason::Uninjected`] where no chain has handed this blur its
    /// depth map and lens: [`Opai::process`](crate::Opai::process) always does, so this is a caller planning a chain
    /// by hand. A blur whose lens is at or above its aperture needs no depth map, and a chain that knew so did not
    /// estimate one.
    pub(crate) fn pipeline<B: Backend>(&self) -> Result<Shared<B>, InferenceError> {
        let refused = |missing| InferenceError::Unsupported {
            operation: self.display_name(),
            reason: UnsupportedReason::Uninjected { missing },
        };
        // A blur whose lens is known to be at or above its aperture adds nothing and is handed no depth: the chain
        // skipped estimating one, and the render returns the photograph as it is.
        let depth = match (&self.depth, self.lens) {
            (Some(handle), _) => Some(Arc::clone(&handle.map)),
            (None, Some(lens)) if Blur::adds_nothing(&lens, self.f_number.as_f32()) => None,
            (None, _) => return Err(refused("a depth map")),
        };
        let lens = self.lens.ok_or_else(|| refused("the photograph's lens"))?;
        let matte = self.matte.as_ref().filter(|_| self.edges != LensBlurEdges::Depth);

        Ok(Arc::new(render::LensBlurPipeline::new(
            self.display_name(),
            self.required_artifacts(),
            render::Settings {
                f_number: self.f_number.as_f32(),
                focus: (self.focus_x.get(), self.focus_y.get()),
                lens,
            },
            depth,
            self.keep_sharp.as_ref().map(|handle| Arc::clone(handle.plane())),
            // Never a matte for a blur told to render from the depth alone, whoever handed it one.
            matte.map(|handle| Arc::clone(&handle.matte)),
            matte.and_then(|handle| handle.refined.clone()),
        )))
    }

    /// The key the run cache stores this operation's result under:
    /// `lb-lisbon-fp16-n1.4-x0.55-y0.3-d<signature>-l36-6-v3`.
    ///
    /// The render's version, `-v<n>`, so a build whose render or matte join draws differently is
    /// never served an older build's blur.
    ///
    /// The f-number, the focus point, the depth map's signature (`d0` before one is injected) and the lens the blur
    /// used (`l0` before one is), so a photograph whose EXIF differs is never served another's blur. A matte adds its
    /// signature, `-a<signature>`, and rendering from the depth alone by request adds `-e0`; a keep-sharp weight adds
    /// its own signature, `-k<signature>`. Hyphen-separated, so it cannot be mistaken for an underscore-separated
    /// artifact name.
    pub fn cache_tag(&self) -> String {
        let depth = self
            .depth
            .as_ref()
            .map_or_else(|| "0".to_string(), |handle| format!("{:016x}", handle.signature));
        let lens = self.lens.map_or_else(|| "l0".to_string(), |lens| lens.tag());

        let mut tag = format!(
            "{}-{}-{}-n{}-x{}-y{}-d{depth}-{lens}-v{RENDER_VERSION}",
            Family::LensBlur.prefix(),
            self.variant.codename(),
            self.variant.precision(),
            self.f_number,
            self.focus_x,
            self.focus_y,
        );
        // A solid subject is spelled by its matte alone: where no hole was found its matte is the one `Matte` makes, and
        // so is the render.
        match (&self.matte, self.edges) {
            (_, LensBlurEdges::Depth) => tag.push_str("-e0"),
            (Some(matte), LensBlurEdges::Matte | LensBlurEdges::Solid) => {
                tag.push_str(&format!("-a{:016x}", matte.signature));
            }
            (None, LensBlurEdges::Matte | LensBlurEdges::Solid) => {}
        }
        if let Some(weight) = &self.keep_sharp {
            tag.push_str(&format!("-k{:016x}", weight.signature()));
        }

        tag
    }
}

/// The version of what the lens blur renders from its inputs, in its cache tag: raised whenever the render, the fill
/// behind the subject or the join of the mattes draws a different picture from the same depth, matte and settings, so
/// the run cache never serves a blur an older build drew. 2: the background behind the subject filled from the
/// nearest of it.
pub(crate) const RENDER_VERSION: u32 = 3;

impl std::fmt::Display for LensBlur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.variant.label(), self.variant.precision().label())
    }
}

/// A matte as a lens blur holds it: shared, and compared, hashed and printed by its signature, computed once; with the
/// depth its trimap was drawn on, refined to the chain's input, when there is one to hand on.
#[derive(Clone)]
pub(crate) struct MatteHandle {
    matte: Arc<Matte>,
    /// [`Matte::signature`], computed here once.
    signature: u64,
    refined: Option<Arc<Refined>>,
}

impl MatteHandle {
    fn new(matte: Arc<Matte>, refined: Option<Arc<Refined>>) -> Self {
        let signature = matte.signature();

        Self { matte, signature, refined }
    }
}

impl PartialEq for MatteHandle {
    fn eq(&self, other: &Self) -> bool {
        self.signature == other.signature
    }
}

impl Eq for MatteHandle {}

impl std::hash::Hash for MatteHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.signature.hash(state);
    }
}

impl std::fmt::Debug for MatteHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (width, height) = self.matte.dimensions();

        write!(f, "Matte({width}x{height}, {:016x})", self.signature)
    }
}

/// A depth map as a lens blur holds it: shared, and compared, hashed and printed by its signature, computed once.
#[derive(Clone)]
pub(crate) struct DepthHandle {
    map: Arc<DepthMap>,
    /// [`DepthMap::signature`], which hashes the whole map and so is computed here once rather than per comparison.
    signature: u64,
}

impl DepthHandle {
    fn new(map: Arc<DepthMap>) -> Self {
        let signature = map.signature();

        Self { map, signature }
    }
}

impl PartialEq for DepthHandle {
    fn eq(&self, other: &Self) -> bool {
        self.signature == other.signature
    }
}

impl Eq for DepthHandle {}

impl std::hash::Hash for DepthHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.signature.hash(state);
    }
}

impl std::fmt::Debug for DepthHandle {
    /// The signature and the size: a million distances are nothing to print.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (width, height) = self.map.dimensions();

        write!(f, "DepthMap({width}x{height}, {:016x})", self.signature)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::models::test_support::hash_of;
    use crate::pipeline::test_support::NoBackend;

    /// A depth map of two planes: a near square in the middle at `near` metres before a background at `far`.
    pub(crate) fn two_planes(width: u32, height: u32, near: f32, far: f32) -> DepthMap {
        let metres: Vec<f32> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let inside = |at: u32, size: u32| (size / 3..2 * size / 3).contains(&at);
                if inside(x, width) && inside(y, height) { near } else { far }
            })
            .collect();

        DepthMap::new(width, height, &metres, Some(36.0))
    }

    /// A blur at `f_number` focused at the centre, handed a two-plane depth map and a 36 mm f/6 lens.
    pub(crate) fn injected(f_number: f64) -> LensBlur {
        LensBlur::new(
            LensBlurVariant::Lisbon(FloatPrecision::Fp16),
            FNumber::new(f_number).expect("in range"),
            Fraction::CENTRE,
            Fraction::CENTRE,
        )
        .with_depth(Arc::new(two_planes(64, 48, 1.0, 10.0)))
        .with_lens(Lens::new(36.0, 6.0, LensOrigin::Exif))
    }

    fn fraction(value: f64) -> Fraction {
        Fraction::new(value).expect("in range")
    }

    #[test]
    fn the_cache_tag_spells_every_parameter_the_depth_and_the_lens() {
        let Operation::LensBlur(blur) =
            LensBlur::lisbon(FNumber::new(1.4).expect("in range"), fraction(0.55), fraction(0.3))
        else {
            panic!("Lisbon built another family");
        };

        assert_eq!(blur.cache_tag(), "lb-lisbon-fp16-n1.4-x0.55-y0.3-d0-l0-v3");

        let map = Arc::new(two_planes(64, 48, 1.0, 10.0));
        let signature = map.signature();
        let blur = blur.with_depth(map).with_lens(Lens::new(36.0, 6.0, LensOrigin::Exif));
        assert_eq!(blur.cache_tag(), format!("lb-lisbon-fp16-n1.4-x0.55-y0.3-d{signature:016x}-l36-6-v3"));
        assert!(!blur.cache_tag().contains('_'), "{}", blur.cache_tag());
    }

    #[test]
    fn two_depth_maps_two_lenses_and_two_focus_points_are_each_two_operations() {
        let base = injected(1.4);
        let others = [
            base.clone().with_depth(Arc::new(two_planes(64, 48, 1.0, 12.0))),
            base.clone().with_lens(Lens::new(24.0, 5.91, LensOrigin::Exif)),
            LensBlur { focus_x: fraction(0.2), ..base.clone() },
            injected(2.8),
            base.clone().with_keep_sharp(Arc::new(Plane::new(4, 4, 1.0))),
            base.clone().with_matte(Arc::new(Matte::new(2, 2, &[0.0, 0.5, 1.0, 1.0])), None),
            base.clone().with_edges(LensBlurEdges::Depth),
        ];

        for other in others {
            assert_ne!(other, base);
            assert_ne!(other.cache_tag(), base.cache_tag());
            assert_ne!(hash_of(&other), hash_of(&base));
        }

        // The same map handed twice is one operation: the handle compares by the map's signature, not its address.
        assert_eq!(base.clone().with_depth(Arc::new(two_planes(64, 48, 1.0, 10.0))), base);
    }

    #[test]
    fn a_matte_and_the_depth_alone_are_each_spelled_in_the_cache_tag() {
        let base = injected(1.4);
        let matte = Arc::new(Matte::new(2, 2, &[0.0, 0.5, 1.0, 1.0]));
        let signature = matte.signature();

        let matted = base.clone().with_matte(Arc::clone(&matte), None);
        assert_eq!(matted.cache_tag(), format!("{}-a{signature:016x}", base.cache_tag()));

        // The refined depth handed beside it is the depth's and the picture's, already in the key: no other operation.
        let map = two_planes(64, 48, 1.0, 10.0);
        let picture = image::DynamicImage::new_rgb8(64, 48);
        let refined = Arc::new(Refined::of(&map, &picture));
        let handed = base.clone().with_matte(matte, Some(refined));
        assert_eq!(handed, matted);
        assert_eq!(handed.cache_tag(), matted.cache_tag());
        assert_eq!(matted.matte().map(Matte::signature), Some(signature));

        let depth_only = base.clone().with_edges(LensBlurEdges::Depth);
        assert_eq!(depth_only.cache_tag(), format!("{}-e0", base.cache_tag()));
        assert_eq!(depth_only.matting(), None);
        assert_eq!(base.matting(), Some(Matting::porto()));

        // A solid subject is matted by the same model, looks for no holes, and is spelled by its matte.
        let solid = base.clone().with_edges(LensBlurEdges::Solid);
        assert_eq!(solid.matting(), Some(Matting::porto()));
        assert!(base.finds_holes() && !solid.finds_holes() && !depth_only.finds_holes());
        assert_eq!(solid.cache_tag(), base.cache_tag());
        let same = Arc::new(Matte::new(2, 2, &[0.0, 0.5, 1.0, 1.0]));
        assert_eq!(solid.clone().with_matte(same, None).cache_tag(), matted.cache_tag());

        // A blur told to render from the depth alone is spelled as one, whatever it was handed.
        let both = depth_only.clone().with_matte(Arc::new(Matte::new(1, 1, &[1.0])), None);
        assert_eq!(both.cache_tag(), depth_only.cache_tag());
        assert!(!matted.cache_tag().contains('_'), "{}", matted.cache_tag());
    }

    #[test]
    fn the_required_artifacts_are_the_depth_models_graphs() {
        assert_eq!(injected(1.4).required_artifacts(), Depth::lisbon().required_artifacts());
    }

    #[test]
    fn a_blur_no_chain_has_injected_is_refused_naming_what_it_lacks() {
        let bare = LensBlur::new(
            LensBlurVariant::Lisbon(FloatPrecision::Fp16),
            FNumber::new(2.8).expect("in range"),
            Fraction::CENTRE,
            Fraction::CENTRE,
        );

        for (blur, missing) in [
            (bare.clone(), "a depth map"),
            (bare.clone().with_lens(Lens::new(36.0, 6.0, LensOrigin::Exif)), "a depth map"),
            (bare.clone().with_depth(Arc::new(two_planes(8, 8, 1.0, 2.0))), "the photograph's lens"),
        ] {
            let Err(InferenceError::Unsupported { reason, .. }) = blur.pipeline::<NoBackend>() else {
                panic!("{blur:?} reached a pipeline with nothing to drive it");
            };
            assert_eq!(reason, UnsupportedReason::Uninjected { missing });
        }

        assert!(injected(1.4).pipeline::<NoBackend>().is_ok());

        // A blur at or above the photograph's own f-number adds nothing, so it runs without a depth map; but it still
        // needs the lens that says so.
        let wide_open =
            |f_number: f64| LensBlur { f_number: FNumber::new(f_number).expect("in range"), ..bare.clone() };
        let own = Lens::new(36.0, 6.0, LensOrigin::Exif);
        assert!(wide_open(6.0).with_lens(own).pipeline::<NoBackend>().is_ok());
        assert!(wide_open(6.0).pipeline::<NoBackend>().is_err());
        assert!(wide_open(5.9).with_lens(own).pipeline::<NoBackend>().is_err());
    }

    #[test]
    fn a_lens_blur_is_tagged_with_its_own_model_and_never_the_depth_models() {
        assert_eq!(injected(1.4).model_tag(), "lb_lisbon_fp16");
        assert_eq!(injected(2.8).model_tag(), injected(1.4).model_tag());
    }

    #[test]
    fn a_serialized_blur_carries_its_parameters_and_nothing_injected() {
        let json = serde_json::to_string(&Operation::LensBlur(injected(1.4))).expect("serializes");

        assert_eq!(
            json,
            r#"{"family":"lens_blur","variant":{"codename":"lisbon","precision":"fp16"},"f_number":1.4,"focus_x":0.5,"focus_y":0.5}"#
        );

        let Operation::LensBlur(back) = serde_json::from_str(&json).expect("deserializes") else {
            panic!("the tag named another family");
        };
        assert_eq!((back.depth(), back.lens()), (None, None));
        assert_eq!((back.f_number(), back.focus()), (injected(1.4).f_number(), injected(1.4).focus()));
        assert_eq!(back.edges(), LensBlurEdges::Matte);
    }

    #[test]
    fn edges_from_the_depth_alone_are_serialized_and_the_default_is_left_out() {
        let depth_only = injected(1.4).with_edges(LensBlurEdges::Depth);
        let json = serde_json::to_string(&Operation::LensBlur(depth_only)).expect("serializes");

        assert!(json.ends_with(r#""focus_y":0.5,"edges":"depth"}"#), "{json}");
        let Operation::LensBlur(back) = serde_json::from_str(&json).expect("deserializes") else {
            panic!("the tag named another family");
        };
        assert_eq!(back.edges(), LensBlurEdges::Depth);

        let solid = injected(1.4).with_edges(LensBlurEdges::Solid);
        let json = serde_json::to_string(&Operation::LensBlur(solid)).expect("serializes");
        assert!(json.ends_with(r#""edges":"solid"}"#), "{json}");
    }
}
