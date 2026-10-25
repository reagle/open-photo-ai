//! What can be asked for in the restore family: the rows and the enum that names one of them at a precision.

use serde::{Deserialize, Serialize};

use super::super::catalogue::RESTORE_PARAMETERS;
use super::super::precision::{FloatPrecision, Precision};
use super::super::simple::SimpleVariant;

use super::kazan;
use imaging::TileGeometry;

use crate::providers::profile::EpProfile;

/// One restore model as the catalogue publishes it: its row, and the precisions it is published at.
///
/// The precisions are on the row rather than assumed, unlike the single-graph families', because the generative
/// models are not all published at the same pair: Kazan is exported at FP32 and FP16, and a diffusion model whose
/// transformer is quantized publishes INT8 where FP32 would be. Each row states its own, in the order a chooser offers
/// them, so the catalogue reads the fact rather than a second list beside the models.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RestoreRow {
    /// The codename, label and parameters, in the shape every single-graph family's rows share.
    pub(crate) row: SimpleVariant,
    /// Every precision this model is published at, in the order a chooser should offer them.
    pub(crate) precisions: &'static [Precision],
}

/// Kazan, the restorer a front end offers first: PiSA-SR on Stable Diffusion 2.1.
pub(crate) const KAZAN: RestoreRow = RestoreRow {
    row: SimpleVariant { codename: "kazan", label: "Kazan", parameters: &RESTORE_PARAMETERS },
    precisions: &FloatPrecision::PRECISIONS,
};

/// Every restore row, in the order a chooser should offer them. Read by the catalogue.
pub(crate) const ALL: [RestoreRow; 1] = [KAZAN];

/// Which restore model an operation runs, and at which precision.
///
/// The precision travels **inside** the variant, in the domain that model is published in, so `rs_kazan_int8` is not
/// a request this refuses — it does not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum RestoreVariant {
    /// Kazan: one 512x512 graph per tile, run through the shared filter.
    Kazan(FloatPrecision),
}

impl RestoreVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Kazan(precision) => precision.into(),
        }
    }

    /// The execution-provider tuning declared for this variant: Kazan's is [`kazan::profile`].
    pub(crate) fn profile(self) -> EpProfile {
        match self {
            Self::Kazan(precision) => kazan::profile(precision.into()),
        }
    }

    /// The tile this variant's graph accepts and how far tiles overlap: [`kazan::GEOMETRY`].
    pub(crate) const fn geometry(self) -> TileGeometry {
        match self {
            Self::Kazan(_) => kazan::GEOMETRY,
        }
    }

    /// The variant this family publishes under `codename` at `precision`, or `None` where it publishes none.
    ///
    /// The inverse of the codename each variant reports, as every family's `from_codename` is. A precision the model
    /// is not published at answers `None` rather than being narrowed to one it is.
    pub fn from_codename(codename: &str, precision: Precision) -> Option<Self> {
        // Matched on the row's own constant rather than a literal, so a renamed row cannot leave this behind.
        match codename {
            codename if codename == KAZAN.row.codename => FloatPrecision::narrow(precision).map(Self::Kazan),
            _ => None,
        }
    }

    /// The row backing this variant.
    pub(crate) const fn row(self) -> RestoreRow {
        // An exhaustive match rather than a lookup, so a variant added later cannot compile without one.
        match self {
            Self::Kazan(_) => KAZAN,
        }
    }

    /// The display text a user sees for this variant, with no precision or scale appended.
    pub(crate) const fn label(self) -> &'static str {
        self.row().row.label
    }

    /// The developer-facing identifier this variant's artifact names are composed from.
    pub(crate) const fn codename(self) -> &'static str {
        self.row().row.codename
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every variant and precision pairing this family publishes, derived from the rows' own precision lists.
    pub(crate) fn every_variant() -> Vec<RestoreVariant> {
        ALL.iter()
            .flat_map(|model| {
                model
                    .precisions
                    .iter()
                    .map(|precision| RestoreVariant::from_codename(model.row.codename, *precision))
            })
            .map(|variant| variant.expect("every published pairing names a variant"))
            .collect()
    }

    #[test]
    fn every_variant_reports_its_label_codename_and_precision() {
        assert_eq!(RestoreVariant::Kazan(FloatPrecision::Fp32).label(), "Kazan");

        for variant in every_variant() {
            assert_eq!(variant.codename(), variant.label().to_lowercase(), "{variant:?}");
            assert!(variant.row().precisions.contains(&variant.precision()), "{variant:?}");
        }
    }

    #[test]
    fn every_published_row_is_reachable_from_its_codename() {
        // Both directions, for the reason sharpen's test of the same name gives: a row the enum cannot name is offered
        // and refused, and a variant no row publishes is one the catalogue offers no way to choose.
        for model in ALL {
            for precision in model.precisions {
                let variant = RestoreVariant::from_codename(model.row.codename, *precision)
                    .unwrap_or_else(|| panic!("{} is published but names no variant", model.row.codename));

                assert_eq!(variant.codename(), model.row.codename);
                assert_eq!(variant.precision(), *precision);
            }
        }

        assert_eq!(every_variant().len(), ALL.iter().map(|model| model.precisions.len()).sum::<usize>());
    }

    #[test]
    fn a_pairing_this_family_does_not_publish_names_nothing() {
        assert_eq!(RestoreVariant::from_codename("moscow", Precision::Fp32), None, "a sharpen codename named one");
        assert_eq!(RestoreVariant::from_codename("", Precision::Fp32), None);
        assert_eq!(RestoreVariant::from_codename("KAZAN", Precision::Fp32), None);
        assert_eq!(RestoreVariant::from_codename("kazan", Precision::Int8), None, "Kazan has no INT8 build");
    }

    #[test]
    fn every_variant_reaches_its_own_models_profile() {
        for variant in every_variant() {
            match variant {
                RestoreVariant::Kazan(FloatPrecision::Fp16) => {
                    assert_ne!(variant.profile(), EpProfile::default(), "{variant:?} lost its declared profile");
                }
                RestoreVariant::Kazan(FloatPrecision::Fp32) => {
                    assert_eq!(variant.profile(), EpProfile::default(), "{variant:?} declared a setting at FP32");
                }
            }
        }
    }
}
