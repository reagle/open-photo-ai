//! The upscale family: the three variants, what one operation is, and what it resolves to.
//!
//! One contract lives here, the **convolutional** one — Tokyo, Kyoto and Saitama — which holds one set of weights per
//! native scale, so the requested scale selects the weights and is part of the model's identity. The diffusion one,
//! Osaka, which restores detail at whatever size the image was resampled to, moved to the restore family: it redraws
//! detail rather than estimating it, and its size is a restore's rather than an upscale factor.

pub(crate) mod conv;
pub(crate) mod kyoto;
pub(crate) mod passes;
pub(crate) mod resolve;
pub(crate) mod saitama;
pub(crate) mod tokyo;
pub(crate) mod variant;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::operation::Operation;
use super::precision::{FloatPrecision, Precision};
use super::scale::Scale;

use crate::error::InferenceError;
use crate::pipeline::Backend;
use crate::pipeline::Shared;
use crate::providers::profile::EpProfile;
use passes::PassSequence;
use resolve::Resolution;

// The enumeration the catalogue reads, which is what keeps a cross-family file from importing three rows by name: the
// seven simple families expose the same as `denoise::ALL` and its six siblings.
//
// A model reachable through `UpscaleVariant::model` and missing from here would be absent from every chooser while
// resolving perfectly, and only the first of the two is checked by the compiler — so a test drives every published
// variant and asserts its model is in this list.
//
// Not derived from the enum: `UpscaleVariant` cannot be enumerated without a precision per arm, and the precisions
// differ by arm, which is the whole point of the precision living inside the variant.
/// Every model this family publishes, in the order a chooser should offer them.
///
/// **Not a dispatch seam** — nothing resolves through it.
pub(crate) static MODELS: [&'static dyn UpscaleModel; 3] = [&TOKYO, &KYOTO, &SAITAMA];

pub(crate) use kyoto::KYOTO;
pub(crate) use saitama::SAITAMA;
pub(crate) use tokyo::TOKYO;
pub(crate) use variant::UpscaleModel;
pub use variant::UpscaleVariant;

impl Upscale {
    /// The pipeline this operation runs — the family's **contract** seam, and one of the two matches a model is
    /// reachable through.
    ///
    /// # Errors
    ///
    /// Returns [`InferenceError::NoPasses`] where no sequence of native passes covers the requested scale — a refusal
    /// rather than a failure further down: this runs during planning, before anything is installed.
    pub(crate) fn pipeline<B: Backend>(self) -> Result<Shared<B>, InferenceError> {
        // One arm per contract rather than one per model, which is what keeps this from growing on two axes at once:
        // a fifth convolutional variant is a row and an arm in `UpscaleVariant`, and nothing here changes. It grows
        // only if a genuinely new contract arrives. No model is named in it.
        //
        // The contract is `Resolution`'s to report; this is the answer to the question it asks, and it is the last
        // place in the crate where the two are told apart. Above it, `Operation::pipeline` knows only families; above
        // that, the chain driver knows only `ImagePipeline`.
        let required = self.required_artifacts();
        let name = self.display_name();

        // What a run takes beyond naming the model, read through the seam that publishes it rather than off the
        // operation's own fields. Both arms below are built from this, which is what makes `params` load-bearing:
        // a pipeline cannot be constructed from something the library did not say a run needs.
        let params = self.params();

        let Resolution::Passes(passes) = self.resolve();

        // The shape the model's row declares, which every row resolving to passes declares.
        let shape = self
            .variant()
            .model()
            .pass_shape()
            .expect("a variant that resolved to passes declares the shape they run at");

        // Unreachable today — the bucket tables are checked to reach the maximum scale with no gap — and guarded
        // anyway, because what it prevents is not a crash. Zero passes means the pass loop runs zero times and the
        // correcting resample hands back a plain Lanczos resize of the input, presented as a successful AI upscale with
        // no error.
        if passes.is_empty() {
            return Err(InferenceError::NoPasses { operation: name, scale: params.scale.get() });
        }

        Ok(Arc::new(PassSequence::new(name, passes, params.scale, required, self.profile(), shape)))
    }
}

/// One upscale run: which model, and how much larger the result is.
///
/// This value **is** the operation's identity: it compares and hashes as a value, so a consumer holding two of them is
/// told whether they describe the same request. Its one string form is the run cache tag, which has to outlive the
/// process.
///
/// Identity is **not** what decides which weights are loaded. A resident session is filed under the artifact it was
/// opened from and the provider it was opened on — see this crate's session cache — so an 8x Kyoto and a 4x Kyoto share
/// `up_kyoto_4x_fp16` although they are two operations. Equality answers "the same request"; [`Upscale::cache_tag`]
/// answers "the same pixels".
///
/// Nothing here owns a native resource. It is a plain `Copy` value describing *what* to run, with no reference to a
/// loaded session — which is what lets a front end hold and persist one independently of anything built from it.
///
/// This type is the one to **build**; [`Operation::Upscale`] is the one to
/// **keep**. See [`Operation`] for which to reach for when.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Upscale {
    /// The model and its precision.
    variant: UpscaleVariant,
    /// The factor the image is enlarged by, and part of the operation's identity for every variant: two scales are
    /// two requests, whether or not they are served by one set of weights.
    scale: Scale,
}

