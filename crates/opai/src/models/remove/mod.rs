//! The remove family: one model, Lima, which takes out what a mask marks and fills it from the photograph around it.
//!
//! # The region contract
//!
//! A removal carries its own [`Mask`] — the same type a run's limiting mask is, so one brush paints both — whose weight,
//! rasterised at the chain input's size, marks a pixel for removal at 0.5 and above. The marks are grown, split into
//! regions, and each region is inferred over a crop of the photograph with context of its own, at the graph's square,
//! then read back and pasted through a feather confined to the hole the graph filled. Nothing beyond the hole changes.
//!
//! What a run is built from, each at the tier its callers put it:
//!
//! - `region`: the arithmetic — the marks, the regions, the crops and their merging, the distance from the marks, the
//!   feather, and the two resamplings between a crop and the square.
//! - `process`: the pipeline, region by region: presented, run, read back, pasted at the run's depth.
//! - `lima`: the graph's contract and its execution-provider profile.

// The family is the fork's alone: the weights were trained on Places365, whose terms are non-commercial research, and
// upstream asks for models whose terms are compatible with its own licence.
//
// A family rather than an option of the run, because it is an enhancement a user adds to a stack, listed by the
// catalogue and built from a row like every other — and first in the apply order, so everything after it treats the
// fill as photograph. Its mask is part of the operation, not of the run: `ProcessOptions::mask` says where the
// enhancements apply, this says what to take out, and the two are painted separately.

pub(crate) mod lima;
#[cfg(test)]
mod live;
mod process;
pub(crate) mod region;
pub(crate) mod variant;

use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};

use super::artifact::{ArtifactId, Family};
use super::operation::Operation;
use super::precision::{FloatPrecision, Precision};
use super::simple::single_artifact;

use crate::mask::{Mask, Plane};
use crate::models::Face;
use crate::pipeline::{Backend, Shared};
use crate::providers::profile::EpProfile;

pub(crate) use variant::ALL;
pub use variant::RemoveVariant;

/// The name the mask a removal takes is published under in the catalogue.
pub const MASK: &str = "mask";

/// One removal: which model, and what it takes out.
///
/// The mask is shared rather than owned, as a run's limiting mask is: one built from a file holds a plane the size of
/// that file, and a batch hands one to every photograph. Two removals are the same operation exactly when they run the
/// same model over masks of one [`signature`](Mask::signature) — the comparison, the hash and the cache tag all read it,
/// taken once when the removal is built.
///
/// Serialised as `{ "variant": …, "mask": { "strokes": […], "inverted": … } }`, the mask in its own wire shape: a base
/// built in-process from a file does not cross the wire.
///
/// Build this; keep it as an [`Operation::Remove`] — see [`Operation`] for which of the two to reach for.
#[derive(Clone, Serialize, Deserialize)]
#[serde(from = "Wire")]
pub struct Remove {
    /// The model and its precision.
    variant: RemoveVariant,
    /// What to take out: weight 0.5 and above.
    #[serde(serialize_with = "shared")]
    mask: Arc<Mask>,
    /// The mask's signature, taken once: every comparison, hash and cache tag reads it.
    #[serde(skip)]
    signature: u64,
}

/// A [`Remove`] as it arrives, before its mask is signed.
#[derive(Deserialize)]
struct Wire {
    variant: RemoveVariant,
    #[serde(default)]
    mask: Mask,
}

impl From<Wire> for Remove {
    fn from(wire: Wire) -> Self {
        Self::new(wire.variant, Arc::new(wire.mask))
    }
}

/// The mask as its own wire shape.
fn shared<S: Serializer>(mask: &Arc<Mask>, serializer: S) -> Result<S::Ok, S::Error> {
    mask.as_ref().serialize(serializer)
}

impl Remove {
    /// The removal running `variant` over `mask`.
    pub fn new(variant: RemoveVariant, mask: Arc<Mask>) -> Self {
        let signature = mask.signature();

        Self { variant, mask, signature }
    }

    /// Lima over `mask`, at FP32, the one precision it is published at.
    ///
    /// Returns the carrier rather than `Self`, so a caller collects operations of several families into one chain with
    /// no per-element wrapping; the typed value stays reachable by matching it.
    pub fn lima(mask: Arc<Mask>) -> Operation {
        Operation::Remove(Self::new(RemoveVariant::Lima(FloatPrecision::Fp32), mask))
    }

    /// Which model this runs, and at which precision.
    pub const fn variant(&self) -> RemoveVariant {
        self.variant
    }

