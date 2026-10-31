//! The grain family: a sensor's noise, put back where the chain took the photograph's own away, or added on purpose.
//!
//! A denoise, a generative restore and a lens blur each leave smoother pixels than any capture of the same scene: skin,
//! sky and walls read as "plastic" next to what a camera records, and a synthetic blur reads as painted where a real
//! one keeps the sensor's noise. Every major tool with an AI restorer now offers grain back for this reason (Topaz Photo
//! AI's Grain is a finishing step after denoise, sharpening, upscaling or face recovery). What none of them does by
//! default is put back the grain *the photograph had*, which is what this family does unless asked otherwise.
//!
//! **What a run takes.** An amount and a mode, as Lightroom's Grain takes an Amount:
//!
//! - [`GrainMode::Match`], the default: the amount is a share of the grain the result lacks against the photograph's
//!   own, measured per brightness on the photograph and tile by tile on the result (see `imaging::grain`). A clean
//!   photograph gets nothing; a grainy one gets its own back, only where the chain smoothed it away. **The photograph
//!   is the chain's input, or what its last [denoise](super::denoise) made**: a denoise is a request for less grain, and
//!   matching the source it removed would undo it.
//! - [`GrainMode::Add`]: the amount is a share of a sensor's grain, [`ADDED_AT_FULL`] at mid-grey and shaped by
//!   brightness as a capture's is, added everywhere whatever is already there: grain on purpose.
//!
//! **Where it runs.** Last of all, after the lens blur; see [`Family::APPLY_ORDER`]. Never suggested by Autopilot and
//! never added by default: a front end adds it when a user asks.
//!
//! **At the result's pixels.** Under an enlargement the photograph's grain is measured per source pixel, before any
//! resample, and drawn per output pixel: what a sensor of the output's resolution would record, finer at print size than
//! the source's, never an enlarged noise field. The noise is drawn from a hash of the output's coordinates, so the same
//! run gives the same pixels and the run cache is stable. The base pixels only: a gain map is left as it is.

pub(crate) mod amount;
pub(crate) mod pipeline;
pub(crate) mod variant;

use serde::{Deserialize, Serialize};

use super::artifact::{ArtifactId, Family};
use super::operation::Operation;
use super::precision::{FloatPrecision, Precision};

use crate::error::InferenceError;
use crate::pipeline::{Backend, Shared};
use crate::providers::profile::EpProfile;

pub use amount::GrainAmount;
pub(crate) use variant::ALL;
pub use variant::GrainVariant;

/// The residual, in the unit range, of the grain [`GrainMode::Add`] adds at mid-grey at the full amount: four 8-bit
/// levels, a high-ISO capture's, about twice that as the deviation of the noise drawn.
pub const ADDED_AT_FULL: f32 = 4.0 / 255.0;

/// How much the three channels of added grain move together: mostly luminance, as a demosaiced capture's noise is.
const ADDED_CORRELATION: f32 = 0.85;

/// What a grain's amount is a share of.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrainMode {
    /// Of the grain the result lacks against the photograph's own: nothing where it has it, nothing on a clean
    /// photograph.
    #[default]
    Match,
    /// Of a sensor's grain, [`ADDED_AT_FULL`] at mid-grey, added everywhere whatever is there.
    Add,
}

impl GrainMode {
    /// The name the catalogue publishes the mode under, a switch from 0 (add) to 1 (match the source).
    pub const NAME: &str = "match";

    /// How the catalogue's switch publishes this mode: 1 matches the source, 0 adds.
    pub const fn as_switch(self) -> f64 {
        match self {
            Self::Match => 1.0,
            Self::Add => 0.0,
        }
    }

    /// The mode a switch's value names: at or above a half matches the source, anything else (a value that is not a
    /// number included) adds.
    pub fn from_switch(value: f64) -> Self {
        if value >= 0.5 { Self::Match } else { Self::Add }
    }
}

/// One grain run: which grain, how much, and what the amount is a share of.
///
/// This value **is** the operation's identity, as every family's is. Build this; keep it as an [`Operation::Grain`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Grain {
    /// The grain and its precision.
    variant: GrainVariant,
    /// How much, as a share of what [`mode`](Self::mode) names.
    amount: GrainAmount,
    /// What the amount is a share of.
    mode: GrainMode,
}

impl Grain {
    /// The operation drawing `variant` at `amount` of what `mode` names.
    pub const fn new(variant: GrainVariant, amount: GrainAmount, mode: GrainMode) -> Self {
        Self { variant, amount, mode }
    }

    /// A sensor's grain at `amount` of what `mode` names.
    ///
    /// Returns the carrier rather than `Self`, so a caller collects operations of several families into one chain with
    /// no per-element wrapping. Published at FP32 alone.
    pub fn sensor(amount: GrainAmount, mode: GrainMode) -> Operation {
        Operation::Grain(Self::new(GrainVariant::Sensor(FloatPrecision::Fp32), amount, mode))
    }

    /// Which grain this draws.
    pub const fn variant(self) -> GrainVariant {
        self.variant
    }

    /// How much, at the value it was quantized to.
    pub const fn amount(self) -> GrainAmount {
        self.amount
    }

    /// What the amount is a share of.
    pub const fn mode(self) -> GrainMode {
        self.mode
    }

    /// The precision this runs at, widened to the common currency.
    pub fn precision(self) -> Precision {
        self.variant.precision()
    }

    /// The execution-provider tuning of the model this runs: none, since it runs none.
    pub(crate) fn profile(self) -> EpProfile {
        EpProfile::default()
    }

    /// The name a user interface shows: `Sensor (FP32)`, without the amount.
    pub fn display_name(self) -> String {
        self.to_string()
    }

