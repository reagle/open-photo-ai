//! The photograph's own grain, put back into a restoration: measured on the photograph the restorer was handed and on
//! what it made, and the share of the difference a run asks for added to the result as noise.
//!
//! A generative restorer draws no sensor noise, so what it makes is smoother than any capture of the same scene. This
//! wraps a restorer's whole pipeline — outside [`resample`](super::resample), and around Osaka's driver, which resamples
//! inside itself — so it sees the photograph before any enlargement and the result after the strength blend:
//!
//! - **Before the resample**, because a Lanczos enlargement softens the noise it enlarges, and measured after it the
//!   source's per-pixel noise would be understated at 2x and 4x. The noise is drawn per **output** pixel at the
//!   per-pixel deviation the source had: what a sensor of the output's resolution would record, finer at print size
//!   than the source's and never an enlarged noise field.
//! - **After the strength blend**, because a blended result already holds part of the photograph's noise: less is
//!   missing and less is added, and at no strength nothing is.
//!
//! The base pixels only: the gain map an Ultra HDR photograph carries is not this pipeline's to rewrite, and an HDR
//! display raises the grain with the highlight it raises, as it would a capture's own noise. See `imaging::grain` for
//! the measurement and the synthesis.
//!
//! Family tier: it names no model, and both of the family's contracts run through it.

use std::sync::Arc;

use image::DynamicImage;
use imaging::ChannelDepth;
use imaging::grain::{added, measured, missing};

use crate::error::InferenceError;
use crate::models::ArtifactId;
use crate::models::grain::Grain;
use crate::pipeline::{Backend, ImagePipeline, Model, Shared};
use crate::providers::profile::EpProfile;
use crate::sessions::SessionHandle;

/// The salt this family's noise is drawn under, so it never draws the field another caller of the hash does.
const SALT: u64 = 0x7265_7374_6f72_6521;

/// `restorer`, putting back `grain` of the noise it removes; `restorer` itself, unwrapped, at no grain, so a run
/// without grain is exactly the run it was before grain existed.
pub(crate) fn grained<B: Backend>(grain: Grain, restorer: Shared<B>) -> Shared<B> {
    if grain.is_off() {
        return restorer;
    }

    Arc::new(Grained { grain, restorer })
}

/// A restoration with `grain` of the photograph's noise put back.
struct Grained<B: Backend> {
    /// The share of the missing deviation to add, above 0.
    grain: Grain,
    /// The whole restorer, resample included. Asked for its sessions and artifacts as it is, since the grain needs
    /// neither.
    restorer: Shared<B>,
}

impl<B: Backend> Model<B> for Grained<B> {
    fn required(&self) -> &[ArtifactId] {
        self.restorer.required()
    }

    fn sessions(&self) -> Vec<(&ArtifactId, &EpProfile)> {
        self.restorer.sessions()
    }
}

impl<B: Backend> ImagePipeline<B> for Grained<B> {
    /// The wrapped pipeline's: the grain is not a model run.
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
        let started = std::time::Instant::now();
        let source = measured(input);
        let measuring = started.elapsed();
        let mut result = self.restorer.run(input, sessions, depth, progress, cancelled)?;

