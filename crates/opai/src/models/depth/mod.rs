//! The depth family: one variant, two graphs, no per-run parameter, and a result that is not an image but a map of
//! distances.
//!
//! Not to be confused with `inference::depth`, whose [`OutputDepth`](crate::OutputDepth) is bits per channel of a
//! picture. The depth here is the distance from the camera, in metres.

pub(crate) mod lisbon;
pub(crate) mod map;
pub(crate) mod recover;
pub(crate) mod variant;

#[cfg(test)]
mod live;

use serde::{Deserialize, Serialize};

use super::artifact::{ArtifactId, Family};
use super::precision::{FloatPrecision, Precision};

use crate::providers::profile::EpProfile;

pub use map::DepthMap;
pub(crate) use variant::ALL;
pub use variant::DepthVariant;

/// One depth-estimation run: which model, and nothing else. Two operations are equal exactly when their variants are.
///
/// What a run produces is a [`DepthMap`], the distance of every pixel from the camera, rather than an image — which
/// is what puts this family beside [`Detection`](super::Detection) rather than in a chain. Its constructor hands back
/// this type itself rather than an [`Analysis`](super::operation::Analysis): what
/// [`Opai::estimate_depth`](crate::Opai::estimate_depth) takes, and what it returns, are this family's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Depth {
    // No per-run input, expressed as the absence of a field, as `Detection`'s is.
    /// The model and its precision — the whole of this operation's identity.
    variant: DepthVariant,
}

impl Depth {
    /// The operation running `variant`.
    pub const fn new(variant: DepthVariant) -> Self {
        Self { variant }
    }

    /// Lisbon, this family's model, at the one precision it is published in.
    pub const fn lisbon() -> Self {
        Self::new(DepthVariant::Lisbon(FloatPrecision::Fp16))
    }

    /// Which model this runs, and at which precision.
    pub const fn variant(self) -> DepthVariant {
        self.variant
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning measured for the model this runs, at the precision it runs at.
    pub(crate) fn profile(self) -> EpProfile {
        self.variant.profile()
    }

    /// The name a user interface shows: `Lisbon (FP16)`.
    pub fn display_name(self) -> String {
        self.to_string()
    }

    /// The artifact serving a photograph of `orientation`: `dp_lisbon_landscape_fp16`.
    pub(crate) fn graph(self, orientation: lisbon::Orientation) -> ArtifactId {
        ArtifactId::suffixed(Family::Depth, self.variant.codename(), orientation.suffix(), self.precision())
    }

    /// The artifacts that must be on disk before this operation can run: both orientations' graphs.
    ///
    /// Both, although a run opens one: which one is decided by the photograph, and whoever installs what an operation
    /// needs installs it before any photograph is known.
    pub fn required_artifacts(self) -> Vec<ArtifactId> {
        lisbon::Orientation::ALL.into_iter().map(|orientation| self.graph(orientation)).collect()
    }

    /// The key the run cache stores this operation's result under: `dp-lisbon-fp16`.
    ///
    /// The variant and the precision alone, as detection's. Not the orientation: the photograph decides it, and the
    /// photograph's identity is already the other half of the key.
    pub fn cache_tag(self) -> String {
        format!("{}-{}-{}", Family::Depth.prefix(), self.variant.codename(), self.variant.precision())
    }
}

impl std::fmt::Display for Depth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.variant.label(), self.variant.precision().label())
    }
}

#[cfg(test)]
mod tests {
    use super::variant::tests::every_variant;
    use super::*;
    use crate::models::test_support::hash_of;

    #[test]
    fn lisbon_names_one_graph_per_orientation() {
        let depth = Depth::lisbon();

        assert_eq!(depth.graph(lisbon::Orientation::Landscape).as_str(), "dp_lisbon_landscape_fp16");
        assert_eq!(depth.graph(lisbon::Orientation::Portrait).as_str(), "dp_lisbon_portrait_fp16");
        assert_eq!(
            depth.required_artifacts().iter().map(ArtifactId::as_str).collect::<Vec<_>>(),
            vec!["dp_lisbon_landscape_fp16", "dp_lisbon_portrait_fp16"]
        );
    }

    #[test]
    fn the_cache_tag_is_the_written_one_and_never_an_artifact_name() {
        let tag = Depth::lisbon().cache_tag();

        assert_eq!(tag, "dp-lisbon-fp16");
        assert!(!tag.contains('_'), "{tag} is spelled the way an artifact name is");
    }

    #[test]
    fn an_operation_is_fully_described_by_its_variant() {
        for variant in every_variant() {
            let (one, two) = (Depth::new(variant), Depth::new(variant));

            assert_eq!(one, two);
            assert_eq!(hash_of(&one), hash_of(&two));
            assert_eq!(one.cache_tag(), two.cache_tag());
        }
    }

    #[test]
    fn a_display_name_is_the_variant_and_the_precision() {
        assert_eq!(Depth::lisbon().display_name(), "Lisbon (FP16)");
    }

    #[test]
    fn an_operation_round_trips_through_its_serialized_form() {
        let json = serde_json::to_string(&Depth::lisbon()).expect("an operation serializes");

        assert_eq!(json, r#"{"variant":{"codename":"lisbon","precision":"fp16"}}"#);
        assert_eq!(serde_json::from_str::<Depth>(&json).expect("an operation deserializes"), Depth::lisbon());
    }
}
