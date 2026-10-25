//! What an operation resolves to: the native passes a convolutional variant runs.

use super::super::artifact::ArtifactId;
use super::Upscale;

/// One native scaling pass, and the artifact that serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pass {
    /// The native scale of the weights this pass runs, which is a whole number because the weights are published per
    /// whole factor. Emphatically not the scale that was requested.
    pub scale: u8,
    /// The artifact serving this pass.
    pub artifact: ArtifactId,
}

// An enum rather than the list itself, so that a second contract arriving in this family is an arm every caller has to
// answer rather than a shape it could default to this one. It had one: Osaka's graph set, which moved to the restore
// family with Osaka, where the scale it is resampled to is a size rather than an upscale factor.
/// What one upscale operation resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A convolutional variant: an ordered sequence of native passes covering the requested scale. Where the sequence
    /// overshoots the request — a 1.5x request served by a 2x pass — the overshoot is corrected after inference and
    /// is not a property of this resolution.
    Passes(Vec<Pass>),
}

impl Upscale {
    /// The passes this operation runs. Never empty.
    pub fn resolve(self) -> Resolution {
        self.variant().model().resolve(self.precision(), self.scale())
    }

    /// The distinct artifacts that must be on disk before this operation can run, in the order they are first needed.
    ///
    /// Deduplicated, because a sequence that runs one artifact twice still requires it once — Tokyo at 8x is two
    /// passes of one set of weights, not two downloads.
    pub fn required_artifacts(self) -> Vec<ArtifactId> {
        let mut required: Vec<ArtifactId> = Vec::new();

        // Pushed straight into the answer rather than collected and then filtered: the duplicates are the point of
        // the scan, and a second vector holding them on the way through buys nothing. `contains` rather than a sort
        // or a set because the order is the order they are first needed, and the list is three entries at most.
        let Resolution::Passes(passes) = self.resolve();

        for pass in passes {
            if !required.contains(&pass.artifact) {
                required.push(pass.artifact);
            }
        }

        required
    }
}

#[cfg(test)]
mod tests {
    use super::super::UpscaleVariant;
    use super::super::variant::tests::every_variant;
    use super::*;
    use crate::models::precision::FloatPrecision;
    use crate::models::scale::Scale;

    fn operation(variant: UpscaleVariant, value: f64) -> Upscale {
        Upscale::new(variant, Scale::new(value).expect("the test supplied a scale in range"))
    }

    fn passes(operation: Upscale) -> Vec<Pass> {
        let Resolution::Passes(passes) = operation.resolve();
        passes
    }

    fn names(artifacts: &[ArtifactId]) -> Vec<&str> {
        artifacts.iter().map(ArtifactId::as_str).collect()
    }

    #[test]
    fn kyoto_covers_a_small_scale_with_its_two_times_weights() {
        let resolved = passes(operation(UpscaleVariant::Kyoto(FloatPrecision::Fp32), 1.5));

        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].scale, 2);
        assert_eq!(resolved[0].artifact.as_str(), "up_kyoto_2x_fp32");
    }

    #[test]
    fn kyoto_covers_a_large_scale_with_two_passes_of_different_weights() {
        let operation = operation(UpscaleVariant::Kyoto(FloatPrecision::Fp32), 8.0);
        let resolved = passes(operation);

        assert_eq!(resolved.iter().map(|pass| pass.scale).collect::<Vec<_>>(), vec![4, 2]);
        assert_eq!(
            resolved.iter().map(|pass| pass.artifact.as_str()).collect::<Vec<_>>(),
            vec!["up_kyoto_4x_fp32", "up_kyoto_2x_fp32"]
        );
        assert_eq!(names(&operation.required_artifacts()), vec!["up_kyoto_4x_fp32", "up_kyoto_2x_fp32"]);
    }

    #[test]
    fn tokyo_covers_a_large_scale_by_repeating_one_set_of_weights() {
        let operation = operation(UpscaleVariant::Tokyo(FloatPrecision::Fp16), 8.0);
        let resolved = passes(operation);

        assert_eq!(resolved.iter().map(|pass| pass.scale).collect::<Vec<_>>(), vec![4, 4]);
        assert!(resolved.iter().all(|pass| pass.artifact.as_str() == "up_tokyo_4x_fp16"));
        assert_eq!(
            names(&operation.required_artifacts()),
            vec!["up_tokyo_4x_fp16"],
            "a sequence running one artifact twice required it twice"
        );
    }

    #[test]
    fn a_requested_scale_never_appears_in_an_artifact_name() {
        // The two differ whenever a request falls between native scales, and naming an artifact from the request
        // would name a file that was never published.
        let operation = operation(UpscaleVariant::Kyoto(FloatPrecision::Fp32), 2.5);

        assert_eq!(names(&operation.required_artifacts()), vec!["up_kyoto_4x_fp32"]);
    }

    #[test]
    fn every_scale_in_range_resolves_to_at_least_one_pass() {
        // Zero passes downstream is a plain resize presented as a successful AI upscale — no error and no log line.
        for variant in every_variant() {
            let mut value = Scale::MIN;
            while value <= Scale::MAX {
                let operation = operation(variant, value);
                assert!(!passes(operation).is_empty(), "{operation} resolved to no passes");
                assert!(!operation.required_artifacts().is_empty(), "{operation} required no artifacts");
                value += 0.001;
            }
        }
    }

    #[test]
    fn every_pass_is_served_by_an_artifact_named_from_its_own_native_scale() {
        for variant in every_variant() {
            for value in [1.0, 1.5, 2.0, 2.5, 4.0, 6.0, 8.0] {
                let operation = operation(variant, value);

                for pass in passes(operation) {
                    let expected = format!("up_{}_{}x_{}", variant.codename(), pass.scale, variant.precision());
                    assert_eq!(pass.artifact.as_str(), expected, "{operation} named a pass from something else");
                }
            }
        }
    }

    #[test]
    fn every_artifact_id_is_a_stem_carrying_no_file_extension() {
        // One artifact is often several files — a large model keeps its weights in a sibling `.onnx.data` — and which
        // ones do is discovered from the remote listing rather than predicted here.
        for variant in every_variant() {
            for artifact in operation(variant, 8.0).required_artifacts() {
                assert!(!artifact.as_str().contains('.'), "{artifact} carried a file extension");
                assert!(artifact.as_str().starts_with("up_"), "{artifact} is not named for the upscale family");
            }
        }
    }
}
