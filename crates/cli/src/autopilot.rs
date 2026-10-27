//! Which model autopilot runs each family with: what the family's `OPAI_AP_*` variable names, or else what the GUI's
//! saved settings name for it, or else the bare flag's default — or nothing at all where the variable says `off` or,
//! with no variable, the GUI's settings say Autopilot may not suggest the family.
//!
//! Read once, at startup and only under `--autopilot`, through a lookup the caller hands in, so the tests read a map
//! and nothing here touches the process environment. Only this front end reads these variables: the GUI answers the
//! same question from its own settings, and publishes them for this one (see `crate::settings`).

use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{CommandFactory, ValueEnum};
use opai::Family;
use opai::settings::SharedSettings;

use crate::args::{
    Args, BalanceModel, ColorizationModel, DenoiseModel, FaceModel, LensBlurModel, LightModel, RestoreModel,
    SharpenModel, UpscaleModel,
};
use crate::settings::Source;

/// The word that leaves a family out of the analysis. No model is named `off`, so it is free.
const OFF: &str = "off";

/// A family's model vocabulary, as the command line names it: what [`plan`](crate::chain::plan) builds an operation
/// from.
pub trait CatalogueModel: ValueEnum + Copy {
    /// The family these models belong to.
    const FAMILY: Family;
}

/// A family's model vocabulary, as autopilot reads it from the environment.
pub trait FamilyModel: CatalogueModel {
    /// The environment variable that chooses one.
    const VARIABLE: &'static str;
    /// The model the family's flag runs when given with no model, and autopilot's default.
    const DEFAULT: Self;

    /// This family's model in `models`, or `None` where it is switched off.
    fn choice(models: &AutopilotModels) -> Option<Self>;
}

macro_rules! family_model {
    ($model:ty, $family:expr, $variable:literal, $default:expr, $field:ident) => {
        impl CatalogueModel for $model {
            const FAMILY: Family = $family;
        }

        impl FamilyModel for $model {
            const VARIABLE: &'static str = $variable;
            const DEFAULT: Self = $default;

            fn choice(models: &AutopilotModels) -> Option<Self> {
                models.$field
            }
        }
    };
}

// The defaults are the bare flags' `default_missing_value`s, which clap does not expose for reading back;
// `every_default_is_the_bare_flags` holds the two together.
family_model!(DenoiseModel, Family::Denoise, "OPAI_AP_DENOISE", DenoiseModel::Stockholm, denoise);
family_model!(FaceModel, Family::FaceRecovery, "OPAI_AP_FACE", FaceModel::Athens, face_recovery);
family_model!(
    ColorizationModel,
    Family::Colorization,
    "OPAI_AP_COLORIZATION",
    ColorizationModel::Delhi,
    colorization
);
family_model!(LightModel, Family::LightAdjustment, "OPAI_AP_LIGHT", LightModel::Paris, light_adjustment);
family_model!(BalanceModel, Family::ColorBalance, "OPAI_AP_COLOR", BalanceModel::Rio, color_balance);
family_model!(SharpenModel, Family::Sharpen, "OPAI_AP_SHARPEN", SharpenModel::Moscow, sharpen);
family_model!(UpscaleModel, Family::Upscale, "OPAI_AP_UPSCALE", UpscaleModel::Tokyo, upscale);

// A command-line family only. An analysis never suggests a generative redraw, so autopilot has no model to choose for
// it and there is no `OPAI_AP_RESTORE`.
impl CatalogueModel for RestoreModel {
    const FAMILY: Family = Family::Restore;
}

// A command-line family only. An analysis never suggests a lens blur, a creative choice rather than a correction, so
// autopilot has no model to choose for it and there is no `OPAI_AP_LENS_BLUR`.
impl CatalogueModel for LensBlurModel {
    const FAMILY: Family = Family::LensBlur;
}

/// Autopilot's model for every family, in the library's apply order: `None` where the family is switched off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutopilotModels {
    pub denoise: Option<DenoiseModel>,
    pub face_recovery: Option<FaceModel>,
    pub colorization: Option<ColorizationModel>,
    pub light_adjustment: Option<LightModel>,
    pub color_balance: Option<BalanceModel>,
    pub sharpen: Option<SharpenModel>,
    pub upscale: Option<UpscaleModel>,
    /// Where each family's choice came from, in the library's apply order.
    pub sources: [Source; 7],
}

