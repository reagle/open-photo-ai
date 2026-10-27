//! What can be asked for in the restore family: the rows and the enum that names one of them at a precision.

use serde::{Deserialize, Serialize};

use super::super::catalogue::RESTORE_PARAMETERS;
use super::super::precision::{FloatPrecision, Precision};
use super::super::simple::SimpleVariant;

use super::osaka::precision::OsakaPrecision;
use super::{kazan, osaka};

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

/// Osaka: SeedVR2, a one-step diffusion transformer between the two halves of a VAE.
pub(crate) const OSAKA: RestoreRow = RestoreRow {
    row: SimpleVariant { codename: "osaka", label: "Osaka", parameters: &RESTORE_PARAMETERS },
    precisions: &OsakaPrecision::PRECISIONS,
};

/// Every restore row, in the order a chooser should offer them. Read by the catalogue.
///
/// Kazan first: it is a fraction of Osaka's size and memory, and restores a soft photograph further than Osaka's
/// one-step SeedVR2 does at 1x. Osaka is the one to reach for where Kazan's redraw is not enough.
pub(crate) const ALL: [RestoreRow; 2] = [KAZAN, OSAKA];

/// Which restore model an operation runs, and at which precision.
///
/// The precision travels **inside** the variant, in the domain that model is published in, so `rs_kazan_int8` and
/// `up_osaka_fp32` are not requests this refuses — they do not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum RestoreVariant {
    /// Kazan: one 512x512 graph per tile, run through the shared filter.
    Kazan(FloatPrecision),
    /// Osaka: three graphs run as one pass over 960x960 regions, by its own driver.
    Osaka(OsakaPrecision),
}

impl RestoreVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Kazan(precision) => precision.into(),
            Self::Osaka(precision) => precision.into(),
        }
    }

    /// The execution-provider tuning declared for this variant: Kazan's is [`kazan::profile`] and Osaka's the one
    /// its row declares, which its graphs each start from.
    pub(crate) fn profile(self) -> EpProfile {
        match self {
            Self::Kazan(precision) => kazan::profile(precision.into()),
            Self::Osaka(precision) => osaka::profile(precision.into()),
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
            codename if codename == OSAKA.row.codename => OsakaPrecision::narrow(precision).map(Self::Osaka),
            _ => None,
        }
    }

    /// The row backing this variant.
    pub(crate) const fn row(self) -> RestoreRow {
        // An exhaustive match rather than a lookup, so a variant added later cannot compile without one.
        match self {
            Self::Kazan(_) => KAZAN,
            Self::Osaka(_) => OSAKA,
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
    use crate::providers::profile::{CoreMlComputeUnits, CoreMlSpecialization, ExecutionMode};

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
        assert_eq!(RestoreVariant::Osaka(OsakaPrecision::Int8).label(), "Osaka");

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
        assert_eq!(RestoreVariant::from_codename("osaka", Precision::Fp32), None, "Osaka has no FP32 build");
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
                // Measured at both of the precisions it is published at; see `osaka::profile`.
                RestoreVariant::Osaka(precision) => {
                    assert_eq!(variant.profile(), super::super::osaka::profile(precision.into()), "{variant:?}");
                    assert_ne!(variant.profile(), EpProfile::default(), "{variant:?} lost its measured profile");
                }
            }
        }
    }

    #[test]
    fn osaka_declares_every_measured_setting_at_both_precisions() {
        // Field by field and then whole, so "carries its arm's profile" cannot pass by both sides being the default —
        // and at both precisions, because nothing in this profile is a property of the export's types.
        for precision in OsakaPrecision::ALL {
            let osaka = RestoreVariant::Osaka(precision).profile();

            assert_eq!(osaka.execution_mode, ExecutionMode::Sequential, "at {precision:?}");
            assert!(osaka.disable_mem_pattern, "at {precision:?}: the memory planner was left on");
            // Both names, spelled out rather than compared against `BROKEN_OPTIMIZERS`, so that dropping one of them
            // fails here instead of agreeing with itself. The order is the declaration's; the runtime reads them as a
            // set, but the value written is a list.
            assert_eq!(
                osaka.disabled_optimizers,
                ["ReshapeFusion", "SimplifiedLayerNormFusion"],
                "at {precision:?}: the transformers the reference names for this graph were not both declared"
            );
            assert_eq!(osaka.coreml_compute_units, CoreMlComputeUnits::CpuAndGpu, "at {precision:?}");
            assert!(osaka.cuda_prefer_nhwc, "at {precision:?}: the VAE halves' layout was not declared");
            assert_eq!(
                osaka.trt_options.get("trt_builder_optimization_level").map(String::as_str),
                Some("3"),
                "at {precision:?}"
            );

            // And nothing else. TensorRT's FP16 and INT8 modes are the two absences that are decisions rather than
            // omissions — see `osaka::profile` — so a profile that named either would fail here.
            assert!(!osaka.trt_options.contains_key("trt_fp16_enable"), "at {precision:?}: FP16 mode was declared");
            assert!(!osaka.trt_options.contains_key("trt_int8_enable"), "at {precision:?}: INT8 mode was declared");
            assert_eq!(osaka.trt_options.len(), 1, "at {precision:?}: an unmeasured TensorRT option was declared");
            assert_eq!(osaka.coreml_specialization, CoreMlSpecialization::default(), "at {precision:?}");
        }

        // The two arms are the same declaration rather than two that happen to agree today, which is what the
        // precision-independence above actually claims.
        assert_eq!(
            RestoreVariant::Osaka(OsakaPrecision::Fp16).profile(),
            RestoreVariant::Osaka(OsakaPrecision::Int8).profile()
        );
    }
}
