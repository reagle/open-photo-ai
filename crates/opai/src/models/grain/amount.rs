//! How much grain a grain run puts back, or adds.

use serde::{Deserialize, Serialize};

use super::super::quantized::{quantized_param, write_thousandths};

/// How much grain a [`Grain`](super::Grain) run puts back of what the chain took away, or adds of a sensor's own.
///
/// Bounded to [`GrainAmount::MIN`]-[`GrainAmount::MAX`] inclusive, and quantized to three decimal places on acceptance.
/// Matching the source, it is a share, in standard deviation, of what the result lacks against the photograph's own
/// grain, measured per band of luminance and per tile: **1** puts all of it back, so the result has its source's noise;
/// 0.5 half its deviation; **0** none. A clean photograph has nothing taken away and is given nothing at any amount.
/// Adding, it is a share of [`super::ADDED_AT_FULL`], a sensor's grain at mid-grey. See `imaging::grain` for how it is
/// measured and drawn.
///
/// Stored as an integer count of quantization steps rather than as an `f64`, for the reason [`super::super::scale::Scale`]
/// gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct GrainAmount {
    /// The quantized amount in steps of 1/[`STEPS_PER_UNIT`](super::super::quantized::STEPS_PER_UNIT), always within
    /// `0..=1000`.
    steps: u16,
}

impl GrainAmount {
    /// The smallest amount: no grain.
    pub const MIN: f64 = 0.0;

    /// The largest amount: all of the grain the chain removed, so the result matches its source.
    pub const MAX: f64 = 1.0;

    /// No grain, as a value: the neutral amount a caller that set none falls back to.
    pub const OFF: Self = Self { steps: 0 };

    /// Whether this puts nothing back.
    pub const fn is_off(&self) -> bool {
        self.steps == 0
    }

    /// The amount as the `f32` the per-pixel arithmetic is done in.
    pub(crate) fn as_f32(self) -> f32 {
        #[expect(clippy::cast_possible_truncation, reason = "an amount is bounded to 0.0..=1.0 at construction")]
        let narrowed = self.get() as f32;

        narrowed
    }
}

impl Default for GrainAmount {
    /// [`GrainAmount::OFF`].
    fn default() -> Self {
        Self::OFF
    }
}

quantized_param! {
    GrainAmount(u16),
    name: "amount",
    /// A value that is not a number bounds to [`GrainAmount::MIN`], which changes an image least.
    not_a_number: GrainAmount::MIN,
}

impl std::fmt::Display for GrainAmount {
    /// Renders the amount as [`super::super::strength::Strength`] does, with no trailing zeros: `0`, `0.5`, `1`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_thousandths(f, false, self.steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    super::super::super::quantized::quantized_param_tests! {
        GrainAmount {
            accepted: [0.0, 0.25, 0.5, 1.0],
            refused: [-0.1, 1.5],
            range: "between 0 and 1",
            not_a_number: GrainAmount::MIN,
            quantized: 0.123_456 => 0.123,
            alike: [0.499_99, 0.500_01],
            serialized: 0.5 => "0.5",
            refused_serialized: ["1.5", "-0.1"],
        }
    }

    #[test]
    fn off_is_the_value_the_constructor_produces_for_the_lower_bound_and_the_default() {
        assert_eq!(GrainAmount::OFF, GrainAmount::new(GrainAmount::MIN).expect("in range"));
        assert_eq!(GrainAmount::default(), GrainAmount::OFF);
        assert!(GrainAmount::OFF.is_off());
        assert!(!GrainAmount::new(0.001).expect("in range").is_off());
    }

    #[test]
    fn an_amount_renders_without_trailing_zeros() {
        assert_eq!(GrainAmount::new(0.0).expect("in range").to_string(), "0");
        assert_eq!(GrainAmount::new(0.5).expect("in range").to_string(), "0.5");
        assert_eq!(GrainAmount::new(1.0).expect("in range").to_string(), "1");
    }
}