impl Default for AutopilotModels {
    fn default() -> Self {
        Self {
            denoise: Some(DenoiseModel::DEFAULT),
            face_recovery: Some(FaceModel::DEFAULT),
            colorization: Some(ColorizationModel::DEFAULT),
            light_adjustment: Some(LightModel::DEFAULT),
            color_balance: Some(BalanceModel::DEFAULT),
            sharpen: Some(SharpenModel::DEFAULT),
            upscale: Some(UpscaleModel::DEFAULT),
            sources: [Source::Default; 7],
        }
    }
}

impl AutopilotModels {
    /// The seven `OPAI_AP_*` variables, each read through `lookup`, over what `gui` saved for the family.
    ///
    /// # Errors
    ///
    /// A usage error, as a bad flag value is one, for the first variable that is neither empty, `off`, nor a model of
    /// its family, naming the variable and the value and listing what it accepts. Nothing the GUI saved is an error.
    pub fn read(lookup: impl Fn(&str) -> Option<OsString>, gui: Option<&SharedSettings>) -> Result<Self, clap::Error> {
        let (denoise, dn) = read_one(&lookup, gui)?;
        let (face_recovery, fr) = read_one(&lookup, gui)?;
        let (colorization, cl) = read_one(&lookup, gui)?;
        let (light_adjustment, la) = read_one(&lookup, gui)?;
        let (color_balance, cb) = read_one(&lookup, gui)?;
        let (sharpen, sh) = read_one(&lookup, gui)?;
        let (upscale, up) = read_one(&lookup, gui)?;

        Ok(Self {
            denoise,
            face_recovery,
            colorization,
            light_adjustment,
            color_balance,
            sharpen,
            upscale,
            sources: [dn, fr, cl, la, cb, sh, up],
        })
    }

    /// Each family in the library's apply order, with its model's display name or `None` where it is off, and where
    /// the choice came from, worded for `--verbose`.
    pub fn describe(&self) -> Vec<(Family, Option<&'static str>, String)> {
        fn one<M: FamilyModel>(models: &AutopilotModels) -> (Family, Option<&'static str>, String) {
            let label = M::choice(models).map(|model| {
                let name = model.to_possible_value().expect("no model is skipped");
                opai::VariantEntry::published(M::FAMILY, name.get_name()).map_or("?", |variant| variant.label)
            });
            (M::FAMILY, label, models.source::<M>().describe())
        }

        vec![
            one::<DenoiseModel>(self),
            one::<FaceModel>(self),
            one::<ColorizationModel>(self),
            one::<LightModel>(self),
            one::<BalanceModel>(self),
            one::<SharpenModel>(self),
            one::<UpscaleModel>(self),
        ]
    }

    /// Where the family `M`'s choice came from.
    pub fn source<M: FamilyModel>(&self) -> Source {
        // Counted from denoise, the first family an analysis suggests: a removal runs before it and has no source.
        let first = Family::Denoise.applied_at().unwrap_or_default();
        M::FAMILY
            .applied_at()
            .and_then(|index| index.checked_sub(first))
            .and_then(|index| self.sources.get(index).copied())
            .unwrap_or(Source::Default)
    }

