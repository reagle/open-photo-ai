//! The aperture a lens blur simulates, as a 35 mm full-frame-equivalent f-number.

use serde::{Deserialize, Serialize};

use super::quantized::{quantized_param, write_thousandths};

/// The f-number a lens blur simulates, in **35 mm full-frame-equivalent** terms: the real f-number times the camera's
/// crop factor, so one value blurs a phone's photograph and a camera's by the same fraction of the frame.
///
/// Bounded to [`FNumber::MIN`]-[`FNumber::MAX`] inclusive and quantized to three decimal places on acceptance, as every
/// bounded parameter is. Smaller is a wider aperture and more blur; at or above the photograph's own equivalent
/// f-number a lens blur adds nothing.
///
/// Stored as an integer count of quantization steps rather than as an `f64`, for the reason [`super::scale::Scale`]
/// gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct FNumber {
    /// The quantized f-number in steps of 1/[`STEPS_PER_UNIT`](super::quantized::STEPS_PER_UNIT), always within
    /// `1000..=32000`.
    steps: u16,
}

impl FNumber {
    /// The widest aperture offered: f/1.0, faster than any lens most photographs were taken with, so the slider can
    /// always go past the photograph's own.
    pub const MIN: f64 = 1.0;

    /// The narrowest aperture offered: f/32, past which no lens adds blur to any photograph.
    pub const MAX: f64 = 32.0;

    /// The f-number as the `f32` the lens maths is done in.
    pub(crate) fn as_f32(self) -> f32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an f-number is bounded to 1.0..=32.0 at construction"
        )]
        let narrowed = self.get() as f32;

        narrowed
    }
}

quantized_param! {
    FNumber(u16),
    name: "f_number",
    /// A value that is not a number bounds to [`FNumber::MAX`], the narrowest aperture, which adds the least blur.
    not_a_number: FNumber::MAX,
}

impl std::fmt::Display for FNumber {
    /// Renders the f-number with no trailing zeros: `1.4`, `2.8`, `8`, the way a lens is labelled.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_thousandths(f, false, self.steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    super::super::quantized::quantized_param_tests! {
        FNumber {
            accepted: [1.0, 1.4, 2.8, 5.6, 32.0],
            refused: [0.5, 33.0],
            range: "between 1 and 32",
            not_a_number: FNumber::MAX,
            quantized: 2.345_67 => 2.346,
            alike: [2.800_1, 2.799_9],
            serialized: 1.4 => "1.4",
            refused_serialized: ["0.9", "64"],
        }
    }

    #[test]
    fn an_f_number_renders_as_a_lens_is_labelled() {
        assert_eq!(FNumber::new(1.4).expect("in range").to_string(), "1.4");
        assert_eq!(FNumber::new(8.0).expect("in range").to_string(), "8");
        assert_eq!(FNumber::new(2.83).expect("in range").to_string(), "2.83");
    }
}