impl Upscale {
    /// The operation running `variant` at `scale`.
    pub const fn new(variant: UpscaleVariant, scale: Scale) -> Self {
        Self { variant, scale }
    }

    /// Tokyo at `precision`, enlarging by `scale`.
    ///
    /// Returns the [`Operation`] carrying it rather than `Self`, so a caller writes
    /// `opai.process(&picture, &[tokyo, kyoto], None)` with no `Operation::Upscale(…)` around each element. The typed
    /// value stays reachable by matching the carrier.
    ///
    /// Published at FP32 and FP16, which is why the argument is a [`FloatPrecision`]
    /// rather than a [`Precision`]: `up_tokyo_4x_int8` is not a request this refuses, it is one that cannot be
    /// written.
    pub fn tokyo(precision: FloatPrecision, scale: Scale) -> Operation {
        Operation::Upscale(Self::new(UpscaleVariant::Tokyo(precision), scale))
    }

    /// Kyoto at `precision`, enlarging by `scale`. Published at FP32 and FP16; see [`Upscale::tokyo`].
    pub fn kyoto(precision: FloatPrecision, scale: Scale) -> Operation {
        Operation::Upscale(Self::new(UpscaleVariant::Kyoto(precision), scale))
    }

    /// Saitama at `precision`, enlarging by `scale`. Published at FP32 and FP16; see [`Upscale::tokyo`].
    pub fn saitama(precision: FloatPrecision, scale: Scale) -> Operation {
        Operation::Upscale(Self::new(UpscaleVariant::Saitama(precision), scale))
    }

    /// Which model this runs, and at which precision.
    pub const fn variant(self) -> UpscaleVariant {
        self.variant
    }

    /// The factor the image is enlarged by, at the value it was quantized to.
    pub const fn scale(self) -> Scale {
        self.scale
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning measured for the model this runs, at the precision it runs at.
    ///
    /// The scale selects which native passes run, not what they run with, so every pass of a run carries this.
    pub(crate) fn profile(self) -> EpProfile {
        self.variant.profile()
    }

    /// The name a user interface shows: `Kyoto 4x (FP16)`. It carries the scale, because the scale is what selected
    /// the weights.
    pub fn display_name(self) -> String {
        self.to_string()
    }
}

/// What one upscale run takes, in the shape its pipelines want it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpscaleParams {
    // A typed value rather than a map of strings to anything. The reference implementation passes these as
    // `map[string]any` under stringly-typed keys and substitutes a default on a missing key or a wrong type, so a typo
    // there is a wrong image rather than an error; here a parameter that is not carried cannot be read.
    //
    // Not an `Option`: every upscale run takes a scale — the convolutional pipeline needs it to pick its passes and
    // correct the result.
    /// The factor this run enlarges by.
    pub scale: Scale,
}

