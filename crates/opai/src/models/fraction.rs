//! A position across the framed photograph, as a fraction of its width or height.

use serde::{Deserialize, Serialize};

use super::quantized::{quantized_param, write_thousandths};

/// A position across the framed photograph as a fraction of its width or its height: 0 is the left or top edge, 1 the
/// right or bottom one. What a lens blur's focus point is given in, so it does not depend on the photograph's size and
/// stays where it was put through an upscale.
///
/// Bounded to [`Fraction::MIN`]-[`Fraction::MAX`] inclusive and quantized to thousandths on acceptance: a step is a
/// thousandth of the frame, finer than the depth map the point is read from, which is at most 1280 pixels across.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Fraction {
    /// The quantized fraction in steps of 1/[`STEPS_PER_UNIT`](super::quantized::STEPS_PER_UNIT), always within
    /// `0..=1000`.
    steps: u16,
}

impl Fraction {
    /// The left or top edge.
    pub const MIN: f64 = 0.0;

    /// The right or bottom edge.
    pub const MAX: f64 = 1.0;

    /// The middle of the frame, where a focus point nobody placed is.
    pub const CENTRE: Self = Self { steps: 500 };
}

quantized_param! {
    Fraction(u16),
    name: "fraction",
    /// A value that is not a number bounds to the middle of the frame, where a point nobody placed is.
    not_a_number: 0.5,
}

impl std::fmt::Display for Fraction {
    /// Renders the fraction with no trailing zeros: `0`, `0.5`, `0.55`, `1`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_thousandths(f, false, self.steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    super::super::quantized::quantized_param_tests! {
        Fraction {
            accepted: [0.0, 0.25, 0.5, 0.551, 1.0],
            refused: [-0.1, 1.2],
            range: "between 0 and 1",
            not_a_number: 0.5,
            quantized: 0.123_456 => 0.123,
            alike: [0.499_99, 0.500_01],
            serialized: 0.3 => "0.3",
            refused_serialized: ["1.2", "-0.1"],
        }
    }

    #[test]
    fn a_refusal_can_be_named_for_the_axis_it_was_given_for() {
        let refused = Fraction::new(1.2).expect_err("outside the frame").named(super::super::FOCUS_X);

        assert_eq!(refused.parameter, "focus_x");
        assert_eq!(refused.to_string(), "focus_x must be between 0 and 1 inclusive, but 1.2 was supplied");
    }

    #[test]
    fn the_centre_is_the_value_the_constructor_produces_for_one_half() {
        assert_eq!(Fraction::CENTRE, Fraction::new(0.5).expect("in range"));
        assert_eq!(Fraction::CENTRE.to_string(), "0.5");
    }
}
