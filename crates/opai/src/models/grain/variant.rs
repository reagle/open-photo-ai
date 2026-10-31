//! What can be asked for in the grain family: the one row, and the enum that names it at a precision.

use serde::{Deserialize, Serialize};

use super::super::catalogue::GRAIN_PARAMETERS;
use super::super::precision::{FloatPrecision, Precision};
use super::super::simple::SimpleVariant;

/// Sensor, named for the noise it draws: a digital sensor's, mostly luminance and a little coarser than a pixel. The
/// grain runs no graph; a film grain, clumped and stock-specific, would be a row beside it.
pub(crate) const SENSOR: SimpleVariant =
    SimpleVariant { codename: "sensor", label: "Sensor", parameters: &GRAIN_PARAMETERS };

/// Every grain row, in the order a chooser should offer them. Read by the catalogue.
pub(crate) const ALL: [SimpleVariant; 1] = [SENSOR];

/// The precisions the grain is published in: FP32 alone, the precision its arithmetic is done in. One precision, so a
/// chooser has no tier to name.
pub(crate) const PRECISIONS: [Precision; 1] = [Precision::Fp32];

/// Which grain an operation draws, and at which precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum GrainVariant {
    /// Sensor.
    Sensor(FloatPrecision),
}

impl GrainVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Sensor(precision) => precision.into(),
        }
    }

    /// The variant this family publishes under `codename` at `precision`, or `None` where it publishes none.
    pub fn from_codename(codename: &str, precision: Precision) -> Option<Self> {
        if !PRECISIONS.contains(&precision) {
            return None;
        }

        let precision = FloatPrecision::narrow(precision)?;

        [Self::Sensor(precision)].into_iter().find(|variant| variant.codename() == codename)
    }

    /// The row backing this variant.
    pub(crate) const fn row(self) -> SimpleVariant {
        match self {
            Self::Sensor(_) => SENSOR,
        }
    }

    /// The display text a user sees for this variant, with no precision appended.
    pub(crate) const fn label(self) -> &'static str {
        self.row().label
    }

    /// The developer-facing identifier this variant's cache tag is composed from.
    pub(crate) const fn codename(self) -> &'static str {
        self.row().codename
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every variant and precision pairing this family publishes.
    pub(crate) fn every_variant() -> Vec<GrainVariant> {
        PRECISIONS
            .iter()
            .filter_map(|precision| FloatPrecision::narrow(*precision))
            .map(GrainVariant::Sensor)
            .collect()
    }

    #[test]
    fn the_family_publishes_a_sensors_grain_at_fp32_alone() {
        assert_eq!(every_variant(), vec![GrainVariant::Sensor(FloatPrecision::Fp32)]);
        assert_eq!(GrainVariant::from_codename("sensor", Precision::Fp32), Some(every_variant()[0]));
        assert_eq!(GrainVariant::from_codename("sensor", Precision::Fp16), None);
        assert_eq!(GrainVariant::from_codename("film", Precision::Fp32), None);
    }
}
