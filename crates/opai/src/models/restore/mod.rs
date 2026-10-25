//! The restore family: generative restorers, which redraw the detail a photograph lost rather than estimate it, at the
//! photograph's own size or larger.
//!
//! **Why a family of its own.** Every other enhancement is faithful: it estimates the photograph that was there, and a
//! face it sharpens is the face that was photographed. A generative model draws a plausible one, and on a face,
//! lettering or a texture it does not recognise what it draws can differ from what was there. Offered beside the
//! faithful models in their families, that difference is one entry in a chooser; here it is the family, so a front end
//! can say so once, run these last, and keep them out of anything automatic.
//!
//! **What a run takes.** A model, a size and a strength. The size is how much larger the result is — 1x restores the
//! photograph as it is — and an enlarging run resamples first and restores at the new size, which is how these models
//! are trained and driven; see [`resample`]. The strength blends the restoration with the image it was conditioned on,
//! so it is the fidelity control.
//!
//! **Where it runs.** Last, after every faithful family; see [`Family::APPLY_ORDER`].

pub(crate) mod kazan;
pub(crate) mod resample;
pub(crate) mod variant;

use serde::{Deserialize, Serialize};

use super::artifact::{ArtifactId, Family};
use super::operation::Operation;
use super::precision::{FloatPrecision, Precision};
use super::scale::{RangeError, Scale};
use super::simple::single_artifact;
use super::strength::Strength;

use crate::error::InferenceError;
use crate::models::filter;
use crate::pipeline::Backend;
use crate::pipeline::Shared;
use crate::providers::profile::EpProfile;

pub(crate) use variant::ALL;
pub use variant::RestoreVariant;

/// One restore run: which model, how much larger the result is, and how much of the model's output to apply.
///
/// This value **is** the operation's identity, as every family's is: two sizes or two strengths are two requests,
/// although one set of weights serves them all. Nothing here owns a native resource.
///
/// **Narrower than the types it carries.** A [`Scale`] reaches 8x and a [`Strength`] 3, and neither is offered here:
/// a restorer resampling eightfold is drawing almost the whole photograph, and a strength past 1 extrapolates invented
/// detail away from the photograph. [`Restore::new`] brings both inside [`Restore::MAX_SCALE`] and
/// [`Restore::MAX_STRENGTH`], and a serialized operation outside them is refused, so a value that exists is one the
/// catalogue offers.
///
/// Build this; keep it as an [`Operation::Restore`] — see [`Operation`] for which of the two to reach for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Fields")]
pub struct Restore {
    /// The model and its precision.
    variant: RestoreVariant,
    /// How much larger the result is than the photograph, from 1 to [`Restore::MAX_SCALE`].
    scale: Scale,
    /// How much of the model's output this run applies, from 0 to [`Restore::MAX_STRENGTH`].
    strength: Strength,
}

/// [`Restore`]'s fields as they arrive, before the family's own bounds are checked.
#[derive(Deserialize)]
struct Fields {
    variant: RestoreVariant,
    scale: Scale,
    strength: Strength,
}

impl TryFrom<Fields> for Restore {
    type Error = RangeError;

    /// Refused rather than clamped, as every bounded type's own deserialization is: a persisted value outside the
    /// range is a fault in whatever wrote it, and silently changing the request it names would hide that.
    fn try_from(fields: Fields) -> Result<Self, RangeError> {
        let refused = |parameter, value: f64, min, max| RangeError { parameter, value, min, max };

        if fields.scale.get() > Self::MAX_SCALE {
            return Err(refused(Scale::NAME, fields.scale.get(), Scale::MIN, Self::MAX_SCALE));
        }
        if fields.strength.get() > Self::MAX_STRENGTH {
            return Err(refused(Strength::NAME, fields.strength.get(), Strength::MIN, Self::MAX_STRENGTH));
        }

        Ok(Self::new(fields.variant, fields.scale, fields.strength))
    }
}

impl Restore {
    /// The largest size a restore run enlarges to: 4x, the largest a front end offers.
    ///
    /// Past it the model would be drawing nearly every pixel of the result from a resample, and the cost grows with
    /// the square of the size — a 4x run over a 12-megapixel photograph is already 192 megapixels of restoration.
    pub const MAX_SCALE: f64 = 4.0;

    /// The largest strength a restore run applies: the model's own output. Below it the restoration is blended back
    /// towards the image it was conditioned on.
    pub const MAX_STRENGTH: f64 = 1.0;

    /// The operation running `variant` at `scale` and `strength`, each brought inside this family's bounds.
    pub fn new(variant: RestoreVariant, scale: Scale, strength: Strength) -> Self {
        Self {
            variant,
            scale: scale.min(Scale::clamped(Self::MAX_SCALE)),
            strength: strength.min(Strength::clamped(Self::MAX_STRENGTH)),
        }
    }

    /// Kazan at `precision`, restoring at `scale` and applying `strength` of its output.
    ///
    /// Returns the carrier rather than `Self`, so a caller collects operations of several families into one chain with
    /// no per-element wrapping; the typed value stays reachable by matching it. Published at FP32 and FP16.
    pub fn kazan(precision: FloatPrecision, scale: Scale, strength: Strength) -> Operation {
        Operation::Restore(Self::new(RestoreVariant::Kazan(precision), scale, strength))
    }