    /// The families not switched off, in the library's apply order: what the analysis is asked to look for.
    pub fn families(&self) -> Vec<Family> {
        fn on<M: FamilyModel>(models: &AutopilotModels) -> Option<Family> {
            M::choice(models).map(|_| M::FAMILY)
        }

        [
            on::<DenoiseModel>(self),
            on::<FaceModel>(self),
            on::<ColorizationModel>(self),
            on::<LightModel>(self),
            on::<BalanceModel>(self),
            on::<SharpenModel>(self),
            on::<UpscaleModel>(self),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The model the family `M` runs with, the bare flag's where the variable left the default, or `None` where the
    /// family is switched off.
    pub fn model_for<M: FamilyModel>(&self) -> Option<M> {
        M::choice(self)
    }
}

/// The model `M`'s variable chooses, read through `lookup`, and where the choice came from: `None` where it says
/// `off`; where it is unset or empty, `None` where `gui` excludes the family or the model `gui` saved, and the bare
/// flag's where `gui` says neither.
fn read_one<M: FamilyModel>(
    lookup: impl Fn(&str) -> Option<OsString>,
    gui: Option<&SharedSettings>,
) -> Result<(Option<M>, Source), clap::Error> {
    let Some(value) = lookup(M::VARIABLE).filter(|value| !value.is_empty()) else {
        return Ok(from_gui::<M>(gui));
    };
    let source = Source::Variable(M::VARIABLE);

    let text = value.to_str();
    if text.is_some_and(|text| text.eq_ignore_ascii_case(OFF)) {
        return Ok((None, source));
    }

    text.and_then(|text| M::from_str(text, true).ok())
        .filter(visible)
        .map(|model| (Some(model), source))
        .ok_or_else(|| {
            let accepted: Vec<String> = M::value_variants()
                .iter()
                .filter_map(ValueEnum::to_possible_value)
                .filter(|value| !value.is_hide_set())
                .map(|value| value.get_name().to_owned())
                .chain([OFF.to_owned()])
                .collect();

            Args::command().error(
                ErrorKind::InvalidValue,
                format!(
                    "invalid value '{}' for the environment variable {}\n  [possible values: {}]",
                    value.to_string_lossy(),
                    M::VARIABLE,
                    accepted.join(", ")
                ),
            )
        })
}

/// A hidden value is a retired model — Osaka, which moved to restore — and is refused like one never published, from
/// a list that does not name it, or, from what the GUI saved, passed over for the bare flag's default.
fn visible<M: FamilyModel>(model: &M) -> bool {
    model.to_possible_value().is_some_and(|value| !value.is_hide_set())
}

/// What `gui` saved for the family `M`: off where Autopilot may not suggest it, its model where one resolves to a model
/// this command names, and otherwise the bare flag's default.
fn from_gui<M: FamilyModel>(gui: Option<&SharedSettings>) -> (Option<M>, Source) {
    let Some(gui) = gui else { return (Some(M::DEFAULT), Source::Default) };

    if gui.excludes(M::FAMILY) {
        return (None, Source::Gui);
    }

    // The command line names every published model by its codename, which `every_published_model_is_named` holds.
    gui.model(M::FAMILY)
        .and_then(|choice| M::from_str(choice.codename, true).ok())
        .filter(visible)
        .map_or((Some(M::DEFAULT), Source::Default), |model| (Some(model), Source::Gui))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Reads the variables from `pairs` alone.
    fn read(pairs: &[(&str, &str)]) -> Result<AutopilotModels, clap::Error> {
        read_over(pairs, None)
    }

    /// Reads the variables from `pairs`, over `gui`.
    fn read_over(pairs: &[(&str, &str)], gui: Option<&SharedSettings>) -> Result<AutopilotModels, clap::Error> {
        let map: HashMap<String, OsString> =
            pairs.iter().map(|(name, value)| ((*name).to_owned(), OsString::from(value))).collect();

        AutopilotModels::read(|name| map.get(name).cloned(), gui)
    }

    /// What the GUI saves for a user who switched colour balance and colorization off and chose Saitama and Malmö.
    fn saved() -> SharedSettings {
        SharedSettings::from_gui(
            "auto",
            ["color_balance", "colorization"],
            [("upscale", "saitama_fp16"), ("denoise", "malmo_fp16")],
        )
    }

    #[test]
    fn the_guis_settings_choose_where_no_variable_does() {
        let models = read_over(&[], Some(&saved())).expect("reads");

        assert_eq!(models.color_balance, None);
        assert_eq!(models.colorization, None);
        assert_eq!(models.source::<BalanceModel>(), Source::Gui);
        assert_eq!(models.model_for(), Some(UpscaleModel::Saitama));
        assert_eq!(models.model_for(), Some(DenoiseModel::Malmo));
        assert_eq!(models.source::<UpscaleModel>(), Source::Gui);
        assert_eq!(models.model_for(), Some(SharpenModel::Moscow));
        assert_eq!(models.source::<SharpenModel>(), Source::Default);
        assert!(!models.families().contains(&Family::ColorBalance));
        assert!(!models.families().contains(&Family::Colorization));

        let described = models.describe();
        assert_eq!(described[0], (Family::Denoise, Some("Malmö"), "from GUI settings".to_owned()));
        assert_eq!(described[4], (Family::ColorBalance, None, "from GUI settings".to_owned()));
        assert_eq!(described[5], (Family::Sharpen, Some("Moscow"), "default".to_owned()));
    }

    #[test]
    fn a_variable_overrides_the_guis_settings() {
        let models = read_over(
            &[("OPAI_AP_COLOR", "saopaulo"), ("OPAI_AP_UPSCALE", "off"), ("OPAI_AP_DENOISE", "")],
            Some(&saved()),
        )
        .expect("reads");

        assert_eq!(models.model_for(), Some(BalanceModel::SaoPaulo));
        assert_eq!(models.source::<BalanceModel>(), Source::Variable("OPAI_AP_COLOR"));
        assert_eq!(models.model_for::<UpscaleModel>(), None);
        assert_eq!(models.source::<UpscaleModel>(), Source::Variable("OPAI_AP_UPSCALE"));
        assert_eq!(models.model_for(), Some(DenoiseModel::Malmo), "an empty variable leaves the GUI's choice");
        assert_eq!(models.colorization, None, "a family no variable names stays as the GUI saved it");
    }

    #[test]
    fn a_bad_variable_is_refused_whatever_the_gui_saved() {
        let error = read_over(&[("OPAI_AP_COLOR", "paris")], Some(&saved())).expect_err("paris is not a balance model");

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    /// Every model the catalogue publishes is named by this command, so a model the GUI saved is never dropped here.
    #[test]
    fn every_published_model_is_named() {
        fn check<M: FamilyModel>() {
            let entry = opai::catalogue().iter().find(|entry| entry.family == M::FAMILY).expect("published");
            for variant in &entry.variants {
                assert!(M::from_str(variant.codename, true).is_ok(), "{:?} {}", M::FAMILY, variant.codename);
            }
        }

        check::<DenoiseModel>();
        check::<FaceModel>();
        check::<ColorizationModel>();
        check::<LightModel>();
        check::<BalanceModel>();
        check::<SharpenModel>();
        check::<UpscaleModel>();
    }

    /// Every family an analysis can suggest, in the library's apply order: all of them but restore, which a generative
    /// redraw is never chosen automatically, lens blur, a creative choice an analysis never makes, and remove, whose
    /// region is the user's to paint.
    fn suggestible() -> Vec<Family> {
        Family::APPLY_ORDER
            .into_iter()
            .filter(|family| !matches!(family, Family::Remove | Family::Restore | Family::LensBlur))
            .collect()
    }

    #[test]
    fn no_variable_leaves_every_family_at_its_default() {
        let models = read(&[]).expect("reads");

        assert_eq!(models, AutopilotModels::default());
        assert_eq!(models.families(), suggestible());
        assert_eq!(models.model_for(), Some(DenoiseModel::Stockholm));
        assert_eq!(models.model_for(), Some(FaceModel::Athens));
        assert_eq!(models.model_for(), Some(ColorizationModel::Delhi));
        assert_eq!(models.model_for(), Some(LightModel::Paris));
        assert_eq!(models.model_for(), Some(BalanceModel::Rio));
        assert_eq!(models.model_for(), Some(SharpenModel::Moscow));
        assert_eq!(models.model_for(), Some(UpscaleModel::Tokyo));
    }

    #[test]
    fn a_named_model_is_taken_whatever_its_case() {
        let models = read(&[
            ("OPAI_AP_DENOISE", "MALMO"),
            ("OPAI_AP_FACE", "Santorini"),
            ("OPAI_AP_COLORIZATION", "jaiPUR"),
            ("OPAI_AP_LIGHT", "Lyon"),
            ("OPAI_AP_COLOR", "SaoPaulo"),
            ("OPAI_AP_SHARPEN", "petersburg"),
            ("OPAI_AP_UPSCALE", "Saitama"),
        ])
        .expect("reads");

        assert_eq!(models.model_for(), Some(DenoiseModel::Malmo));
        assert_eq!(models.model_for(), Some(FaceModel::Santorini));
        assert_eq!(models.model_for(), Some(ColorizationModel::Jaipur));
        assert_eq!(models.model_for(), Some(LightModel::Lyon));
        assert_eq!(models.model_for(), Some(BalanceModel::SaoPaulo));
        assert_eq!(models.model_for(), Some(SharpenModel::Petersburg));
        assert_eq!(models.model_for(), Some(UpscaleModel::Saitama));
        assert_eq!(models.families(), suggestible());
    }

    #[test]
    fn off_in_any_case_leaves_the_family_out() {
        let models = read(&[("OPAI_AP_SHARPEN", "off"), ("OPAI_AP_FACE", "OFF")]).expect("reads");

        assert_eq!(models.sharpen, None);
        assert_eq!(models.face_recovery, None);
        assert_eq!(models.model_for::<SharpenModel>(), None);
        assert_eq!(models.model_for::<FaceModel>(), None);
        assert_eq!(
            models.families(),
            [
                Family::Denoise,
                Family::Colorization,
                Family::LightAdjustment,
                Family::ColorBalance,
                Family::Upscale
            ]
        );
    }

    #[test]
    fn an_empty_value_is_the_default() {
        let models = read(&[("OPAI_AP_UPSCALE", ""), ("OPAI_AP_DENOISE", "")]).expect("reads");

        assert_eq!(models, AutopilotModels::default());
    }

    #[test]
    fn another_familys_model_is_refused_naming_the_variable_and_listing_the_accepted_ones() {
        let error = read(&[("OPAI_AP_UPSCALE", "paris")]).expect_err("paris is not an upscale model");

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
        assert_eq!(error.exit_code(), 2);
        let message = error.to_string();
        assert!(message.contains("OPAI_AP_UPSCALE") && message.contains("'paris'"), "{message}");
        assert!(message.contains("[possible values: kyoto, tokyo, saitama, off]"), "{message}");
    }

    #[test]
    fn osaka_is_refused_as_an_upscaler_now_that_it_restores() {
        // Autopilot never restores, so there is nothing for this variable to move to: it is refused, from a list that
        // does not offer it.
        let error = read(&[("OPAI_AP_UPSCALE", "osaka")]).expect_err("Osaka is not an upscaler");

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
        assert!(error.to_string().contains("[possible values: kyoto, tokyo, saitama, off]"), "{error}");
    }

    #[test]
    fn every_family_refuses_an_unknown_model_with_its_own_list() {
        for (variable, accepted) in [
            ("OPAI_AP_DENOISE", "stockholm, gothenburg, malmo, off"),
            ("OPAI_AP_FACE", "athens, santorini, off"),
            ("OPAI_AP_COLORIZATION", "delhi, mumbai, jaipur, off"),
            ("OPAI_AP_LIGHT", "paris, lyon, off"),
            ("OPAI_AP_COLOR", "rio, saopaulo, off"),
            ("OPAI_AP_SHARPEN", "moscow, petersburg, novgorod, off"),
        ] {
            let error = read(&[(variable, "tokyo")]).expect_err(variable);
            let message = error.to_string();

            assert!(message.contains(variable), "{message}");
            assert!(message.contains(&format!("[possible values: {accepted}]")), "{message}");
        }
    }

    #[test]
    fn every_default_is_the_bare_flags() {
        let bare = crate::args::parse_from("opai photo.jpg --dn --fr --cl --la --cb --sh --up -o out.png".split(' '))
            .expect("parses");

        assert_eq!(bare.selection.denoise, Some(DenoiseModel::DEFAULT));
        assert_eq!(bare.selection.face_recovery, Some(FaceModel::DEFAULT));
        assert_eq!(bare.selection.colorization, Some(ColorizationModel::DEFAULT));
        assert_eq!(bare.selection.light_adjustment, Some(LightModel::DEFAULT));
        assert_eq!(bare.selection.color_balance, Some(BalanceModel::DEFAULT));
        assert_eq!(bare.selection.sharpen, Some(SharpenModel::DEFAULT));
        assert_eq!(bare.selection.upscale, Some(UpscaleModel::DEFAULT));
    }

    /// Autopilot's default model is the one the library's catalogue lists first, which is what the GUI falls back to.
    #[test]
    fn every_default_is_the_catalogues_first_variant() {
        fn name(value: &impl ValueEnum) -> String {
            value.to_possible_value().expect("no variant is skipped").get_name().to_owned()
        }

        fn first(family: Family) -> &'static str {
            let entry = opai::catalogue().iter().find(|entry| entry.family == family).expect("published");
            entry.variants[0].codename
        }

        fn check<M: FamilyModel>(models: &AutopilotModels) {
            let model: M = models.model_for().expect("on by default");
            assert_eq!(name(&model), first(M::FAMILY), "{:?}", M::FAMILY);
        }

        let models = AutopilotModels::default();
        check::<DenoiseModel>(&models);
        check::<FaceModel>(&models);
        check::<ColorizationModel>(&models);
        check::<LightModel>(&models);
        check::<BalanceModel>(&models);
        check::<SharpenModel>(&models);
        check::<UpscaleModel>(&models);
    }
}
