//! What can be asked for in the depth family: the one row, and the enum that names it at a precision.

use serde::{Deserialize, Serialize};

use super::super::precision::{FloatPrecision, Precision};
use super::super::simple::SimpleVariant;
use super::lisbon;

use crate::providers::profile::EpProfile;

/// Lisbon, MoGe-2 ViT-B re-exported at a fixed size per orientation, and the only depth model published.
pub(crate) const LISBON: SimpleVariant = SimpleVariant { codename: "lisbon", label: "Lisbon", parameters: &[] };

/// Every depth row, in the order a chooser should offer them. Read by the catalogue.
pub(crate) const ALL: [SimpleVariant; 1] = [LISBON];

/// The precisions Lisbon is published in: FP16 alone.
///
/// A list rather than [`FloatPrecision::PRECISIONS`], because Lisbon's graphs are exported at half precision only (2 x
/// 324 MB); an FP32 build would be another 2 x 420 MB for a CPU-only machine, and is a row change here when it exists.
pub(crate) const PRECISIONS: [Precision; 1] = [Precision::Fp16];

// The variant carries a `FloatPrecision` rather than nothing, although only one precision is published: an FP32 build
// is the open question design D2 records, and the contract is then a list change rather than a new type. What keeps an
// FP32 request from reaching an artifact name is `from_codename`, which consults `PRECISIONS`.
/// Which depth model an operation runs, and at which precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum DepthVariant {
    /// Lisbon.
    Lisbon(FloatPrecision),
}

impl DepthVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Lisbon(precision) => precision.into(),
        }
    }

    /// The execution-provider tuning measured for this variant.
    pub(crate) fn profile(self) -> EpProfile {
        // Asked of the model's own file, which is where the measurement and the prose behind it live.
        match self {
            Self::Lisbon(precision) => lisbon::profile(precision.into()),
        }
    }

    /// The variant this family publishes under `codename` at `precision`, or `None` where it publishes none.
    ///
    /// The inverse of the codename each variant reports, as every family's is. A precision this row does not publish
    /// answers `None` — FP32 as well as INT8, since Lisbon is published at FP16 alone.
    pub fn from_codename(codename: &str, precision: Precision) -> Option<Self> {
        if !PRECISIONS.contains(&precision) {
            return None;
        }

        let precision = FloatPrecision::narrow(precision)?;

        [Self::Lisbon(precision)].into_iter().find(|variant| variant.codename() == codename)
    }

    /// The row backing this variant.
    pub(crate) const fn row(self) -> SimpleVariant {
        // An exhaustive match rather than a lookup, so a variant added later cannot compile without one.
        match self {
            Self::Lisbon(_) => LISBON,
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
    pub(crate) fn every_variant() -> Vec<DepthVariant> {
        PRECISIONS
            .iter()
            .filter_map(|precision| FloatPrecision::narrow(*precision))
            .map(DepthVariant::Lisbon)
            .collect()
    }

    #[test]
    fn the_family_publishes_exactly_one_variant_at_half_precision() {
        assert_eq!(ALL.len(), 1);
        assert_eq!(every_variant(), vec![DepthVariant::Lisbon(FloatPrecision::Fp16)]);
        assert_eq!(DepthVariant::Lisbon(FloatPrecision::Fp16).label(), "Lisbon");
        assert_eq!(DepthVariant::Lisbon(FloatPrecision::Fp16).codename(), "lisbon");
    }

    #[test]
    fn every_published_row_is_reachable_from_its_codename() {
        for row in ALL {
            for precision in PRECISIONS {
                let variant = DepthVariant::from_codename(row.codename, precision)
                    .unwrap_or_else(|| panic!("{} is published but its codename names no variant", row.codename));

                assert_eq!(variant.codename(), row.codename);
                assert_eq!(variant.precision(), precision);
            }
        }
    }

    #[test]
    fn a_precision_lisbon_is_not_published_at_names_nothing() {
        // FP32 is a value the type can hold, which is why the refusal is the row's rather than the type's.
        assert_eq!(DepthVariant::from_codename("lisbon", Precision::Fp32), None);
        assert_eq!(DepthVariant::from_codename("lisbon", Precision::Int8), None);
        assert_eq!(DepthVariant::from_codename("newyork", Precision::Fp16), None);
        assert_eq!(DepthVariant::from_codename("Lisbon", Precision::Fp16), None);
    }
}
