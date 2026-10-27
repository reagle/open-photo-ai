//! What can be asked for in the matting family: the one row, and the enum that names it at a precision.

use serde::{Deserialize, Serialize};

use super::super::precision::{FloatPrecision, Precision};
use super::super::simple::SimpleVariant;
use super::porto;

use crate::providers::profile::EpProfile;

/// Porto, ViTMatte-S re-exported at a fixed 1024-pixel tile, and the only matting model.
pub(crate) const PORTO: SimpleVariant = SimpleVariant { codename: "porto", label: "Porto", parameters: &[] };

/// Every matting row, in the order a chooser should offer them. Read by the catalogue.
pub(crate) const ALL: [SimpleVariant; 1] = [PORTO];

/// The precisions Porto is published in: FP32 alone.
///
/// A list rather than [`FloatPrecision::PRECISIONS`], because Porto's graph is exported at full precision only
/// (104 MB); a half-precision build is a measurement away, and a row change here when it exists.
pub(crate) const PRECISIONS: [Precision; 1] = [Precision::Fp32];

/// Which matting model an operation runs, and at which precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum MattingVariant {
    /// Porto.
    Porto(FloatPrecision),
}

impl MattingVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Porto(precision) => precision.into(),
        }
    }

    /// The execution-provider tuning measured for this variant.
    pub(crate) fn profile(self) -> EpProfile {
        match self {
            Self::Porto(precision) => porto::profile(precision.into()),
        }
    }

    /// The variant this family publishes under `codename` at `precision`, or `None` where it publishes none.
    ///
    /// The inverse of the codename each variant reports, as every family's is. A precision this row does not publish
    /// answers `None` — FP16 as well as INT8, since Porto is published at FP32 alone.
    pub fn from_codename(codename: &str, precision: Precision) -> Option<Self> {
        if !PRECISIONS.contains(&precision) {
            return None;
        }

        let precision = FloatPrecision::narrow(precision)?;

        [Self::Porto(precision)].into_iter().find(|variant| variant.codename() == codename)
    }

    /// The row backing this variant.
    pub(crate) const fn row(self) -> SimpleVariant {
        match self {
            Self::Porto(_) => PORTO,
        }
    }

    /// The display text a user sees for this variant, with no precision appended.
    pub(crate) const fn label(self) -> &'static str {
        self.row().label
    }

    /// The developer-facing identifier this variant's artifact names are composed from.
    pub(crate) const fn codename(self) -> &'static str {
        self.row().codename
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every variant and precision pairing this family publishes.
    pub(crate) fn every_variant() -> Vec<MattingVariant> {
        PRECISIONS
            .iter()
            .filter_map(|precision| FloatPrecision::narrow(*precision))
            .map(MattingVariant::Porto)
            .collect()
    }

    #[test]
    fn the_family_publishes_exactly_one_variant_at_full_precision() {
        assert_eq!(ALL.len(), 1);
        assert_eq!(every_variant(), vec![MattingVariant::Porto(FloatPrecision::Fp32)]);
        assert_eq!(MattingVariant::Porto(FloatPrecision::Fp32).label(), "Porto");
        assert_eq!(MattingVariant::Porto(FloatPrecision::Fp32).codename(), "porto");
    }

    #[test]
    fn every_published_row_is_reachable_from_its_codename() {
        for row in ALL {
            for precision in PRECISIONS {
                let variant = MattingVariant::from_codename(row.codename, precision)
                    .unwrap_or_else(|| panic!("{} is published but its codename names no variant", row.codename));

                assert_eq!(variant.codename(), row.codename);
                assert_eq!(variant.precision(), precision);
            }
        }
    }

    #[test]
    fn a_precision_porto_is_not_published_at_names_nothing() {
        assert_eq!(MattingVariant::from_codename("porto", Precision::Fp16), None);
        assert_eq!(MattingVariant::from_codename("porto", Precision::Int8), None);
        assert_eq!(MattingVariant::from_codename("lisbon", Precision::Fp32), None);
        assert_eq!(MattingVariant::from_codename("Porto", Precision::Fp32), None);
    }
}