impl Upscale {
    /// What one run of this operation takes beyond naming the model.
    pub const fn params(self) -> UpscaleParams {
        UpscaleParams { scale: self.scale }
    }

    /// The key the run cache stores this operation's result under.
    ///
    /// Two operations that produce the same image from the same input report the same tag and two that do not report
    /// different ones.
    ///
    /// Hyphen-separated, which keeps it out of the underscore-separated namespace artifact names live in: no tag can
    /// ever be mistaken for, or collide with, a published file name.
    pub fn cache_tag(self) -> String {
        // A string because the run cache persists to disk and outlives the process, so value equality alone cannot
        // key it.
        //
        // The scale is spelled here although it is in the identity: the tag is written out by hand rather than derived
        // from it.
        //
        // Written deliberately rather than derived from the serialized form: deriving it would tie the on-disk cache to
        // a field-naming decision, so a rename would silently invalidate every cached image. It is pinned by the tests
        // below instead, and nothing else in the system keys on it.
        format!(
            "{}-{}-{}-s{}",
            super::artifact::Family::Upscale.prefix(),
            self.variant.codename(),
            self.variant.precision(),
            self.scale
        )
    }
}

impl std::fmt::Display for Upscale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = self.variant.label();
        let precision = self.variant.precision().label();

        // The scale, because the scale is what selected its weights.
        write!(f, "{label} {}x ({precision})", self.scale)
    }
}

#[cfg(test)]
mod tests {
    use super::variant::tests::every_variant;
    use super::*;
    use crate::models::precision::FloatPrecision;
    use crate::models::test_support::hash_of;
    use crate::pipeline::test_support::NoBackend;

    fn scale(value: f64) -> Scale {
        Scale::new(value).expect("the test supplied a scale in range")
    }

    /// The artifacts the pipeline `operation` reaches would open sessions for, in order.
    fn session_artifacts(operation: Upscale) -> Vec<String> {
        // Which arm of the contract seam was taken, seen through the only thing the chain driver ever asks. A pass
        // sequence names weights per native scale and a graph set names the three graphs, so the two are not
        // confusable — and reading it off `sessions()` rather than off the concrete type is the same view the
        // acquisition loop has.
        let pipeline = operation.pipeline::<NoBackend>().expect("every published variant reaches a pipeline");

        pipeline.sessions().iter().map(|(artifact, _)| artifact.as_str().to_string()).collect()
    }

    #[test]
    fn every_model_the_family_resolves_to_is_one_it_offers() {
        // By the row's own address rather than by its codename, so two rows that agreed on a name could not stand in
        // for one another. Compared as thin pointers because `ptr::eq` on a `dyn` also compares vtables, which are
        // not guaranteed unique across codegen units.
        let address = |model: &'static dyn UpscaleModel| std::ptr::from_ref(model).cast::<()>();

        for variant in every_variant() {
            assert!(
                MODELS.iter().any(|offered| address(*offered) == address(variant.model())),
                "{variant:?} resolves to a model the family does not offer"
            );
        }

        assert_eq!(MODELS.len(), 3, "the family offers a number of models the chooser order was not written for");
    }

