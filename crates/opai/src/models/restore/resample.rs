//! Resample first, restore second: what a restore run at a size larger than the photograph's does before any model
//! sees it.
//!
//! A generative restorer does not enlarge. PiSA-SR and SeedVR2 are both resolution-preserving, trained on photographs
//! enlarged to the target size beforehand and asked to restore detail at that size — so an enlarging run is a resample
//! to the target size followed by an ordinary restoration of the result. This wraps the restoration in that resample,
//! so the model's own pipeline is the one it runs at 1x, unchanged.
//!
//! Family tier: it names no model, and any restorer whose pipeline does not resample on its own runs through it.
//! Osaka's driver resamples inside its own region loop, because the image it was conditioned on is also the reference
//! its colour fix corrects towards, and so does not need this.

use std::sync::Arc;

use image::DynamicImage;
use image::imageops::FilterType;
use imaging::ChannelDepth;

use crate::error::InferenceError;
use crate::models::{ArtifactId, Scale};
use crate::pipeline::{Backend, ImagePipeline, Model, Shared};
use crate::providers::profile::EpProfile;
use crate::sessions::SessionHandle;

/// The pipeline `restorer` names, run over the photograph resampled to `scale` times its size.
///
/// **The strength blends against the resampled photograph**, not against the original: the restorer is handed the
/// resampled one, and the shared filter blends towards what it was handed. That is also the only image of the same
/// size there is to blend with.
pub(crate) fn resampled<B: Backend>(scale: Scale, restorer: Shared<B>) -> Shared<B> {
    Arc::new(Resampled { scale, restorer })
}

/// A restoration run at `scale`: a Lanczos3 resample, then the wrapped pipeline over the result.
struct Resampled<B: Backend> {
    /// How much larger the result is than the photograph.
    scale: Scale,
    /// What runs at the new size. Asked for its sessions and artifacts as it is, since resampling needs neither.
    restorer: Shared<B>,
}

impl<B: Backend> Model<B> for Resampled<B> {
    fn required(&self) -> &[ArtifactId] {
        self.restorer.required()
    }

    fn sessions(&self) -> Vec<(&ArtifactId, &EpProfile)> {
        self.restorer.sessions()
    }
}

impl<B: Backend> ImagePipeline<B> for Resampled<B> {
    /// The wrapped pipeline's: the resample is not a model run.
    fn stages(&self) -> usize {
        self.restorer.stages()
    }

    fn run(
        &self,
        input: &DynamicImage,
        sessions: &[SessionHandle<B::Session>],
        depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        let (width, height) = self.scale.applied_to(input.width(), input.height());

        // Skipped outright at 1x, which is a restoration of the photograph as it is and seconds of filtering to
        // produce the image it was given.
        if (width, height) == (input.width(), input.height()) {
            return self.restorer.run(input, sessions, depth, progress, cancelled);
        }

        // `resize_exact` on the `DynamicImage` keeps the photograph's own channel type, where `imageops::resize`
        // through `GenericImageView` would hand back eight bits; see Osaka's driver, which resamples the same way and
        // for the same reason. Lanczos3 is the filter the convolutional upscalers correct an overshoot with.
        let resampled = input.resize_exact(width, height, FilterType::Lanczos3);

        // The resample of a large photograph is seconds of work before the first tile, so a cancellation during it is
        // honoured before the grid starts rather than after its first tile.
        if cancelled() {
            return Err(InferenceError::Cancelled);
        }

        self.restorer.run(&resampled, sessions, depth, progress, cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::filter;
    use crate::models::precision::Precision;
    use crate::models::strength::Strength;
    use crate::models::{ArtifactId, Family};
    use crate::pipeline::test_support::{Fake, PAIR, darkened, session};
    use imaging::TileGeometry;
    use imaging::tensor::{Normalisation, Sampler};
    use imaging::test_support::photograph;

    fn restoring(scale: f64, strength: f64) -> Shared<Fake> {
        let artifact = ArtifactId::new(Family::Restore, "fake", None, Precision::Fp32);
        let strength = Strength::new(strength).expect("in range");
        let filter = filter::pipeline::<Fake>(
            "Fake (FP32)".into(),
            artifact,
            EpProfile::default(),
            Normalisation::Unit,
            strength,
            None,
            None,
            TileGeometry::DEFAULT,
        );

        resampled(Scale::new(scale).expect("in range"), filter)
    }

    fn run(pipeline: &Shared<Fake>, source: &DynamicImage) -> DynamicImage {
        let handle = session(None);

        pipeline
            .run(source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| false)
            .expect("a restoration over a photograph")
    }

    #[test]
    fn a_run_at_one_is_the_restorer_alone() {
        let source = photograph(PAIR.0, PAIR.1);
        let sampler = Sampler::new(&source);
        let produced = run(&restoring(1.0, 1.0), &source);
        let produced = produced.as_rgb8().expect("eight-bit");

        assert_eq!(produced.dimensions(), PAIR);
        for (x, y) in [(0, 0), (10, 150), (400, 100)] {
            assert_eq!(produced.get_pixel(x, y).0, darkened::<u8>(&sampler, x, y), "({x}, {y})");
        }
    }

    #[test]
    fn an_enlarging_run_restores_the_resampled_photograph() {
        let source = photograph(PAIR.0, PAIR.1);
        let enlarged = source.resize_exact(PAIR.0 * 2, PAIR.1 * 2, FilterType::Lanczos3);
        let sampler = Sampler::new(&enlarged);

        let produced = run(&restoring(2.0, 1.0), &source);
        let produced = produced.as_rgb8().expect("eight-bit");

        assert_eq!(produced.dimensions(), (PAIR.0 * 2, PAIR.1 * 2));
        for (x, y) in [(0, 0), (20, 300), (800, 200)] {
            assert_eq!(produced.get_pixel(x, y).0, darkened::<u8>(&sampler, x, y), "({x}, {y})");
        }
    }

    #[test]
    fn the_strength_blends_towards_the_resampled_photograph() {
        // At no strength the model's output is withheld entirely, and what is left is the resample: the image it was
        // conditioned on, rather than the photograph at its own size.
        let source = photograph(PAIR.0, PAIR.1);
        let enlarged = source.resize_exact(PAIR.0 * 2, PAIR.1 * 2, FilterType::Lanczos3);

        let produced = run(&restoring(2.0, 0.0), &source);

        assert_eq!(produced.as_rgb8(), enlarged.as_rgb8(), "a restoration at no strength was not the resample");
    }

    #[test]
    fn a_wrapped_restorer_asks_for_exactly_what_it_would_alone() {
        let wrapped = restoring(2.0, 1.0);

        assert_eq!(wrapped.required().len(), 1);
        assert_eq!(wrapped.sessions().len(), 1);
        assert_eq!(wrapped.stages(), 1);
    }

    #[test]
    fn a_run_cancelled_during_the_resample_produces_nothing() {
        let source = photograph(PAIR.0, PAIR.1);
        let handle = session(None);

        let result =
            restoring(2.0, 1.0).run(&source, std::slice::from_ref(&handle), ChannelDepth::Eight, None, &|| true);

        assert!(matches!(result, Err(InferenceError::Cancelled)), "{result:?}");
        assert_eq!(
            handle.session().runs.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "a tile ran after cancelling"
        );
    }
}
