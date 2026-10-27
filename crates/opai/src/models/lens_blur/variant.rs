//! What can be asked for in the lens blur family: the one row, and the enum that names it at a precision.

use serde::{Deserialize, Serialize};

use super::super::catalogue::LENS_BLUR_PARAMETERS;
use super::super::depth::Depth;
use super::super::matting::Matting;
use super::super::precision::{FloatPrecision, Precision};
use super::super::simple::SimpleVariant;

use crate::providers::profile::EpProfile;

/// Lisbon, named for the depth model it is driven by: the blur itself runs no graph.
pub(crate) const LISBON: SimpleVariant =
    SimpleVariant { codename: "lisbon", label: "Lisbon", parameters: &LENS_BLUR_PARAMETERS };

/// Every lens blur row, in the order a chooser should offer them. Read by the catalogue.
pub(crate) const ALL: [SimpleVariant; 1] = [LISBON];

/// The precisions Lisbon's lens blur is published in: its depth model's, FP16 alone.
pub(crate) const PRECISIONS: [Precision; 1] = super::super::depth::variant::PRECISIONS;

/// Which lens blur an operation runs, and at which precision — the precision of the depth model it is driven by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum LensBlurVariant {
    /// Lisbon.
    Lisbon(FloatPrecision),
}

impl LensBlurVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Lisbon(precision) => precision.into(),
        }
    }

    /// The depth estimate this blur is driven by, which a chain runs before it and hands it.
    pub fn depth(self) -> Depth {
        match self {
            Self::Lisbon(_) => Depth::lisbon(),
        }
    }

    /// The matting a chain runs to matte this blur's subject.
    pub(crate) fn matting(self) -> Matting {
        match self {
            Self::Lisbon(_) => Matting::porto(),
        }
    }

    /// The execution-provider tuning measured for the model this variant depends on: its depth model's, under which
    /// the graphs it requires are opened. The blur itself opens no session.
    pub(crate) fn profile(self) -> EpProfile {
        self.depth().profile()
    }

    /// The variant this family publishes under `codename` at `precision`, or `None` where it publishes none.
    pub fn from_codename(codename: &str, precision: Precision) -> Option<Self> {
        if !PRECISIONS.contains(&precision) {
            return None;
        }

        let precision = FloatPrecision::narrow(precision)?;

        [Self::Lisbon(precision)].into_iter().find(|variant| variant.codename() == codename)
    }

    /// The row backing this variant.
    pub(crate) const fn row(self) -> SimpleVariant {
        match self {
            Self::Lisbon(_) => LISBON,
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
    pub(crate) fn every_variant() -> Vec<LensBlurVariant> {
        PRECISIONS
            .iter()
            .filter_map(|precision| FloatPrecision::narrow(*precision))
            .map(LensBlurVariant::Lisbon)
            .collect()
    }

    #[test]
    fn the_family_publishes_lisbon_at_its_depth_models_precision_alone() {
        assert_eq!(every_variant(), vec![LensBlurVariant::Lisbon(FloatPrecision::Fp16)]);
        assert_eq!(LensBlurVariant::from_codename("lisbon", Precision::Fp16), Some(every_variant()[0]));
        assert_eq!(LensBlurVariant::from_codename("lisbon", Precision::Fp32), None);
        assert_eq!(LensBlurVariant::from_codename("tokyo", Precision::Fp16), None);
        assert_eq!(every_variant()[0].depth(), Depth::lisbon());
    }
}
