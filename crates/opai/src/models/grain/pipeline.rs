//! The grain's run: the photograph's grain measured per brightness, what the result lacks of it measured tile by tile,
//! and the asked-for share of the difference drawn and added; or a sensor's grain added on purpose.
//!
//! **Measured on the photograph at its own size**, which the chain hands over beside the step's input (see
//! [`ImagePipeline::run_against`]): the chain's input before any resample, or what its last denoise made. A Lanczos or
//! learned enlargement softens the noise it enlarges, so measured after one the photograph's per-pixel noise would be
//! understated at 2x and 4x. The result is measured at its own pixels and the noise drawn there.
//!
//! **Tile by tile, on both**, because what the chain took is not the same everywhere: a lens blur's background has lost
//! the grain its subject kept, and a restorer smooths a cheek while it draws texture into hair of the same brightness.
//! A tile of the result holding what the photograph held at the same place, or more, is left as it is.
//!
//! Family tier: it names no model and opens no session.

use std::sync::Arc;

use image::DynamicImage;
use imaging::ChannelDepth;
use imaging::grain::{Lack, Profile, added_where, lacking, measured, sensor};

use super::{ADDED_AT_FULL, ADDED_CORRELATION, GrainMode, GrainParams};

use crate::error::InferenceError;
use crate::models::ArtifactId;
use crate::pipeline::{Backend, ImagePipeline, Model, Shared};
use crate::providers::profile::EpProfile;
use crate::sessions::SessionHandle;

/// The salt this family's noise is drawn under, so it never draws the field another caller of the hash does: `grain!!!`.
const SALT: u64 = 0x6772_6169_6e21_2121;

/// The grain `params` asks for, as the chain runs it; `name` is the operation's display name, for the log.
pub(crate) fn grain<B: Backend>(name: String, params: GrainParams) -> Shared<B> {
    Arc::new(Grained { name, params })
}

/// A grain run.
struct Grained {
    /// The operation's display name.
    name: String,
    /// How much, and of what.
    params: GrainParams,
}

impl<B: Backend> Model<B> for Grained {
    fn required(&self) -> &[ArtifactId] {
        &[]
    }

    fn sessions(&self) -> Vec<(&ArtifactId, &EpProfile)> {
        Vec::new()
    }
}

impl Grained {
    /// `input` with the grain added, matched against `photograph` where the mode matches one.
    fn grained(
        &self,
        input: &DynamicImage,
        photograph: Option<&DynamicImage>,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        if cancelled() {
            return Err(InferenceError::Cancelled);
        }

        let started = std::time::Instant::now();
        let share = self.params.amount.as_f32();
        let (profile, lack) = match self.params.mode {
            // A photograph too small to say what its grain is gives nothing to match, and nothing is added.
            GrainMode::Match => match photograph.and_then(measured).map(Profile::floored) {
                Some(reference) => {
                    let lack = lacking(&reference, photograph.expect("measured from it"), input);
                    (reference.scaled(share), lack)
                }
                None => (sensor(0.0, 0.0), Lack::everywhere(0.0)),
            },
            GrainMode::Add => (sensor(ADDED_AT_FULL * share, ADDED_CORRELATION), Lack::everywhere(1.0)),
        };

        let mut result = input.clone();
        added_where(&mut result, &profile, &lack, SALT);

        // In 8-bit levels, green, dark to light: what a log reader compares against the photograph.
        let levels = |profile: &Profile| profile.sigma.map(|band| (band[1] * 2550.0).round() / 10.0);
        tracing::debug!(
            operation = %self.name,
            mode = ?self.params.mode,
            amount = %self.params.amount,
            grain = ?levels(&profile),
            lacking = lack.mean(),
            correlation = profile.correlation,
            duration = ?started.elapsed(),
            "grain added"
        );

        if let Some(progress) = progress {
            progress(1.0);
        }

        Ok(result)
    }
}

impl<B: Backend> ImagePipeline<B> for Grained {
    /// None: the grain is not a model run.
    fn stages(&self) -> usize {
        0
    }

    /// Without a photograph to match, a grain matching its source has nothing to put back: the chain always hands it
    /// one (see [`run_against`](ImagePipeline::run_against)), and this is only what a caller without one gets.
    fn run(
        &self,
        input: &DynamicImage,
        _sessions: &[SessionHandle<B::Session>],
        _depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        self.grained(input, None, progress, cancelled)
    }

    fn measures_against_photograph(&self) -> bool {
        self.params.mode == GrainMode::Match
    }

    fn run_against(
        &self,
        input: &DynamicImage,
        photograph: &DynamicImage,
        _sessions: &[SessionHandle<B::Session>],
        _depth: ChannelDepth,
        progress: Option<&dyn Fn(f64)>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<DynamicImage, InferenceError> {
        self.grained(input, Some(photograph), progress, cancelled)
    }
}

#[cfg(test)]
mod tests {
    use image::imageops::FilterType;
    use image::{GenericImageView as _, ImageBuffer, Rgb};
    use imaging::grain::{BANDS, normals};

    use super::*;
    use crate::models::grain::GrainAmount;
    use crate::pipeline::test_support::{Fake, session};

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