        // A photograph too small to say what its grain is, or a result too small to say what it has, is left as the
        // restorer made it.
        let started = std::time::Instant::now();
        if let Some(source) = source
            && let Some(made) = measured(&result)
        {
            let lacking = missing(&source, &made).scaled(self.grain.as_f32());
            added(&mut result, &lacking, SALT);

            // In 8-bit levels, green, dark to light: what a log reader compares against the photograph.
            let levels =
                |profile: &imaging::grain::Profile| profile.sigma.map(|band| (band[1] * 2550.0).round() / 10.0);
            tracing::debug!(
                grain = %self.grain,
                source = ?levels(&source),
                restored = ?levels(&made),
                added = ?levels(&lacking),
                correlation = source.correlation,
                duration = ?(measuring + started.elapsed()),
                "grain put back"
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use image::imageops::FilterType;
    use image::{GenericImageView as _, ImageBuffer, Rgb};
    use imaging::TileGeometry;
    use imaging::grain::{BANDS, Profile, normals};
    use imaging::tensor::Normalisation;

    use super::*;
    use crate::models::filter;
    use crate::models::precision::Precision;
    use crate::models::restore::resample::resampled;
    use crate::models::strength::Strength;
    use crate::models::{Family, Scale};
    use crate::pipeline::test_support::{Fake, session};

    /// A stand-in restorer that hands back a clean picture at `scale` times the size of whatever it is given: what a
    /// generative restorer does to a noisy photograph of that picture, with nothing else changed.
    struct Cleaning {
        clean: DynamicImage,
        scale: u32,
    }

    impl Model<Fake> for Cleaning {
        fn required(&self) -> &[ArtifactId] {
            &[]
        }

        fn sessions(&self) -> Vec<(&ArtifactId, &EpProfile)> {
            Vec::new()
        }
    }

    impl ImagePipeline<Fake> for Cleaning {
        fn stages(&self) -> usize {
            1
        }

        fn run(
            &self,
            input: &DynamicImage,
            _sessions: &[SessionHandle<<Fake as Backend>::Session>],
            _depth: ChannelDepth,
            _progress: Option<&dyn Fn(f64)>,
            _cancelled: &dyn Fn() -> bool,
        ) -> Result<DynamicImage, InferenceError> {
            assert_eq!(input.dimensions(), self.clean.dimensions(), "the restorer was handed a resampled photograph");
            let (width, height) = (input.width() * self.scale, input.height() * self.scale);

            Ok(self.clean.resize_exact(width, height, FilterType::Lanczos3))
        }
    }

    /// A smooth picture over most of the range: a horizontal ramp from 0.08 to 0.92 with a tint, at 16 bits, so every
    /// band of luminance it reaches holds thousands of samples.
    fn clean(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb16(ImageBuffer::from_fn(width, height, |x, _| {
            let level = 0.08 + 0.84 * f64::from(x) / f64::from(width - 1);
            Rgb([0.0, 0.02, -0.02].map(|tint| ((level + tint) * 65535.0).round() as u16))
        }))
    }

    /// `picture` with sensor-like noise: signal-dependent (rising with the level, then falling towards white), mostly
    /// shared by the three channels.
    fn noisy(picture: &DynamicImage) -> DynamicImage {
        let picture = picture.to_rgb16();
        DynamicImage::ImageRgb16(ImageBuffer::from_fn(picture.width(), picture.height(), |x, y| {
            let [shared, r, g, b] = normals(u64::from(x) + 10_000, u64::from(y), 99);
            let pixel = picture.get_pixel(x, y).0;
            let level = f32::from(pixel[1]) / 65535.0;
            let sigma = 0.004 + 0.05 * level * (1.0 - level);
            let mut out = [0_u16; 3];
            for (channel, own) in [r, g, b].into_iter().enumerate() {
                let value = f32::from(pixel[channel]) / 65535.0 + sigma * 0.9_f32.mul_add(shared, 0.3 * own);
                out[channel] = (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
            }
            Rgb(out)
        }))
    }

    fn run(pipeline: &Shared<Fake>, source: &DynamicImage) -> DynamicImage {
        let handle = session(None);
        pipeline
            .run(source, std::slice::from_ref(&handle), ChannelDepth::Sixteen, None, &|| false)
            .expect("a restoration")
    }

    fn restorer(source: &DynamicImage, scale: u32, grain: f64) -> Shared<Fake> {
        let cleaning: Shared<Fake> = Arc::new(Cleaning { clean: clean(source.width(), source.height()), scale });
        grained(Grain::new(grain).expect("in range"), cleaning)
    }

    /// The worst relative difference between two profiles' deviations, over every band and channel.
    fn worst(measured: &Profile, wanted: &Profile) -> f32 {
        (0..BANDS)
            .flat_map(|band| (0..3).map(move |channel| (band, channel)))
            .map(|(band, channel)| {
                let (got, expected) = (measured.sigma[band][channel], wanted.sigma[band][channel]);
                (got - expected).abs() / expected
            })
            .fold(0.0, f32::max)
    }

    #[test]
    fn no_grain_is_the_restorer_itself() {
        let artifact = ArtifactId::new(Family::Restore, "fake", None, Precision::Fp32);
        let filter = filter::pipeline::<Fake>(
            "Fake (FP32)".into(),
            artifact,
            EpProfile::default(),
            Normalisation::Unit,
            Strength::new(1.0).expect("in range"),
            None,
            TileGeometry::DEFAULT,
        );
        let plain = resampled(Scale::new(2.0).expect("in range"), filter);
        let wrapped = grained(Grain::OFF, plain.clone());

        assert!(Arc::ptr_eq(&plain, &wrapped), "grain off wrapped the restorer");
        let source = noisy(&clean(200, 120));
        assert_eq!(run(&wrapped, &source), run(&plain, &source));
    }

    #[test]
    fn a_noisy_photographs_grain_is_matched_at_its_own_size() {
        let source = noisy(&clean(640, 480));
        let restored = run(&restorer(&source, 1, 1.0), &source);

        let wanted = measured(&source).expect("measured");
        let got = measured(&restored).expect("measured");
        assert!(worst(&got, &wanted) < 0.10, "{got:?} against {wanted:?}");
        assert!((got.correlation - wanted.correlation).abs() < 0.1, "{got:?} against {wanted:?}");
    }

    #[test]
    fn an_enlarging_restore_is_given_the_sources_per_pixel_grain_at_its_own_pixels() {
        let source = noisy(&clean(400, 300));
        let restored = run(&restorer(&source, 2, 1.0), &source);

        assert_eq!(restored.dimensions(), (800, 600));
        let wanted = measured(&source).expect("measured");
        let got = measured(&restored).expect("measured");
        assert!(worst(&got, &wanted) < 0.15, "{got:?} against {wanted:?}");
    }

    #[test]
    fn half_the_grain_is_half_the_missing_deviation() {
        let source = noisy(&clean(640, 480));
        let restored = run(&restorer(&source, 1, 0.5), &source);

        let wanted = measured(&source).expect("measured").scaled(0.5);
        let got = measured(&restored).expect("measured");
        assert!(worst(&got, &wanted) < 0.10, "{got:?} against {wanted:?}");
    }

    #[test]
    fn a_clean_photograph_is_given_nothing() {
        let source = clean(320, 240);
        let restored = run(&restorer(&source, 1, 1.0), &source);

        assert_eq!(restored, source.resize_exact(320, 240, FilterType::Lanczos3));
    }

    #[test]
    fn the_same_restore_draws_the_same_pixels() {
        let source = noisy(&clean(300, 200));
        let pipeline = restorer(&source, 2, 0.5);

        assert_eq!(run(&pipeline, &source), run(&pipeline, &source));
    }

    #[test]
    fn a_wrapped_restorer_asks_for_exactly_what_it_would_alone() {
        let source = clean(10, 10);
        let pipeline = restorer(&source, 1, 1.0);

        assert!(pipeline.required().is_empty());
        assert!(pipeline.sessions().is_empty());
        assert_eq!(pipeline.stages(), 1);
    }
}