    /// Which model this runs, and at which precision.
    pub const fn variant(self) -> RestoreVariant {
        self.variant
    }

    /// How much larger the result is, at the value it was quantized to.
    pub const fn scale(self) -> Scale {
        self.scale
    }

    /// How much of the model's output this run applies, at the value it was quantized to.
    pub const fn strength(self) -> Strength {
        self.strength
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning declared for the model this runs, at the precision it runs at.
    pub(crate) fn profile(self) -> EpProfile {
        self.variant.profile()
    }

    /// The name a user interface shows: `Kazan (FP16)`, without the size or the strength.
    pub fn display_name(self) -> String {
        // Neither selects the weights, so a name fixed at whichever value happened to load them first would be wrong
        // from the second run onwards — Osaka's reason for leaving its scale out of its name.
        self.to_string()
    }

    /// What this run needs beyond the operation itself.
    pub const fn params(self) -> RestoreParams {
        RestoreParams { scale: self.scale, strength: self.strength }
    }

    /// The pipeline this operation runs — the **family's contract seam**, where a variant becomes a run.
    ///
    /// # Errors
    ///
    /// None today: Kazan's pipeline cannot be refused. A `Result` because the family's contracts are not all one —
    /// a multi-graph restorer is refused where its graph set is incomplete, as upscale's diffusion arm is.
    pub(crate) fn pipeline<B: Backend>(self) -> Result<Shared<B>, InferenceError> {
        // Built from what `params` publishes rather than from the fields, so a pipeline cannot be constructed from
        // something the library did not say a run needs.
        let params = self.params();

        match self.variant {
            // The shared filter at Kazan's own tile, with no guard: a generative tile has no "diverged" a threshold
            // could tell from a redrawn one. Wrapped in the resample, so the filter's strength blends towards the
            // image the model was handed.
            RestoreVariant::Kazan(_) => Ok(resample::resampled(
                params.scale,
                filter::pipeline::<B>(
                    self.display_name(),
                    self.artifact(),
                    self.profile(),
                    kazan::RANGE,
                    params.strength,
                    None,
                    self.variant.geometry(),
                ),
            )),
        }
    }

    /// The artifact serving this operation's model: `rs_kazan_fp16`.
    pub fn artifact(self) -> ArtifactId {
        single_artifact(Family::Restore, self.variant.codename(), self.precision())
    }

    /// The artifacts that must be on disk before this operation can run.
    pub fn required_artifacts(self) -> Vec<ArtifactId> {
        // A `Vec` to match `Upscale::required_artifacts`, so that whoever downloads what an operation needs asks every
        // family the same question.
        vec![self.artifact()]
    }

    /// The key the run cache stores this operation's result under: `rs-kazan-fp16-s2-t1`.
    ///
    /// Carries the size and the strength, which the tag has to spell itself because it is written out by hand rather
    /// than derived from the identity: both are one model and two images. Hyphen-separated, so it cannot be mistaken
    /// for an underscore-separated artifact name.
    pub fn cache_tag(self) -> String {
        // Written deliberately rather than derived from the serialized form, so that renaming a field cannot silently
        // invalidate every cached image. The letters are upscale's `s` and sharpen's `t`, in this family's namespace.
        format!(
            "{}-{}-{}-s{}-t{}",
            Family::Restore.prefix(),
            self.variant.codename(),
            self.variant.precision(),
            self.scale,
            self.strength
        )
    }
}

/// What one restore run takes, in the shape its pipeline wants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreParams {
    // A typed value rather than a map of strings to anything, for the reason `UpscaleParams` gives.
    /// How much larger the result is.
    pub scale: Scale,
    /// How much of the model's output this run applies.
    pub strength: Strength,
}

impl std::fmt::Display for Restore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.variant.label(), self.variant.precision().label())
    }
}

#[cfg(test)]
mod tests {
    use super::variant::tests::every_variant;
    use super::*;
    use crate::models::test_support::hash_of;
    use crate::pipeline::test_support::NoBackend;
    use imaging::tensor::Normalisation;

    fn scale(value: f64) -> Scale {
        Scale::new(value).expect("the test supplied a scale in range")
    }

    fn strength(value: f64) -> Strength {
        Strength::new(value).expect("the test supplied a strength in range")
    }

    fn kazan(precision: FloatPrecision, size: f64, amount: f64) -> Restore {
        Restore::new(RestoreVariant::Kazan(precision), scale(size), strength(amount))
    }

    #[test]
    fn kazans_graph_is_fed_the_unit_range() {
        // The literal, pinned rather than left to be read off a call, for the reason `kazan::RANGE` gives.
        assert_eq!(kazan::RANGE, Normalisation::Unit, "Kazan's graph was switched to the signed range");
    }

