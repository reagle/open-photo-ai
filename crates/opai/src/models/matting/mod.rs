//! The matting family: one variant, one graph, and a result that is not an image but the share of each pixel a subject
//! covers.
//!
//! What a matting run is told about the subject is a trimap: a plane at the photograph's size that marks each pixel as
//! sure background (0), sure subject (1) or unknown (0.5). The model estimates the unknown pixels alone, from the
//! photograph and the trimap; the known ones keep the trimap's value. The trimap reaches an operation by injection, as
//! a lens blur's depth does: the lens blur builds one from its depth map around the plane of focus.

pub(crate) mod matte;
pub(crate) mod porto;
pub(crate) mod variant;

#[cfg(test)]
mod live;

use std::sync::Arc;

use imaging::plane::Plane;
use serde::{Deserialize, Serialize};

use super::artifact::{ArtifactId, Family};
use super::handle::PlaneHandle;
use super::precision::{FloatPrecision, Precision};
use super::simple::single_artifact;

use crate::providers::profile::EpProfile;

pub use matte::Matte;
pub(crate) use variant::ALL;
pub use variant::MattingVariant;

/// One matting run: which model, and — once a caller has handed it one — the trimap it estimates the unknown pixels of.
///
/// What a run produces is a [`Matte`], beside [`Depth`](super::Depth)'s map rather than in a chain. The trimap is
/// skipped by serde, as everything injected is: a serialized matting carries its model, and is handed its trimap again
/// by whatever runs it. Two operations are equal when their variants and their trimaps' signatures are.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Matting {
    /// The model and its precision.
    variant: MattingVariant,
    /// What is surely the subject, surely not and unknown, at the photograph's size.
    #[serde(skip)]
    trimap: Option<PlaneHandle>,
}

impl Matting {
    /// The operation running `variant`, before it has been handed a trimap.
    pub const fn new(variant: MattingVariant) -> Self {
        Self { variant, trimap: None }
    }

    /// Porto, this family's model, at the one precision it is published in.
    pub const fn porto() -> Self {
        Self::new(MattingVariant::Porto(FloatPrecision::Fp32))
    }

    /// This operation over `trimap`: 0 sure background, 0.5 unknown, 1 sure subject, at the size of the photograph it
    /// will run over.
    #[must_use]
    pub(crate) fn with_trimap(self, trimap: Arc<Plane>) -> Self {
        Self { trimap: Some(PlaneHandle::new(trimap)), ..self }
    }

    /// The trimap this operation was handed, if any.
    pub(crate) fn trimap(&self) -> Option<&Arc<Plane>> {
        self.trimap.as_ref().map(PlaneHandle::plane)
    }

    /// Which model this runs, and at which precision.
    pub const fn variant(&self) -> MattingVariant {
        self.variant
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(&self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning measured for the model this runs, at the precision it runs at.
    pub(crate) fn profile(&self) -> EpProfile {
        self.variant.profile()
    }

    /// The name a user interface shows: `Porto (FP32)`.
    pub fn display_name(&self) -> String {
        self.to_string()
    }

    /// The one artifact this runs: `mt_porto_fp32`.
    pub fn artifact(&self) -> ArtifactId {
        single_artifact(Family::Matting, self.variant.codename(), self.precision())
    }

    /// The artifacts that must be on disk before this operation can run: its one graph.
    pub fn required_artifacts(&self) -> Vec<ArtifactId> {
        vec![self.artifact()]
    }

    /// The key the run cache stores this operation's result under: `mt-porto-fp32-t<signature>`.
    ///
    /// The variant, the precision and the trimap's signature (`t0` before one is handed), so a matte is served only to
    /// the trimap it was estimated from. The photograph's identity is the other half of the key.
    pub fn cache_tag(&self) -> String {
        let trimap = self
            .trimap
            .as_ref()
            .map_or_else(|| "0".to_string(), |handle| format!("{:016x}", handle.signature()));

        format!(
            "{}-{}-{}-t{trimap}",
            Family::Matting.prefix(),
            self.variant.codename(),
            self.variant.precision()
        )
    }
}

impl std::fmt::Display for Matting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.variant.label(), self.variant.precision().label())
    }
}

#[cfg(test)]
mod tests {
    use super::variant::tests::every_variant;
    use super::*;
    use crate::models::test_support::hash_of;

    fn trimap(unknown: f32) -> Arc<Plane> {
        let mut plane = Plane::new(8, 6, 0.0);
        plane.as_mut_slice()[20] = unknown;

        Arc::new(plane)
    }

    #[test]
    fn porto_names_one_graph() {
        assert_eq!(Matting::porto().artifact().as_str(), "mt_porto_fp32");
        assert_eq!(
            Matting::porto().required_artifacts().iter().map(ArtifactId::as_str).collect::<Vec<_>>(),
            vec!["mt_porto_fp32"]
        );
    }

    #[test]
    fn the_cache_tag_carries_the_trimap_and_is_never_an_artifact_name() {
        let bare = Matting::porto();
        assert_eq!(bare.cache_tag(), "mt-porto-fp32-t0");

        let handed = bare.clone().with_trimap(trimap(0.5));
        let signature = PlaneHandle::new(trimap(0.5)).signature();
        assert_eq!(handed.cache_tag(), format!("mt-porto-fp32-t{signature:016x}"));
        assert!(!handed.cache_tag().contains('_'), "{}", handed.cache_tag());

        // Another trimap is another operation; the same one handed twice is one.
        let other = bare.clone().with_trimap(trimap(1.0));
        assert_ne!(other, handed);
        assert_ne!(other.cache_tag(), handed.cache_tag());
        assert_ne!(hash_of(&other), hash_of(&handed));
        assert_eq!(bare.with_trimap(trimap(0.5)), handed);
    }

    #[test]
    fn an_operation_is_described_by_its_variant_before_a_trimap() {
        for variant in every_variant() {
            let (one, two) = (Matting::new(variant), Matting::new(variant));

            assert_eq!(one, two);
            assert_eq!(hash_of(&one), hash_of(&two));
        }
    }

    #[test]
    fn a_display_name_is_the_variant_and_the_precision() {
        assert_eq!(Matting::porto().display_name(), "Porto (FP32)");
    }

    #[test]
    fn an_operation_round_trips_through_its_serialized_form_without_its_trimap() {
        let json = serde_json::to_string(&Matting::porto().with_trimap(trimap(0.5))).expect("serializes");

        assert_eq!(json, r#"{"variant":{"codename":"porto","precision":"fp32"}}"#);
        assert_eq!(serde_json::from_str::<Matting>(&json).expect("deserializes"), Matting::porto());
    }
}
