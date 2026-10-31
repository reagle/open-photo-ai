//! How much of the grain a restoration removed a restore run puts back.

use serde::{Deserialize, Serialize};

use super::quantized::{quantized_param, write_thousandths};

/// How much of the photograph's own grain, which a restoration smoothed away, a restore run puts back.
///
/// Bounded to [`Grain::MIN`]-[`Grain::MAX`] inclusive, and quantized to three decimal places on acceptance. It is a
/// share, in standard deviation, of what the restoration lacks against the photograph it was given, measured per band
/// of luminance: **1** puts all of it back, so the result has its source's noise; 0.5 half its deviation; **0** none,
/// which is a restore run as it was before grain existed. A clean photograph has nothing taken away and is given
/// nothing at any amount. See `imaging::grain` for how it is measured and drawn.
///
/// Stored as an integer count of quantization steps rather than as an `f64`, for the reason [`super::scale::Scale`]
/// gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Grain {
    /// The quantized amount in steps of 1/[`STEPS_PER_UNIT`](super::quantized::STEPS_PER_UNIT), always within
    /// `0..=1000`.
    steps: u16,
}

impl Grain {
    /// The smallest amount: no grain put back.
    pub const MIN: f64 = 0.0;

    /// The largest amount: all of the grain the restoration removed, so the result matches its source.
    pub const MAX: f64 = 1.0;

    /// No grain, as a value: what a restore run carries unless it is asked for some, and what a stored or sent
    /// restore without the field reads as.
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

impl Default for Grain {
    /// [`Grain::OFF`].
    fn default() -> Self {
        Self::OFF
    }
}

quantized_param! {
    Grain(u16),
    name: "grain",
    /// A value that is not a number bounds to [`Grain::MIN`], which changes an image least.
    not_a_number: Grain::MIN,
}

impl std::fmt::Display for Grain {
    /// Renders the amount as [`super::strength::Strength`] does, with no trailing zeros: `0`, `0.5`, `1`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_thousandths(f, false, self.steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    super::super::quantized::quantized_param_tests! {
        Grain {
            accepted: [0.0, 0.25, 0.5, 1.0],
            refused: [-0.1, 1.5],
            range: "between 0 and 1",
            not_a_number: Grain::MIN,
            quantized: 0.123_456 => 0.123,
            alike: [0.499_99, 0.500_01],
            serialized: 0.5 => "0.5",
            refused_serialized: ["1.5", "-0.1"],
        }
    }

    #[test]
    fn off_is_the_value_the_constructor_produces_for_the_lower_bound_and_the_default() {
        assert_eq!(Grain::OFF, Grain::new(Grain::MIN).expect("in range"));
        assert_eq!(Grain::default(), Grain::OFF);
        assert!(Grain::OFF.is_off());
        assert!(!Grain::new(0.001).expect("in range").is_off());
    }

    #[test]
    fn an_amount_renders_without_trailing_zeros() {
        assert_eq!(Grain::new(0.0).expect("in range").to_string(), "0");
        assert_eq!(Grain::new(0.5).expect("in range").to_string(), "0.5");
        assert_eq!(Grain::new(1.0).expect("in range").to_string(), "1");
    }
}
