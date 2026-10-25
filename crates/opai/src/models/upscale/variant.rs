//! What distinguishes one upscale variant from another, as data.

// A row per variant rather than a type per variant: everything that differs between Kyoto and Saitama is a codename, a
// display label and a table, and it is the exhaustiveness of `UpscaleVariant` that makes a new row mandatory rather
// than a registration step someone can forget.

use serde::{Deserialize, Serialize};

use super::super::catalogue::ParameterEntry;
use super::super::precision::{FloatPrecision, Precision};
use super::super::scale::Scale;
use super::MODELS;
use super::conv::PassShape;
use super::kyoto::KYOTO;
use super::resolve::Resolution;
use super::saitama::SAITAMA;
use super::tokyo::TOKYO;

use crate::providers::profile::EpProfile;

// Object-safe on purpose: every method answers with **data** — a name, a precision list, a profile value, a
// `Resolution` — and none of them is generic over a backend, holds a session or returns a pipeline.
// That is what lets the four rows be `static`s reached as `&'static dyn`, with no `Arc`, no reference count and no
// lifetime shorter than the process.
//
// Once per contract rather than once per model: every difference between Tokyo, Kyoto and Saitama is data in their
// rows — so a unit struct per model would be three near-identical bodies delegating to one family-tier function, which
// is the copying `crate::models`' tier rule warns against. The diffusion contract that was the second implementor,
// Osaka's, moved to the restore family with Osaka.
/// What the family asks of a model, and the only thing [`UpscaleVariant::model`] hands back.
///
/// Implemented **once per contract**: [`ConvVariant`](super::conv::ConvVariant) in [`conv`](super::conv).
pub(crate) trait UpscaleModel: Send + Sync {
    /// The developer-facing identifier every artifact name for this model is composed from.
    fn codename(&self) -> &'static str;

    /// The display text a user sees, with no precision or scale appended.
    fn label(&self) -> &'static str;

    // Asked of the model rather than decided beside it, which is what keeps a chooser from offering a build that was
    // never published the moment a second model of either contract publishes a different pair.
    /// Every precision this model is published in, in the order a chooser should offer them.
    fn precisions(&self) -> &'static [Precision];

    // Asked of the model for the reason `precisions` is, and the reason `SimpleVariant` carries the same thing on its
    // row: what a model takes is a property of that model. A family-wide constant would give an upscale model taking
    // something other than a scale a silently wrong catalogue entry rather than a compile error.
    /// What a request for this model carries, as the catalogue publishes it.
    fn parameters(&self) -> &'static [ParameterEntry];

    /// The execution-provider tuning measured for this model at `precision`.
    fn profile(&self, precision: Precision) -> EpProfile;

    /// The tensor shape this model's passes run at, or `None` where this model runs no pass sequence.
    ///
    /// [`Upscale::pipeline`](super::Upscale::pipeline) relies on it being `Some` for every model that resolves to
    /// passes, which is every model this family holds today.
    fn pass_shape(&self) -> Option<PassShape>;

    /// The variant naming this row at `precision`, or `None` where this row publishes nothing at it.
    ///
    /// The inverse of [`UpscaleVariant::model`]. Each contract narrows `precision` to its own set here, which is the
    /// same fact [`precisions`](Self::precisions) reports: answering both from the row is what keeps them agreeing.
    fn variant(&self, precision: Precision) -> Option<UpscaleVariant>;

    /// What a request for `scale` at `precision` resolves to: the native passes covering it.
    fn resolve(&self, precision: Precision, scale: Scale) -> Resolution;
}

// The precision inside the variant rather than beside it as one shared enum plus a per-variant check.
/// Which upscale model an operation runs, and at which precision.
///
/// The precision travels **inside** the variant, in the domain that variant is actually published in. Tokyo paired
/// with INT8 is then not a runtime error — it does not compile, and the artifact `up_tokyo_4x_int8` has no path that
/// could name it.
///
/// **Osaka is not here.** It redraws detail rather than estimating it, so it is a
/// [`RestoreVariant`](crate::RestoreVariant), and a front end that offered it as an upscale offers it as a restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "codename", content = "precision", rename_all = "lowercase")]
pub enum UpscaleVariant {
    /// Tokyo: convolutional, native 4x weights only.
    Tokyo(FloatPrecision),
    /// Kyoto: convolutional, native 2x and 4x weights.
    Kyoto(FloatPrecision),
    /// Saitama: convolutional, native 4x weights only.
    Saitama(FloatPrecision),
}

impl UpscaleVariant {
    /// The precision this variant was asked for, widened to the common currency artifact names are composed from.
    pub fn precision(self) -> Precision {
        match self {
            Self::Tokyo(precision) | Self::Kyoto(precision) | Self::Saitama(precision) => precision.into(),
        }
    }