    /// What this run needs beyond the operation itself.
    pub const fn params(self) -> GrainParams {
        GrainParams { amount: self.amount, mode: self.mode }
    }

    /// The pipeline this operation runs — the **family's contract seam**. It cannot refuse: there is no graph to
    /// miss, and the `Result` is kept so the carrier's arm reads as every other refusable family's.
    pub(crate) fn pipeline<B: Backend>(self) -> Result<Shared<B>, InferenceError> {
        Ok(pipeline::grain::<B>(self.display_name(), self.params()))
    }

    /// The artifacts that must be on disk before this operation can run: none. The grain is arithmetic, not a model.
    pub fn required_artifacts(self) -> Vec<ArtifactId> {
        Vec::new()
    }

    /// The name a metric tags this operation's runs with, in place of a model it does not run: `gr_sensor_fp32`, shaped
    /// as an artifact name so a dashboard grouping by model reads it as one.
    pub(crate) fn model_tag(self) -> String {
        format!("{}_{}_{}", Family::Grain.prefix(), self.variant.codename(), self.variant.precision())
    }

    /// The key the run cache stores this operation's result under: `gr-sensor-fp32-m1v1` matching the source at the full
    /// amount, `gr-sensor-fp32-a0.5v1` adding half a sensor's grain.
    ///
    /// The mode's letter before the amount, since the same amount of the two is two images, and the version of the
    /// synthesis after it, so a later change to how the noise is measured or drawn never serves an image drawn the old
    /// way. What the photograph's grain is needs no place here: it is the chain's input or a step of it, and every key
    /// is folded from both.
    pub fn cache_tag(self) -> String {
        let mode = match self.mode {
            GrainMode::Match => 'm',
            GrainMode::Add => 'a',
        };

        format!(
            "{}-{}-{}-{mode}{}v{}",
            Family::Grain.prefix(),
            self.variant.codename(),
            self.variant.precision(),
            self.amount,
            imaging::grain::VERSION
        )
    }
}

/// Whether a step of `family` makes the photograph a later grain matches: a denoise does, since its result is the grain
/// the user asked to keep, and matching the chain's input would put back what it removed. The chain's input is the
/// photograph until one does.
pub(crate) fn is_grain_reference(family: Family) -> bool {
    family == Family::Denoise
}

/// What one grain run takes, in the shape its pipeline wants it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrainParams {
    /// How much, as a share of what `mode` names.
    pub amount: GrainAmount,
    /// What the amount is a share of.
    pub mode: GrainMode,
}

impl std::fmt::Display for Grain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.variant.label(), self.variant.precision().label())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn amount(value: f64) -> GrainAmount {
        GrainAmount::new(value).expect("the test supplied an amount in range")
    }

    fn sensor(value: f64, mode: GrainMode) -> Grain {
        Grain::new(GrainVariant::Sensor(FloatPrecision::Fp32), amount(value), mode)
    }

    #[test]
    fn the_cache_tag_is_the_written_one_rather_than_whatever_serialization_produces() {
        assert_eq!(sensor(1.0, GrainMode::Match).cache_tag(), "gr-sensor-fp32-m1v1");
        assert_eq!(sensor(0.5, GrainMode::Add).cache_tag(), "gr-sensor-fp32-a0.5v1");
        assert_eq!(sensor(0.0, GrainMode::Match).cache_tag(), "gr-sensor-fp32-m0v1");
    }

    #[test]
    fn no_two_runs_producing_different_images_share_one_tag() {
        let mut seen = HashMap::new();
        for mode in [GrainMode::Match, GrainMode::Add] {
            for value in [0.0, 0.25, 0.5, 1.0] {
                let operation = sensor(value, mode);
                if let Some(previous) = seen.insert(operation.cache_tag(), operation) {
                    panic!("{previous:?} and {operation:?} share the cache tag {}", operation.cache_tag());
                }
            }
        }
    }

    #[test]
    fn a_grain_runs_no_model_and_names_itself_as_one_would_be_named() {
        let operation = sensor(1.0, GrainMode::Match);

        assert!(operation.required_artifacts().is_empty());
        assert_eq!(operation.model_tag(), "gr_sensor_fp32");
        assert_eq!(operation.display_name(), "Sensor (FP32)");
        assert_eq!(operation.profile(), EpProfile::default());
    }

    #[test]
    fn an_operation_round_trips_through_its_serialized_form() {
        let operation = sensor(0.75, GrainMode::Add);
        let json = serde_json::to_string(&operation).expect("serializes");

        assert_eq!(json, r#"{"variant":{"codename":"sensor","precision":"fp32"},"amount":0.75,"mode":"add"}"#);
        assert_eq!(serde_json::from_str::<Grain>(&json).expect("deserializes"), operation);
        assert!(
            serde_json::from_str::<Grain>(&json.replace("0.75", "1.5")).is_err(),
            "an amount past 1 was accepted"
        );
    }

    #[test]
    fn the_switch_names_each_mode_and_a_half_matches() {
        for mode in [GrainMode::Match, GrainMode::Add] {
            assert_eq!(GrainMode::from_switch(mode.as_switch()), mode);
        }
        assert_eq!(GrainMode::from_switch(0.5), GrainMode::Match);
        assert_eq!(GrainMode::from_switch(f64::NAN), GrainMode::Add);
        assert_eq!(GrainMode::default(), GrainMode::Match);
    }

    #[test]
    fn only_a_denoise_makes_the_photograph_a_grain_matches() {
        let references: Vec<Family> = Family::ALL.into_iter().filter(|family| is_grain_reference(*family)).collect();

        assert_eq!(references, vec![Family::Denoise]);
    }
}