    fn pipeline(value: f64, mode: GrainMode) -> Shared<Fake> {
        let params = GrainParams { amount: GrainAmount::new(value).expect("in range"), mode };
        grain("Sensor (FP32)".into(), params)
    }

    fn alone(pipeline: &Shared<Fake>, input: &DynamicImage, cancelled: bool) -> Result<DynamicImage, InferenceError> {
        let handle = session(None);
        pipeline.run(input, std::slice::from_ref(&handle), ChannelDepth::Sixteen, None, &|| cancelled)
    }

    fn against(pipeline: &Shared<Fake>, input: &DynamicImage, photograph: &DynamicImage) -> DynamicImage {
        let handle = session(None);
        pipeline
            .run_against(input, photograph, std::slice::from_ref(&handle), ChannelDepth::Sixteen, None, &|| false)
            .expect("a grain")
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
    fn a_photographs_grain_is_put_back_on_a_result_that_lost_it() {
        let photograph = noisy(&clean(640, 480));
        let result = against(&pipeline(1.0, GrainMode::Match), &clean(640, 480), &photograph);

        let wanted = measured(&photograph).expect("measured");
        let got = measured(&result).expect("measured");
        assert!(worst(&got, &wanted) < 0.10, "{got:?} against {wanted:?}");
        assert!((got.correlation - wanted.correlation).abs() < 0.1, "{got:?} against {wanted:?}");
    }

    #[test]
    fn an_enlarged_result_is_given_the_photographs_per_pixel_grain_at_its_own_pixels() {
        let photograph = noisy(&clean(400, 300));
        let enlarged = clean(400, 300).resize_exact(800, 600, FilterType::Lanczos3);
        let result = against(&pipeline(1.0, GrainMode::Match), &enlarged, &photograph);

        assert_eq!(result.dimensions(), (800, 600));
        let wanted = measured(&photograph).expect("measured");
        let got = measured(&result).expect("measured");
        assert!(worst(&got, &wanted) < 0.15, "{got:?} against {wanted:?}");
    }

    #[test]
    fn half_the_amount_is_half_the_missing_deviation() {
        let photograph = noisy(&clean(640, 480));
        let result = against(&pipeline(0.5, GrainMode::Match), &clean(640, 480), &photograph);

        let wanted = measured(&photograph).expect("measured").scaled(0.5);
        let got = measured(&result).expect("measured");
        assert!(worst(&got, &wanted) < 0.10, "{got:?} against {wanted:?}");
    }

    #[test]
    fn a_result_that_is_its_photograph_is_given_nothing() {
        // What a lens blur's subject hands over, or a chain that smoothed nothing: the photograph's own uneven noise is
        // compared with itself, place by place.
        let photograph = noisy(&clean(640, 480));

        assert_eq!(against(&pipeline(1.0, GrainMode::Match), &photograph, &photograph), photograph);
    }

    #[test]
    fn a_clean_photograph_gives_nothing_to_match() {
        let input = clean(320, 240);

        assert_eq!(against(&pipeline(1.0, GrainMode::Match), &input, &clean(320, 240)), input);
    }

    #[test]
    fn the_same_grain_draws_the_same_pixels() {
        let photograph = noisy(&clean(300, 200));
        let grain = pipeline(0.5, GrainMode::Match);

        assert_eq!(against(&grain, &clean(300, 200), &photograph), against(&grain, &clean(300, 200), &photograph));
    }

    #[test]
    fn added_grain_is_a_sensors_whatever_the_photograph_and_only_a_match_asks_for_one() {
        let input = clean(640, 480);
        let added = pipeline(0.5, GrainMode::Add);
        let with = against(&added, &input, &noisy(&input));

        assert_eq!(with, alone(&added, &input, false).expect("a grain"), "adding grain looked at the photograph");
        assert!(!added.measures_against_photograph());
        assert!(pipeline(1.0, GrainMode::Match).measures_against_photograph());

        let wanted = sensor(ADDED_AT_FULL * 0.5, ADDED_CORRELATION);
        let got = measured(&with).expect("measured");
        // The ramp's end bands are measured at its ends, not at their centres, where the shape moves fastest.
        for band in 1..BANDS - 1 {
            let (got, wanted) = (got.sigma[band][1], wanted.sigma[band][1]);
            assert!((got - wanted).abs() / wanted < 0.12, "band {band}: {got} against {wanted}");
        }
    }

    #[test]
    fn a_match_with_no_photograph_adds_nothing_and_a_grain_opens_no_session() {
        let input = noisy(&clean(200, 100));
        let grain = pipeline(1.0, GrainMode::Match);

        assert_eq!(alone(&grain, &input, false).expect("a grain"), input);
        assert!(grain.required().is_empty());
        assert!(grain.sessions().is_empty());
        assert_eq!(grain.stages(), 0);
    }

    #[test]
    fn a_cancelled_grain_returns_no_picture() {
        let result = alone(&pipeline(1.0, GrainMode::Add), &clean(64, 64), true);

        assert!(matches!(result, Err(InferenceError::Cancelled)), "{result:?}");
    }
}