    /// The model this variant runs — **seam 1**, and one of the two matches a model is named in.
    pub(crate) fn model(self) -> &'static dyn UpscaleModel {
        // One arm per model, exhaustive with no wildcard: a variant added to the enum without a directory does not
        // compile. And everything a row answers is a required field of the row rather than an arm someone could
        // forget, so a model written without a profile does not compile either.
        match self {
            Self::Tokyo(_) => &TOKYO,
            Self::Kyoto(_) => &KYOTO,
            Self::Saitama(_) => &SAITAMA,
        }
    }

    /// The execution-provider tuning measured for this variant, at the precision it carries.
    pub(crate) fn profile(self) -> EpProfile {
        // Asked of the model, which is what names the graph a measurement was made against; the precision it was
        // measured at travels inside the variant and is widened on the way through.
        self.model().profile(self.precision())
    }

    /// The display text a user sees for this variant, with no precision or scale appended.
    pub(crate) fn label(self) -> &'static str {
        self.model().label()
    }

    /// The developer-facing identifier every artifact name for this variant is composed from.
    pub(crate) fn codename(self) -> &'static str {
        self.model().codename()
    }

    /// The variant this family publishes under `codename` at `precision`, or `None` where it publishes none.
    ///
    /// The inverse of the codename each variant reports: a front end that filled a chooser from [`fn@crate::catalogue`]
    /// hands the codename and the precision it was given straight back and gets the typed variant, restating none of
    /// this vocabulary in code of its own.
    ///
    /// A published model at a precision it is not published in names nothing: the three convolutional variants at
    /// INT8.
    pub fn from_codename(codename: &str, precision: Precision) -> Option<Self> {
        // Over `MODELS` — the same list `crate::catalogue` builds the forward direction from — so the two directions
        // cannot disagree about which models exist. A second, hand-written list of the arms, left out of date, would
        // produce a model that resolves and runs perfectly and cannot be built from the codename a chooser offers.
        MODELS
            .iter()
            .find(|model| model.codename() == codename)
            .and_then(|model| model.variant(precision))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Every operation the tests here and in the sibling modules build, one per variant and contract.
    pub(crate) fn every_variant() -> Vec<UpscaleVariant> {
        // Derived from `MODELS` crossed with each row's own published precisions, rather than listed by hand. A
        // model added to `MODELS` and forgotten here would leave every test driven off this list silently covering
        // one model fewer — including the ones that exist to catch exactly that kind of omission elsewhere.
        MODELS
            .iter()
            .flat_map(|model| model.precisions().iter().filter_map(|precision| model.variant(*precision)))
            .collect()
    }

    #[test]
    fn every_variant_reports_its_label_and_codename() {
        assert_eq!(UpscaleVariant::Tokyo(FloatPrecision::Fp32).label(), "Tokyo");
        assert_eq!(UpscaleVariant::Kyoto(FloatPrecision::Fp32).label(), "Kyoto");
        assert_eq!(UpscaleVariant::Saitama(FloatPrecision::Fp32).label(), "Saitama");

        for variant in every_variant() {
            assert_eq!(variant.codename(), variant.label().to_lowercase(), "{variant:?}");
        }
    }

    #[test]
    fn every_variant_reports_the_precision_it_was_built_with() {
        assert_eq!(UpscaleVariant::Kyoto(FloatPrecision::Fp32).precision(), Precision::Fp32);
        assert_eq!(UpscaleVariant::Tokyo(FloatPrecision::Fp16).precision(), Precision::Fp16);
        assert_eq!(UpscaleVariant::Saitama(FloatPrecision::Fp32).precision(), Precision::Fp32);
    }

    #[test]
    fn every_published_pairing_is_reachable_from_its_codename() {
        // Driven from `every_variant` — every pairing the family publishes, built from the two precision contracts
        // themselves — so a variant added to either contract and left out of `from_codename` fails here.
        for variant in every_variant() {
            assert_eq!(
                UpscaleVariant::from_codename(variant.codename(), variant.precision()),
                Some(variant),
                "{variant:?} did not round-trip through the codename and precision it reports"
            );
        }
    }

    #[test]
    fn a_pairing_this_family_does_not_publish_names_nothing() {
        // Osaka moved to restore, so its codename names nothing here at any precision — and a front end still
        // holding it as an upscale is refused rather than handed some other model.
        for precision in [Precision::Fp16, Precision::Int8] {
            assert_eq!(UpscaleVariant::from_codename("osaka", precision), None, "Osaka is not an upscaler");
        }

        for row in [TOKYO, KYOTO, SAITAMA] {
            assert_eq!(
                UpscaleVariant::from_codename(row.codename, Precision::Int8),
                None,
                "{} named an INT8 build the convolutional contract does not publish",
                row.codename
            );
        }

        assert_eq!(
            UpscaleVariant::from_codename("stockholm", Precision::Fp32),
            None,
            "a denoise codename named an upscale variant"
        );
        assert_eq!(UpscaleVariant::from_codename("", Precision::Fp32), None);
        assert_eq!(
            UpscaleVariant::from_codename(&TOKYO.codename.to_uppercase(), Precision::Fp32),
            None,
            "a codename was matched by something other than the spelling that is published"
        );
    }

    #[test]
    fn no_variant_beyond_the_measured_arms_declares_anything() {
        // Driven over every variant this family publishes, so a profile nothing measured cannot arrive by accident.
        let measured = [
            UpscaleVariant::Tokyo(FloatPrecision::Fp16),
            UpscaleVariant::Tokyo(FloatPrecision::Fp32),
            UpscaleVariant::Kyoto(FloatPrecision::Fp16),
            UpscaleVariant::Saitama(FloatPrecision::Fp16),
        ];

        for variant in every_variant() {
            if measured.contains(&variant) {
                continue;
            }

            assert_eq!(variant.profile(), EpProfile::default(), "{variant:?} declared a profile nothing measured");
        }
    }
}
