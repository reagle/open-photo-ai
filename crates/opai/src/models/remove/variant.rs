//! What can be asked for in the remove family: the one row, and the enum that names it at a precision.

use serde::{Deserialize, Serialize};

use super::super::catalogue::MASK_PARAMETER;
use super::super::precision::{FloatPrecision, Precision};
use super::super::simple::SimpleVariant;
use super::lima;

use crate::providers::profile::EpProfile;

/// Lima, the one inpainter: Big-LaMa, exported by the fork.
pub(crate) const LIMA: SimpleVariant = SimpleVariant { codename: "lima", label: "Lima", parameters: &MASK_PARAMETER };

/// Every remove row, in the order a chooser should offer them. Read by the catalogue.
pub(crate) const ALL: [SimpleVariant; 1] = [LIMA];

/// The precisions Lima is published in: FP32 alone, the source export's.
pub(crate) const PRECISIONS: [Precision; 1] = [Precision::Fp32];

/// Which inpainter a removal runs, and at which precision.
///
/// The precision travels inside the variant, as every family's does; only FP32 is published, so [`from_codename`]
/// names nothing at FP16, and [`Remove::lima`](super::Remove::lima) builds FP32.
///
/// [`from_codename`]: Self::from_codename
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum RemoveVariant {
    /// Lima.
    Lima(FloatPrecision),
}

impl RemoveVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Lima(precision) => precision.into(),
        }
    }

    /// The execution-provider tuning measured for this variant: [`lima::profile`].
    pub(crate) fn profile(self) -> EpProfile {
        match self {
            Self::Lima(precision) => lima::profile(precision.into()),
        }
    }

    /// The variant this family publishes under `codename` at `precision`, or `None` where it publishes none.
    pub fn from_codename(codename: &str, precision: Precision) -> Option<Self> {
        if !PRECISIONS.contains(&precision) {
            return None;
        }

        let precision = FloatPrecision::narrow(precision)?;

        [Self::Lima(precision)].into_iter().find(|variant| variant.codename() == codename)
    }

    /// The row backing this variant.
    pub(crate) const fn row(self) -> SimpleVariant {
        match self {
            Self::Lima(_) => LIMA,
        }
    }

    /// The display text a user sees for this variant, with no precision appended.
    pub(crate) const fn label(self) -> &'static str {
        self.row().label
    }

    /// The developer-facing identifier this variant's artifact name is composed from.
    pub(crate) const fn codename(self) -> &'static str {
        self.row().codename
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every variant and precision pairing this family publishes.
    pub(crate) fn every_variant() -> Vec<RemoveVariant> {
        PRECISIONS
            .iter()
            .filter_map(|precision| FloatPrecision::narrow(*precision))
            .map(RemoveVariant::Lima)
            .collect()
    }

    #[test]
    fn the_family_publishes_lima_at_fp32_alone() {
        assert_eq!(every_variant(), vec![RemoveVariant::Lima(FloatPrecision::Fp32)]);
        assert_eq!(RemoveVariant::from_codename("lima", Precision::Fp32), Some(every_variant()[0]));
        assert_eq!(RemoveVariant::from_codename("lima", Precision::Fp16), None);
        assert_eq!(RemoveVariant::from_codename("lima", Precision::Int8), None);
        assert_eq!(RemoveVariant::from_codename("tokyo", Precision::Fp32), None);
        assert_eq!((every_variant()[0].label(), every_variant()[0].codename()), ("Lima", "lima"));
    }
}