    /// What this takes out.
    pub fn mask(&self) -> &Arc<Mask> {
        &self.mask
    }

    /// Whether this removal has nothing to take out: its mask has no base and no strokes.
    ///
    /// A chain leaves such a removal out before it is planned, so nothing is installed or opened for it and the result
    /// is named as though it had never been asked for.
    pub fn removes_nothing(&self) -> bool {
        self.mask.is_empty()
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(&self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning measured for the model this runs.
    pub(crate) fn profile(&self) -> EpProfile {
        self.variant.profile()
    }

    /// The pipeline this operation runs — the **family's contract seam**, where a variant becomes a run.
    pub(crate) fn pipeline<B: Backend>(&self) -> Shared<B> {
        // One arm and no `Result`: the one variant has a graph, a contract and a profile, and an empty mask is answered
        // by the pipeline with the photograph rather than refused.
        process::pipeline::<B>(self.display_name(), self.artifact(), self.profile(), Arc::clone(&self.mask))
    }

    /// The name a user interface shows: `Lima (FP32)`.
    pub fn display_name(&self) -> String {
        self.to_string()
    }

    /// The artifact serving this operation.
    pub fn artifact(&self) -> ArtifactId {
        single_artifact(Family::Remove, self.variant.codename(), self.precision())
    }

    /// The artifacts that must be on disk before this operation can run: always exactly one.
    pub fn required_artifacts(&self) -> Vec<ArtifactId> {
        vec![self.artifact()]
    }

    /// The key the run cache stores this operation's result under: `rm-lima-fp32-m<signature>`.
    ///
    /// Carries the mask's signature, so a removal of another region is another image, and nothing else: the grow, the
    /// context and the feather are constants of the family, and a change to one of them changes every removal.
    pub fn cache_tag(&self) -> String {
        // Written deliberately rather than derived from the serialized form, so that renaming a field cannot silently
        // invalidate every cached image.
        format!(
            "{}-{}-{}-m{:016x}",
            Family::Remove.prefix(),
            self.variant.codename(),
            self.variant.precision(),
            self.signature
        )
    }
}

impl PartialEq for Remove {
    fn eq(&self, other: &Self) -> bool {
        self.variant == other.variant && self.signature == other.signature
    }
}

impl Eq for Remove {}

impl std::hash::Hash for Remove {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.variant.hash(state);
        self.signature.hash(state);
    }
}

impl std::fmt::Debug for Remove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The signature rather than the strokes, which can run to thousands of points, or a base's millions.
        f.debug_struct("Remove")
            .field("variant", &self.variant)
            .field("mask", &format_args!("{:016x}", self.signature))
            .finish()
    }
}

impl std::fmt::Display for Remove {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.variant.label(), self.variant.precision().label())
    }
}

/// The longest side a removal's mask is rasterised at to be asked about single points: a face's centre is a coarse
/// question, and a 24-megapixel plane per removal to answer it would be the run's largest allocation but one.
const PROBE: u32 = 1024;

/// Where a chain's removals mark a source, for asking about single points: each mask rasterised once, small.
pub(crate) struct RemovedAt {
    planes: Vec<Plane>,
    /// The probe's size over the source's, on each axis.
    scale: (f32, f32),
}

impl RemovedAt {
    /// What `removals` mark on a `width` x `height` source.
    pub(crate) fn new(removals: &[&Remove], width: u32, height: u32) -> Self {
        let ratio = (f64::from(PROBE) / f64::from(width.max(height).max(1))).min(1.0);
        let probe = |length: u32| ((f64::from(length) * ratio).round() as u32).max(1);
        let (probe_width, probe_height) = (probe(width), probe(height));

        Self {
            planes: removals.iter().map(|removal| removal.mask.rasterised(probe_width, probe_height)).collect(),
            scale: (probe_width as f32 / width.max(1) as f32, probe_height as f32 / height.max(1) as f32),
        }
    }