    #[test]
    fn each_variant_reaches_the_pipeline_its_own_contract_names() {
        // Nothing here names a pipeline type — what is checked is that the operation arrives at the one its
        // `Resolution` reports.
        assert_eq!(
            session_artifacts(Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp16), scale(8.0))),
            vec!["up_kyoto_4x_fp16", "up_kyoto_2x_fp16"],
            "Kyoto did not reach the pass sequence with the passes covering the request"
        );
        assert_eq!(
            session_artifacts(Upscale::new(UpscaleVariant::Tokyo(FloatPrecision::Fp32), scale(4.0))),
            vec!["up_tokyo_4x_fp32"]
        );
        assert_eq!(
            session_artifacts(Upscale::new(UpscaleVariant::Saitama(FloatPrecision::Fp16), scale(4.0))),
            vec!["up_saitama_4x_fp16"]
        );
    }

    #[test]
    fn every_variant_at_every_scale_reaches_a_pipeline_rather_than_a_refusal() {
        // Nothing a user can construct reaches a pipeline with an empty pass sequence, which downstream is a plain
        // resample presented as a successful enhancement.
        for variant in every_variant() {
            for value in [1.0, 1.5, 2.0, 2.5, 4.0, 6.0, 8.0] {
                let operation = Upscale::new(variant, scale(value));
                let pipeline = operation
                    .pipeline::<NoBackend>()
                    .unwrap_or_else(|error| panic!("{operation} reached no pipeline: {error}"));

                assert!(pipeline.stages() > 0, "{operation} reached a pipeline that runs no model at all");
                assert!(!pipeline.required().is_empty(), "{operation} reached a pipeline needing nothing on disk");
            }
        }
    }

    #[test]
    fn a_pass_sequence_carries_one_profile() {
        // Checked at the seam that produces it: the passes of one operation share its single measured declaration.
        // Osaka's graphs each carrying their own is pinned in the restore family, where Osaka now is.
        let kyoto = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp16), scale(8.0));
        let pipeline = kyoto.pipeline::<NoBackend>().expect("Kyoto reaches a pipeline");
        let sessions = pipeline.sessions();

        assert_eq!(sessions.len(), 2, "an 8x Kyoto run is two native passes");
        for (artifact, profile) in &sessions {
            assert_eq!(*profile, &kyoto.profile(), "{artifact} was paired with something other than the operation's");
        }
    }

    #[test]
    fn two_identical_requests_are_the_same_operation() {
        // What a registry keyed on the value needs: requesting the same model twice must find what is loaded rather
        // than load a second copy of it.
        let one = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(2.5));
        let two = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(2.5));

        assert_eq!(one, two);
        assert_eq!(hash_of(&one), hash_of(&two));
    }

    #[test]
    fn two_scales_of_a_convolutional_variant_are_different_operations() {
        let two_times = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(2.0));
        let four_times = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(4.0));

        assert_ne!(two_times, four_times, "two scales served by different weights compared as one model");
    }

    #[test]
    fn two_precisions_are_different_operations() {
        let fp32 = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(4.0));
        let fp16 = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp16), scale(4.0));

        assert_ne!(fp32, fp16, "two precisions served by different artifacts compared as one model");
    }

    #[test]
    fn two_variants_are_never_the_same_operation() {
        let kyoto = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp16), scale(4.0));
        let tokyo = Upscale::new(UpscaleVariant::Tokyo(FloatPrecision::Fp16), scale(4.0));

        assert_ne!(kyoto, tokyo);
    }

    #[test]
    fn an_operation_is_usable_as_a_lookup_key_directly() {
        // The point of deriving the identity on the value: no string stands between an operation and what it looks
        // up. Keyed on the whole request, so a differing scale is a differing key — which is what a consumer asking
        // "have I seen this request before?" wants, and it is not the question a session lookup asks.
        let mut seen = std::collections::HashMap::new();
        let two_times = Upscale::new(UpscaleVariant::Tokyo(FloatPrecision::Fp16), scale(2.0));
        let six_times = Upscale::new(UpscaleVariant::Tokyo(FloatPrecision::Fp16), scale(6.0));

        seen.insert(two_times, "requested");

        assert_eq!(seen.get(&two_times), Some(&"requested"), "one request did not find itself");
        assert_eq!(seen.get(&six_times), None, "a different scale found another request's entry");
    }

    #[test]
    fn a_convolutional_display_name_carries_the_scale() {
        let operation = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp16), scale(4.0));

        assert_eq!(operation.display_name(), "Kyoto 4x (FP16)");
        assert_eq!(operation.to_string(), operation.display_name());
    }

    #[test]
    fn a_fractional_display_name_renders_the_scale_as_it_was_accepted() {
        let operation = Upscale::new(UpscaleVariant::Saitama(FloatPrecision::Fp32), scale(2.5));

        assert_eq!(operation.display_name(), "Saitama 2.5x (FP32)");
    }

    #[test]
    fn every_variant_reports_the_scale_its_run_takes() {
        // For the reasons `UpscaleParams::scale` gives.
        for variant in every_variant() {
            assert_eq!(Upscale::new(variant, scale(2.5)).params(), UpscaleParams { scale: scale(2.5) });
        }
    }

    #[test]
    fn a_repeated_operation_reports_a_stable_tag() {
        let operation = Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(2.0));

        assert_eq!(operation.cache_tag(), operation.cache_tag());
        assert_eq!(
            operation.cache_tag(),
            Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(2.0)).cache_tag()
        );
    }

    #[test]
    fn the_cache_tag_is_the_written_one_rather_than_whatever_serialization_produces() {
        // Pinned here so that renaming a field is a failing test rather than a silent invalidation of every cached
        // image on every user's disk.
        assert_eq!(
            Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(2.5)).cache_tag(),
            "up-kyoto-fp32-s2.5"
        );
    }

    #[test]
    fn no_two_operations_producing_different_images_share_one_tag() {
        // Everything that changes the output — the variant, the precision and the scale, whether or not the scale is
        // part of the identity — has to reach the tag.
        let mut seen = std::collections::HashMap::new();

        for variant in every_variant() {
            for value in [1.0, 1.5, 2.0, 2.5, 4.0, 6.0, 8.0] {
                let operation = Upscale::new(variant, scale(value));

                if let Some(previous) = seen.insert(operation.cache_tag(), operation) {
                    panic!("{previous} and {operation} share the cache tag {}", operation.cache_tag());
                }
            }
        }
    }

    #[test]
    fn a_cache_tag_can_never_be_mistaken_for_an_artifact_name() {
        for variant in every_variant() {
            let tag = Upscale::new(variant, scale(2.0)).cache_tag();

            assert!(!tag.contains('_'), "{tag} is spelled the way an artifact name is");
        }
    }

    #[test]
    fn an_operation_round_trips_through_its_serialized_form() {
        for variant in every_variant() {
            let operation = Upscale::new(variant, scale(2.5));
            let json = serde_json::to_string(&operation).expect("an operation serializes");
            let back: Upscale = serde_json::from_str(&json).expect("an operation deserializes");

            assert_eq!(back, operation, "{json} did not round-trip to an equal operation");
            assert_eq!(back.variant(), operation.variant(), "{json} lost its variant or precision");
            assert_eq!(back.scale(), operation.scale(), "{json} lost its scale");
        }
    }

    #[test]
    fn the_serialized_form_names_its_variant_rather_than_positioning_it() {
        // Tagged by name so that adding a family or a variant later cannot invalidate anything already persisted.
        let json = serde_json::to_string(&Upscale::new(UpscaleVariant::Kyoto(FloatPrecision::Fp32), scale(2.0)))
            .expect("an operation serializes");

        assert_eq!(json, r#"{"variant":{"codename":"kyoto","precision":"fp32"},"scale":2.0}"#);
    }

    #[test]
    fn a_serialized_operation_carrying_an_out_of_range_scale_is_refused() {
        let json = r#"{"variant":{"codename":"kyoto","precision":"fp32"},"scale":12.0}"#;

        assert!(
            serde_json::from_str::<Upscale>(json).is_err(),
            "deserialization produced an operation whose scale could not have been constructed"
        );
    }
}