    #[test]
    fn every_variant_names_the_artifact_it_is_installed_as() {
        let expected = [
            (RestoreVariant::Kazan(FloatPrecision::Fp32), "rs_kazan_fp32"),
            (RestoreVariant::Kazan(FloatPrecision::Fp16), "rs_kazan_fp16"),
        ];

        for (variant, name) in expected {
            let operation = Restore::new(variant, scale(2.0), strength(1.0));

            assert_eq!(operation.artifact().as_str(), name);
            assert_eq!(operation.required_artifacts(), vec![operation.artifact()]);
        }
    }

    #[test]
    fn the_cache_tag_is_the_written_one_rather_than_whatever_serialization_produces() {
        assert_eq!(kazan(FloatPrecision::Fp16, 1.0, 1.0).cache_tag(), "rs-kazan-fp16-s1-t1");
        assert_eq!(kazan(FloatPrecision::Fp32, 2.0, 0.5).cache_tag(), "rs-kazan-fp32-s2-t0.5");
    }

    #[test]
    fn no_two_runs_producing_different_images_share_one_tag() {
        // The size and the strength both change the image, and neither selects the weights — so both reach the tag.
        let mut seen = std::collections::HashMap::new();

        for variant in every_variant() {
            for size in [1.0, 1.5, 2.0, 4.0] {
                for amount in [0.0, 0.25, 1.0] {
                    let operation = Restore::new(variant, scale(size), strength(amount));

                    if let Some(previous) = seen.insert(operation.cache_tag(), operation) {
                        panic!("{previous:?} and {operation:?} share the cache tag {}", operation.cache_tag());
                    }
                }
            }
        }
    }

    #[test]
    fn two_sizes_are_two_operations_sharing_one_set_of_weights() {
        let one = kazan(FloatPrecision::Fp16, 1.0, 1.0);
        let two = kazan(FloatPrecision::Fp16, 2.0, 1.0);

        assert_ne!(one, two, "two sizes read as one request");
        assert_ne!(hash_of(&one), hash_of(&two));
        assert_eq!(one.required_artifacts(), two.required_artifacts(), "a size change would reload the weights");
    }

    #[test]
    fn a_display_name_is_the_label_and_the_precision_alone() {
        for (size, amount) in [(1.0, 1.0), (4.0, 0.25)] {
            let operation = kazan(FloatPrecision::Fp16, size, amount);

            assert_eq!(operation.display_name(), "Kazan (FP16)");
            assert_eq!(operation.to_string(), operation.display_name());
        }
    }

    #[test]
    fn a_size_or_strength_past_the_familys_bounds_is_brought_inside_them() {
        let operation = kazan(FloatPrecision::Fp32, 8.0, 3.0);

        assert_eq!(operation.scale().get(), Restore::MAX_SCALE);
        assert_eq!(operation.strength().get(), Restore::MAX_STRENGTH);
        assert_eq!(operation.params(), RestoreParams { scale: scale(4.0), strength: strength(1.0) });
    }

    #[test]
    fn an_operation_round_trips_through_its_serialized_form() {
        for variant in every_variant() {
            let operation = Restore::new(variant, scale(2.0), strength(0.75));
            let json = serde_json::to_string(&operation).expect("an operation serializes");
            let back: Restore = serde_json::from_str(&json).expect("an operation deserializes");

            assert_eq!(back, operation, "{json} did not round-trip");
            assert_eq!(back.cache_tag(), operation.cache_tag());
        }
    }

    #[test]
    fn the_serialized_form_names_its_variant_rather_than_positioning_it() {
        let json = serde_json::to_string(&kazan(FloatPrecision::Fp16, 2.0, 0.5)).expect("an operation serializes");

        assert_eq!(json, r#"{"variant":{"codename":"kazan","precision":"fp16"},"scale":2.0,"strength":0.5}"#);
    }

    #[test]
    fn a_serialized_operation_outside_the_familys_bounds_is_refused() {
        for json in [
            r#"{"variant":{"codename":"kazan","precision":"int8"},"scale":2.0,"strength":0.5}"#,
            r#"{"variant":{"codename":"kazan","precision":"fp16"},"scale":6.0,"strength":0.5}"#,
            r#"{"variant":{"codename":"kazan","precision":"fp16"},"scale":2.0,"strength":2.0}"#,
            r#"{"variant":{"codename":"moscow","precision":"fp16"},"scale":2.0,"strength":0.5}"#,
        ] {
            assert!(serde_json::from_str::<Restore>(json).is_err(), "deserialization accepted {json}");
        }
    }

    #[test]
    fn every_variant_at_every_size_reaches_a_pipeline_needing_its_artifact() {
        for variant in every_variant() {
            for size in [1.0, 2.0, 4.0] {
                let operation = Restore::new(variant, scale(size), strength(1.0));
                let pipeline = operation.pipeline::<NoBackend>().expect("every published variant reaches a pipeline");

                assert_eq!(pipeline.required(), operation.required_artifacts().as_slice());
                assert_eq!(pipeline.sessions().len(), 1, "{operation:?}");
                assert_eq!(*pipeline.sessions()[0].1, operation.profile(), "{operation:?} lost its profile");
            }
        }
    }
}