    /// Whether any removal marks the centre of `face`'s box, in the source's pixels.
    pub(crate) fn at_centre_of(&self, face: &Face) -> bool {
        let bounds = face.bounding_box();
        let centre = ((bounds.min.x + bounds.max.x) / 2.0, (bounds.min.y + bounds.max.y) / 2.0);

        self.planes.iter().any(|plane| {
            let at = |value: f32, scale: f32, length: u32| ((value * scale).floor().max(0.0) as u32).min(length - 1);
            let (x, y) = (at(centre.0, self.scale.0, plane.width()), at(centre.1, self.scale.1, plane.height()));

            plane.get(x, y) >= region::MARKED
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::mask::{Base, Plane, Stroke};
    use crate::models::test_support::hash_of;

    /// A mask of one dot at `(x, y)`, in fractions of the photograph.
    pub(crate) fn dot(x: f32, y: f32) -> Arc<Mask> {
        Arc::new(Mask { strokes: vec![Stroke::new(false, 0.05, 0.0, vec![x, y])], ..Mask::default() })
    }

    /// Lima over `mask`, typed.
    fn lima(mask: Arc<Mask>) -> Remove {
        Remove::new(RemoveVariant::Lima(FloatPrecision::Fp32), mask)
    }

    #[test]
    fn lima_names_the_artifact_the_fork_exports() {
        assert_eq!(lima(dot(0.5, 0.5)).artifact().as_str(), "rm_lima_fp32");
        assert_eq!(lima(dot(0.5, 0.5)).required_artifacts(), vec![lima(dot(0.5, 0.5)).artifact()]);
        assert_eq!(lima(dot(0.5, 0.5)).display_name(), "Lima (FP32)");
    }

    #[test]
    fn the_cache_tag_carries_the_masks_signature_and_nothing_spelled_like_an_artifact() {
        let mask = dot(0.25, 0.75);
        let tag = lima(Arc::clone(&mask)).cache_tag();

        assert_eq!(tag, format!("rm-lima-fp32-m{:016x}", mask.signature()));
        assert!(!tag.contains('_'), "{tag}");
        assert_ne!(tag, lima(dot(0.25, 0.7)).cache_tag(), "two regions shared a tag");
    }

    #[test]
    fn two_removals_are_one_operation_exactly_when_their_masks_sign_alike() {
        let (one, again, other) = (lima(dot(0.3, 0.3)), lima(dot(0.3, 0.3)), lima(dot(0.3, 0.31)));

        assert_eq!(one, again);
        assert_eq!(hash_of(&one), hash_of(&again));
        assert_ne!(one, other);

        // A base counts, through its pixels.
        let based =
            |level: f32| lima(Arc::new(Mask { base: Some(Base::new(Plane::new(4, 3, level))), ..Mask::default() }));
        assert_eq!(based(1.0), based(1.0));
        assert_ne!(based(1.0), based(0.0));
    }

    #[test]
    fn a_removal_with_an_empty_mask_removes_nothing() {
        assert!(lima(Arc::new(Mask::default())).removes_nothing());
        assert!(lima(Arc::new(Mask { inverted: true, ..Mask::default() })).removes_nothing());
        assert!(!lima(dot(0.5, 0.5)).removes_nothing());
    }

    #[test]
    fn a_removal_round_trips_through_its_wire_shape() {
        let removal = lima(dot(0.4, 0.6));
        let json = serde_json::to_string(&removal).unwrap();

        assert!(
            json.starts_with(r#"{"variant":{"codename":"lima","precision":"fp32"},"mask":{"strokes":"#),
            "{json}"
        );
        let back: Remove = serde_json::from_str(&json).unwrap();
        assert_eq!(back, removal);
        assert_eq!(back.cache_tag(), removal.cache_tag());

        // No mask on the wire is an empty one.
        let bare: Remove = serde_json::from_str(r#"{"variant":{"codename":"lima","precision":"fp32"}}"#).unwrap();
        assert!(bare.removes_nothing());
    }

    #[test]
    fn a_face_is_removed_where_the_centre_of_its_box_is_marked() {
        use crate::models::face::tests::face_at;

        // A dot of radius 0.05 of the longer side at a quarter across and half down a 4000 x 3000 source: 200 px.
        let removal = lima(dot(0.25, 0.5));
        let marked = RemovedAt::new(&[&removal], 4000, 3000);

        assert!(marked.at_centre_of(&face_at(900.0, 1400.0, 1100.0, 1600.0)), "a face under the dot was spared");
        assert!(!marked.at_centre_of(&face_at(2900.0, 1400.0, 3100.0, 1600.0)), "a face far from it was removed");
        // The box overlaps the dot, but its centre does not.
        assert!(!marked.at_centre_of(&face_at(1150.0, 1400.0, 1650.0, 1600.0)));
    }

    #[test]
    fn a_removal_debugs_as_its_signature_rather_than_its_strokes() {
        let removal = lima(dot(0.4, 0.6));
        let debug = format!("{removal:?}");

        assert!(debug.contains(&format!("{:016x}", removal.mask().signature())), "{debug}");
        assert!(!debug.contains("points"), "{debug}");
    }
}
