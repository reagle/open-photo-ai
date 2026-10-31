//! The flags, or what autopilot chose, turned into the chain the library runs.
//!
//! The terminal's own vocabulary — `hd`, `sd`, `--up`, `--dn-str` and the rest — mapped onto the rows of
//! the library's catalogue, the same way the GUI and perf build theirs. The scale ladder is the library's
//! [`opai::suggested_scale`], and a model's builds and parameter defaults are the ones its catalogue row publishes,
//! none restated.

use opai::{
    Bias, FNumber, FOCUS_X, FOCUS_Y, Family, GrainAmount, GrainMode, Operation, ParameterKind, ParameterValues, Scale,
    Strength, Subject, Suggestion, VariantEntry,
};

use crate::args::{Quality, Selection};
use crate::autopilot::{AutopilotModels, CatalogueModel};
use crate::settings::Qualities;

/// What a run will do, and what the user should be told about it before it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The chain, in [`Family::APPLY_ORDER`]. The library puts it into that order anyway, so the flags' order cannot
    /// matter.
    pub operations: Vec<Operation>,
    /// What each operation is, in words, one per operation and in the same order: built from the same values, so the
    /// two cannot disagree.
    pub applied: Vec<Applied>,
    /// Lines to show the user about what was planned, such as an upscale left out.
    pub notes: Vec<String>,
}

/// One operation of a chain as the user is told about it: `Denoise Stockholm strength 100`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// Its family, worded as the row's other wordings word it.
    pub family: Family,
    /// The model's name as the GUI shows it, the catalogue's label: `Malmö`, `São Paulo`.
    pub model: &'static str,
    /// The parameter it runs at: `strength <n>`, `bias <n>` or `<factor>x`, or nothing for a family that takes none.
    pub detail: Option<String>,
}

/// `applied` in words: each `<family> <model>[ <detail>]`, joined by `, `, or `no enhancement needed` for an empty
/// chain.
pub fn describe(applied: &[Applied]) -> String {
    if applied.is_empty() {
        return "no enhancement needed".to_owned();
    }

    let words: Vec<String> = applied
        .iter()
        .map(|applied| {
            let family = crate::look::family_label(applied.family);
            match &applied.detail {
                Some(detail) => format!("{family} {} {detail}", applied.model),
                None => format!("{family} {}", applied.model),
            }
        })
        .collect();
    words.join(", ")
}

impl Selection {
    /// What autopilot chose for one photograph: each family in `suggestions` with its model from `models`, at the
    /// catalogue's default parameters, and an upscale at the scale the analysis read off the photograph.
    pub fn from_suggestions(suggestions: &[Suggestion], models: &AutopilotModels) -> Self {
        let mut selection = Self::default();

        for suggestion in suggestions {
            match *suggestion {
                Suggestion::Denoise => selection.denoise = models.model_for(),
                Suggestion::FaceRecovery => selection.face_recovery = models.model_for(),
                Suggestion::Colorization => selection.colorization = models.model_for(),
                Suggestion::LightAdjustment => selection.light_adjustment = models.model_for(),
                Suggestion::ColorBalance => selection.color_balance = models.model_for(),
                Suggestion::Sharpen => selection.sharpen = models.model_for(),
                Suggestion::Upscale { scale } => {
                    selection.upscale = models.model_for();
                    selection.scale = Some(scale);
                }
            }
        }

        selection
    }
}

/// The parameter the command line sets on a model, if any, and so what its description names.
enum Parameter {
    /// Face recovery and colorization: nothing set, nothing named.
    None,
    /// A strength, as a percentage given on the command line, or the published default.
    Strength(Option<u16>),
    /// A bias, as a percentage given on the command line, or the published default.
    Bias(Option<i16>),
    /// The factor an upscale runs at.
    Scale(Scale),
    /// A restore: the size it runs at where one was given, else the published 1x, and its strength.
    Restore { scale: Option<Scale>, strength: Option<u16> },
    /// A lens blur: the f-number and the focus point where they were given, else the published defaults. Both are
    /// already in the library's range, so they are set as they are.
    LensBlur { f_number: Option<f64>, focus: Option<(f64, f64)> },
    /// A grain: its amount as a percentage where one was given, else the published default, and whether it is added
    /// rather than matched to the photograph.
    Grain { amount: Option<u16>, add: bool },
}

/// The chain `selection` asks for, each family at its quality in `qualities`, over a `width` x `height` photograph.
///
/// An upscale with no scale runs at the factor the photograph needs, and is left out with a note where it needs
/// none. A restore with no size restores at the photograph's own. Every face recovery restores nobody until the faces
/// are found and handed to it, and every lens blur blurs nothing until the depth is estimated and handed to it.
pub fn plan(selection: &Selection, qualities: &Qualities, width: u32, height: u32) -> Plan {
    let mut built = Vec::new();
    let mut notes = Vec::new();

    if let Some(model) = selection.denoise {
        built.push(build(model, qualities, &mut notes, Parameter::Strength(selection.denoise_strength)));
    }

    if let Some(model) = selection.face_recovery {
        // Maximum fidelity: as close to the original face as the model goes, which is what the catalogue publishes.
        built.push(build(model, qualities, &mut notes, Parameter::None));
    }

    if let Some(model) = selection.colorization {
        built.push(build(model, qualities, &mut notes, Parameter::None));
    }

    if let Some(model) = selection.light_adjustment {
        built.push(build(model, qualities, &mut notes, Parameter::Bias(selection.light_bias)));
    }

    if let Some(model) = selection.color_balance {
        built.push(build(model, qualities, &mut notes, Parameter::Bias(selection.balance_bias)));
    }

    if let Some(model) = selection.sharpen {
        built.push(build(model, qualities, &mut notes, Parameter::Strength(selection.sharpen_strength)));
    }

    if let Some(model) = selection.upscale {
        match selection.scale.or_else(|| opai::suggested_scale(width, height)) {
            Some(scale) => built.push(build(model, qualities, &mut notes, Parameter::Scale(scale))),
            None => notes.push(format!(
                "the upscale was skipped: the photograph is already large enough ({width}x{height}); \
                 pass --scale to upscale it anyway"
            )),
        }
    }

    // After everything faithful, as the library runs it. The size is the catalogue's default, 1x, unless one was
    // given; `parse_from` has already refused one above 1x beside an upscale.
    if let Some(model) = selection.restore {
        built.push(build(
            model,
            qualities,
            &mut notes,
            Parameter::Restore { scale: selection.restore_scale, strength: selection.restore_strength },
        ));
    }

    // Last, as the library runs it. The f-number and the focus are the catalogue's defaults unless given; both are
    // already in range, so they are set as they are rather than as percentages.
    if let Some(model) = selection.lens_blur {
        let (blur, mut applied) = build(
            model,
            qualities,
            &mut notes,
            Parameter::LensBlur { f_number: selection.lens_blur_f, focus: selection.lens_blur_focus },
        );

        // Not a catalogue parameter: the edges are a choice between two renders, which no control draws.
        let blur = match (blur, selection.lens_blur_edges) {
            (Operation::LensBlur(blur), Some(edges)) => Operation::LensBlur(blur.with_edges(edges.into())),
            (blur, _) => blur,
        };
        if matches!(&blur, Operation::LensBlur(blur) if blur.edges() == opai::LensBlurEdges::Depth) {
            applied.detail = applied.detail.map(|detail| format!("{detail}, edges from the depth"));
        }
        built.push((blur, applied));
    }

    // After everything, the lens blur included, as the library runs it: the photograph's own grain at the catalogue's
    // default amount, all of it, unless an amount or adding was asked for.
    if let Some(model) = selection.grain {
        built.push(build(
            model,
            qualities,
            &mut notes,
            Parameter::Grain { amount: selection.grain_amount, add: selection.grain_add },
        ));
    }

    let (operations, applied) = built.into_iter().unzip();
    Plan { operations, applied, notes }
}

/// The operation `model` names at its family's quality in `qualities`, built by the library's catalogue at the parameters that model's row
/// publishes as defaults with `parameter` set over them, and its description, from the values it was built with.
///
/// `quality` picks a build by its position in the row's precisions, so a model published at other precisions than
/// FP32 and FP16 — Osaka, at FP16 and INT8 — needs no case of its own.
///
/// A build this install cannot run — a model this fork exports and has not placed at that precision — is swapped for
/// the one it can, with a line in `notes` saying so; see [`served_precision`].
fn build<M: CatalogueModel>(
    model: M,
    qualities: &Qualities,
    notes: &mut Vec<String>,
    parameter: Parameter,
) -> (Operation, Applied) {
    let quality: Quality = qualities.of(M::FAMILY);
    let name = model.to_possible_value().expect("no model is skipped");
    let variant =
        VariantEntry::published(M::FAMILY, name.get_name()).expect("every model the command line names is published");
    let served = served()
        .iter()
        .filter(|entry| entry.family == M::FAMILY)
        .flat_map(|entry| entry.variants.iter())
        .find(|served| served.codename == variant.codename)
        .unwrap_or(variant);
    let (precision, note) = served_precision(served, quality);
    notes.extend(note);
    let precision = &precision;

    let mut values = ParameterValues::new();
    for parameter in variant.parameters {
        if let ParameterKind::Range { default, .. } = parameter.kind {
            values.set(parameter.name, default).expect("a published parameter is known by its name");
        }
    }

    let detail = match parameter {
        Parameter::None => None,
        Parameter::Strength(given) => {
            let percent = set(&mut values, variant, Strength::NAME, given.map(f64::from));
            Some(format!("strength {percent}"))
        }
        Parameter::Bias(given) => {
            let percent = set(&mut values, variant, Bias::NAME, given.map(f64::from));
            Some(format!("bias {percent}"))
        }
        Parameter::Scale(scale) => {
            values = values.with_scale(scale);
            // `Scale` renders without a trailing `.0`: `2`, `1.5`.
            Some(format!("{scale}x"))
        }
        Parameter::Restore { scale, strength } => {
            if let Some(scale) = scale {
                values = values.with_scale(scale);
            }
            let percent = set(&mut values, variant, Strength::NAME, strength.map(f64::from));
            Some(match scale {
                Some(scale) => format!("{scale}x, strength {percent}"),
                None => format!("strength {percent}"),
            })
        }
        Parameter::LensBlur { f_number, focus } => {
            if let Some(f_number) = f_number {
                values.set(FNumber::NAME, f_number).expect("a published parameter is known by its name");
            }
            if let Some((x, y)) = focus {
                values.set(FOCUS_X, x).expect("a published parameter is known by its name");
                values.set(FOCUS_Y, y).expect("a published parameter is known by its name");
            }
            let f_number = f_number.unwrap_or_else(|| published_default(variant, FNumber::NAME));
            let (x, y) =
                focus.unwrap_or_else(|| (published_default(variant, FOCUS_X), published_default(variant, FOCUS_Y)));
            // `f64` renders without a trailing `.0`: `f/2`, `f/2.8`.
            Some(format!("f/{f_number}, focus {x},{y}"))
        }
        Parameter::Grain { amount, add } => {
            let percent = set(&mut values, variant, GrainAmount::NAME, amount.map(f64::from));
            if add {
                values
                    .set(GrainMode::NAME, GrainMode::Add.as_switch())
                    .expect("the mode is published by its name");
            }
            Some(format!("amount {percent}, {}", if add { "added" } else { "matching the photograph" }))
        }
    };

    let operation = match variant.build(*precision, &values) {
        Ok(Subject::Enhancement(operation)) => operation,
        other => panic!("{:?} {} did not build an enhancement: {other:?}", M::FAMILY, variant.codename),
    };
    (operation, Applied { family: M::FAMILY, model: variant.label, detail })
}

// Fork-only, not for upstream: every model upstream names is served at every precision it is published at.
/// The catalogue as this install can serve it, read once per process: see [`opai::installable_catalogue`].
///
/// The library's own catalogue under test, so what the suite plans does not depend on the models placed on the machine
/// running it.
fn served() -> &'static [opai::FamilyEntry] {
    #[cfg(test)]
    {
        opai::catalogue()
    }

    #[cfg(not(test))]
    {
        static SERVED: std::sync::OnceLock<Vec<opai::FamilyEntry>> = std::sync::OnceLock::new();

        SERVED.get_or_init(|| opai::installable_catalogue(opai::APP_NAME, opai::ModelTrust::Published))
    }
}

/// The precision `variant` runs at for `quality` — the build at that position in its precisions, or its first — and,
/// where this install cannot serve that build, the nearest one it can, with the note saying so.
///
/// A fallback rather than a refusal, as the GUI's saved default falls back: `-q hd` is every family's quality at once,
/// and Kazan having only its SD build here is no reason to fail a chain that also denoises. Where no build at all can be
/// served the asked-for one is kept, and the run reports the install's refusal, naming the model.
fn served_precision(variant: &VariantEntry, quality: Quality) -> (opai::Precision, Option<String>) {
    let asked = *variant.precisions.get(quality.position()).unwrap_or(&variant.precisions[0]);

    match variant.nearest_available(asked) {
        Some(served) if served != asked => {
            let (label, tier, asked) = (variant.label, tier(variant, served), spelled(asked));
            (
                served,
                Some(format!(
                    "{label} runs at {tier}: its {} build ({asked}) isn't available on this install",
                    quality.label()
                )),
            )
        }
        _ => (asked, None),
    }
}

/// How the notes name `precision`'s place among `variant`'s builds: `HD`, `SD`, or the precision itself past the two
/// the command line names.
fn tier(variant: &VariantEntry, precision: opai::Precision) -> String {
    match variant.precisions.iter().position(|candidate| *candidate == precision) {
        Some(0) => Quality::Hd.label().to_string(),
        Some(1) => Quality::Sd.label().to_string(),
        _ => spelled(precision),
    }
}

/// `precision` as the notes write it: `FP32`.
fn spelled(precision: opai::Precision) -> String {
    format!("{precision:?}").to_uppercase()
}

/// The default `variant` publishes for the range parameter `name`.
fn published_default(variant: &VariantEntry, name: &str) -> f64 {
    variant
        .parameters
        .iter()
        .find(|parameter| parameter.name == name)
        .and_then(|parameter| match parameter.kind {
            ParameterKind::Range { default, .. } => Some(default),
            _ => None,
        })
        .expect("a model taking a range parameter publishes its default")
}

/// Sets `name` to `percent` of the model's own effect, where one was given, and answers the percentage it runs at:
/// the one given, or the default `variant` publishes for it.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a published strength or bias is a few hundred percent at most"
)]
fn set(values: &mut ParameterValues, variant: &VariantEntry, name: &str, percent: Option<f64>) -> i64 {
    let fraction = match percent {
        Some(percent) => {
            let fraction = percent / 100.0;
            // Every whole percentage the parser accepts is a whole number of the library's 0.001 steps, inside its
            // range.
            values.set(name, fraction).expect("a published parameter is known by its name");
            fraction
        }
        None => variant
            .parameters
            .iter()
            .find(|parameter| parameter.name == name)
            .and_then(|parameter| match parameter.kind {
                ParameterKind::Range { default, .. } => Some(default),
                _ => None,
            })
            .expect("a model taking a strength or a bias publishes its default"),
    };

    (fraction * 100.0).round() as i64
}

#[cfg(test)]
mod tests {
    use opai::{
        ColorBalance, Colorization, Denoise, FaceRecovery, Faces, Family, Fidelity, FloatPrecision, LightAdjustment,
        OsakaPrecision, Restore, Scale, Sharpen, Upscale,
    };
    use opai::{Fraction, LensBlur};

    use super::*;
    use crate::args::{Args, parse_from};
    use crate::settings::Source;

    fn args(line: &str) -> Args {
        parse_from(line.split_whitespace()).unwrap_or_else(|error| panic!("{line}: {error}"))
    }

    /// The chain the command line `line` asks for over a `width` x `height` photograph.
    fn plan(line: &str, width: u32, height: u32) -> Plan {
        let args = args(line);
        super::plan(&args.selection, args.qualities(), width, height)
    }

    /// Stockholm as an install serves it with `unavailable` missing from it.
    fn stockholm_without(unavailable: &[opai::Precision]) -> VariantEntry {
        let mut variant = VariantEntry::published(Family::Denoise, "stockholm").unwrap().clone();
        variant.unavailable = unavailable.to_vec();
        variant
    }

    #[test]
    fn a_build_this_install_cannot_serve_falls_back_to_the_one_it_can_and_says_so() {
        // Kazan HD where only Kazan SD was placed, here as Stockholm: `-q hd`, or the default, runs SD rather than
        // failing the chain on a model nothing hosts.
        let (precision, note) = served_precision(&stockholm_without(&[opai::Precision::Fp32]), Quality::Hd);

        assert_eq!(precision, opai::Precision::Fp16);
        assert_eq!(
            note.as_deref(),
            Some("Stockholm runs at SD: its HD build (FP32) isn't available on this install")
        );
    }

    #[test]
    fn a_build_this_install_serves_runs_as_asked_with_nothing_said() {
        let served = stockholm_without(&[opai::Precision::Fp32]);

        assert_eq!(served_precision(&served, Quality::Sd), (opai::Precision::Fp16, None));
        assert_eq!(served_precision(&stockholm_without(&[]), Quality::Hd), (opai::Precision::Fp32, None));
    }

    #[test]
    fn a_model_with_no_build_this_install_serves_keeps_the_one_asked_for_so_the_run_can_say_why() {
        let served = stockholm_without(&[opai::Precision::Fp32, opai::Precision::Fp16]);

        assert_eq!(served_precision(&served, Quality::Hd), (opai::Precision::Fp32, None));
    }

    fn scale(factor: f64) -> Scale {
        Scale::new(factor).expect("in range")
    }

    fn f_number(value: f64) -> FNumber {
        FNumber::new(value).expect("in range")
    }

    fn fraction(value: f64) -> Fraction {
        Fraction::new(value).expect("in range")
    }

    #[test]
    fn a_lens_blur_runs_last_at_the_f_number_and_focus_asked_for_and_otherwise_at_the_catalogues() {
        assert_eq!(
            plan("opai photo.jpg --lb -o out.jpg", 4000, 3000).operations,
            vec![LensBlur::lisbon(f_number(2.8), Fraction::CENTRE, Fraction::CENTRE)]
        );
        assert_eq!(
            plan("opai photo.jpg --lens-blur --lb-f f/1.4 --lb-focus 0.3,0.6 -o out.jpg", 4000, 3000).operations,
            vec![LensBlur::lisbon(f_number(1.4), fraction(0.3), fraction(0.6))]
        );

        // Matted unless the depth alone is asked for.
        let Operation::LensBlur(depth_only) =
            plan("opai photo.jpg --lb --lb-edges depth -o out.jpg", 4000, 3000).operations.remove(0)
        else {
            panic!("not a lens blur");
        };
        assert_eq!(depth_only.edges(), opai::LensBlurEdges::Depth);
        let Operation::LensBlur(matted) =
            plan("opai photo.jpg --lb --lb-edges matte -o out.jpg", 4000, 3000).operations.remove(0)
        else {
            panic!("not a lens blur");
        };
        assert_eq!(matted.edges(), opai::LensBlurEdges::Matte);
        let Operation::LensBlur(solid) =
            plan("opai photo.jpg --lb --lb-edges solid -o out.jpg", 4000, 3000).operations.remove(0)
        else {
            panic!("not a lens blur");
        };
        assert_eq!(solid.edges(), opai::LensBlurEdges::Solid);

        // After an upscale, whatever order the flags came in.
        assert_eq!(
            plan("opai photo.jpg --lb --lb-f 2 --up -o out.jpg", 1000, 800).operations,
            vec![
                Upscale::tokyo(FloatPrecision::Fp32, scale(4.0)),
                LensBlur::lisbon(f_number(2.0), Fraction::CENTRE, Fraction::CENTRE)
            ]
        );
    }

    fn strength(value: f64) -> Strength {
        Strength::new(value).expect("in range")
    }

    fn bias(value: f64) -> Bias {
        Bias::new(value).expect("in range")
    }

    #[test]
    fn every_row_of_the_quality_table() {
        let four = scale(4.0);

        let rows = [
            ("--up kyoto -q hd", Upscale::kyoto(FloatPrecision::Fp32, four)),
            ("--up kyoto -q sd", Upscale::kyoto(FloatPrecision::Fp16, four)),
            ("--up tokyo -q hd", Upscale::tokyo(FloatPrecision::Fp32, four)),
            ("--up tokyo -q sd", Upscale::tokyo(FloatPrecision::Fp16, four)),
            ("--up saitama -q hd", Upscale::saitama(FloatPrecision::Fp32, four)),
            ("--up saitama -q sd", Upscale::saitama(FloatPrecision::Fp16, four)),
            // Lisbon is published at FP16 alone, so both qualities run it.
            ("--lb lisbon -q hd", LensBlur::lisbon(f_number(2.8), Fraction::CENTRE, Fraction::CENTRE)),
            ("--lb lisbon -q sd", LensBlur::lisbon(f_number(2.8), Fraction::CENTRE, Fraction::CENTRE)),
            (
                "--fr athens -q hd",
                FaceRecovery::athens(FloatPrecision::Fp32, Faces::empty(), Fidelity::MAXIMUM),
            ),
            (
                "--fr athens -q sd",
                FaceRecovery::athens(FloatPrecision::Fp16, Faces::empty(), Fidelity::MAXIMUM),
            ),
            ("--fr santorini -q hd", FaceRecovery::santorini(FloatPrecision::Fp32, Faces::empty())),
            ("--fr santorini -q sd", FaceRecovery::santorini(FloatPrecision::Fp16, Faces::empty())),
        ];
        let one = strength(1.0);
        let half = bias(0.5);
        let rows = rows.into_iter().chain([
            ("--dn stockholm -q hd", Denoise::stockholm(FloatPrecision::Fp32, one)),
            ("--dn stockholm -q sd", Denoise::stockholm(FloatPrecision::Fp16, one)),
            ("--dn gothenburg -q hd", Denoise::gothenburg(FloatPrecision::Fp32, one)),
            ("--dn gothenburg -q sd", Denoise::gothenburg(FloatPrecision::Fp16, one)),
            ("--dn malmo -q hd", Denoise::malmo(FloatPrecision::Fp32, one)),
            ("--dn malmo -q sd", Denoise::malmo(FloatPrecision::Fp16, one)),
            ("--sh moscow -q hd", Sharpen::moscow(FloatPrecision::Fp32, one)),
            ("--sh moscow -q sd", Sharpen::moscow(FloatPrecision::Fp16, one)),
            ("--sh petersburg -q hd", Sharpen::stpetersburg(FloatPrecision::Fp32, one)),
            ("--sh petersburg -q sd", Sharpen::stpetersburg(FloatPrecision::Fp16, one)),
            ("--sh novgorod -q hd", Sharpen::novgorod(FloatPrecision::Fp32, one)),
            ("--sh novgorod -q sd", Sharpen::novgorod(FloatPrecision::Fp16, one)),
            ("--rs kazan -q hd", Restore::kazan(FloatPrecision::Fp32, scale(1.0), one)),
            ("--rs kazan -q sd", Restore::kazan(FloatPrecision::Fp16, scale(1.0), one)),
            ("--rs osaka -q hd", Restore::osaka(OsakaPrecision::Fp16, scale(1.0), one)),
            ("--rs osaka -q sd", Restore::osaka(OsakaPrecision::Int8, scale(1.0), one)),
            ("--cl delhi -q hd", Colorization::delhi(FloatPrecision::Fp32)),
            ("--cl delhi -q sd", Colorization::delhi(FloatPrecision::Fp16)),
            ("--cl mumbai -q hd", Colorization::mumbai(FloatPrecision::Fp32)),
            ("--cl mumbai -q sd", Colorization::mumbai(FloatPrecision::Fp16)),
            ("--cl jaipur -q hd", Colorization::jaipur(FloatPrecision::Fp32)),
            ("--cl jaipur -q sd", Colorization::jaipur(FloatPrecision::Fp16)),
            ("--la paris -q hd", LightAdjustment::paris(FloatPrecision::Fp32, half)),
            ("--la paris -q sd", LightAdjustment::paris(FloatPrecision::Fp16, half)),
            ("--la lyon -q hd", LightAdjustment::lyon(FloatPrecision::Fp32, half)),
            ("--la lyon -q sd", LightAdjustment::lyon(FloatPrecision::Fp16, half)),
            ("--cb rio -q hd", ColorBalance::rio(FloatPrecision::Fp32, half)),
            ("--cb rio -q sd", ColorBalance::rio(FloatPrecision::Fp16, half)),
            ("--cb saopaulo -q hd", ColorBalance::saopaulo(FloatPrecision::Fp32, half)),
            ("--cb saopaulo -q sd", ColorBalance::saopaulo(FloatPrecision::Fp16, half)),
        ]);

        for (flags, expected) in rows {
            let plan = plan(&format!("opai photo.jpg {flags} -o out.png"), 1000, 800);
            assert_eq!(plan.operations, vec![expected], "{flags}");
        }
    }

    #[test]
    fn the_default_quality_runs_tokyo_at_fp32() {
        let plan = plan("opai photo.jpg --up -o out.png", 1000, 800);

        assert_eq!(plan.operations, vec![Upscale::tokyo(FloatPrecision::Fp32, scale(4.0))]);
    }

    #[test]
    fn the_scale_is_what_the_photograph_needs() {
        let up = "opai photo.jpg --up -o out.png";

        let small = plan(up, 1000, 800);
        assert_eq!(small.operations, vec![Upscale::tokyo(FloatPrecision::Fp32, scale(4.0))]);
        assert!(small.notes.is_empty());

        let medium = plan(up, 2000, 1500);
        assert_eq!(medium.operations, vec![Upscale::tokyo(FloatPrecision::Fp32, scale(2.0))]);
        assert!(medium.notes.is_empty());

        let large = plan(up, 4000, 3000);
        assert!(large.operations.is_empty(), "a 12 MP photograph was upscaled: {:?}", large.operations);
        assert_eq!(large.notes.len(), 1);
        assert!(large.notes[0].contains("already large enough"), "{:?}", large.notes);
    }

    #[test]
    fn a_skipped_upscale_leaves_the_face_recovery() {
        let plan = plan("opai photo.jpg --up --fr -o out.png", 4000, 3000);

        assert_eq!(
            plan.operations,
            vec![FaceRecovery::athens(FloatPrecision::Fp32, Faces::empty(), Fidelity::MAXIMUM)]
        );
        assert_eq!(plan.notes.len(), 1);
    }

    #[test]
    fn an_explicit_scale_is_always_honoured() {
        let plan = plan("opai photo.jpg --up --scale 2 -o out.png", 4000, 3000);

        assert_eq!(plan.operations, vec![Upscale::tokyo(FloatPrecision::Fp32, scale(2.0))]);
        assert!(plan.notes.is_empty());
    }

    #[test]
    fn a_restore_runs_at_the_size_and_strength_asked_for_and_otherwise_at_the_photographs_own() {
        assert_eq!(
            plan("opai photo.jpg --rs -o out.png", 4000, 3000).operations,
            vec![Restore::kazan(FloatPrecision::Fp32, scale(1.0), strength(1.0))]
        );
        assert_eq!(
            plan("opai photo.jpg --rs --rs-scale 2 --rs-str 40 -q sd -o out.png", 4000, 3000).operations,
            vec![Restore::kazan(FloatPrecision::Fp16, scale(2.0), strength(0.4))]
        );

        // After an upscale, at 1x: the restore is last, at whatever size the upscale left the photograph.
        assert_eq!(
            plan("opai photo.jpg --rs --up -o out.png", 1000, 800).operations,
            vec![
                Upscale::tokyo(FloatPrecision::Fp32, scale(4.0)),
                Restore::kazan(FloatPrecision::Fp32, scale(1.0), strength(1.0))
            ]
        );
    }

    #[test]
    fn each_parameter_is_a_percentage_of_its_own_enhancement() {
        let plan = plan("opai photo.jpg --dn --dn-str 200 --sh --sh-str 80 --la --la-bias -20 --cb -o x", 1, 1);

        assert_eq!(
            plan.operations,
            vec![
                Denoise::stockholm(FloatPrecision::Fp32, strength(2.0)),
                LightAdjustment::paris(FloatPrecision::Fp32, bias(-0.2)),
                ColorBalance::rio(FloatPrecision::Fp32, bias(0.5)),
                Sharpen::moscow(FloatPrecision::Fp32, strength(0.8)),
            ]
        );
    }

    #[test]
    fn every_accepted_percentage_builds() {
        for percent in 1..=300 {
            let line = format!("opai photo.jpg --dn --dn-str {percent} -o out.png");
            assert_eq!(plan(&line, 1, 1).operations.len(), 1, "{line}");
        }
        for percent in -100..=100 {
            let line = format!("opai photo.jpg --la --la-bias {percent} -o out.png");
            assert_eq!(plan(&line, 1, 1).operations.len(), 1, "{line}");
        }
    }

    #[test]
    fn the_flags_order_does_not_change_the_chain() {
        let up_first = plan("opai photo.jpg --up saitama --fr santorini -q sd -o out.png", 1000, 800);
        let fr_first = plan("opai photo.jpg --fr santorini --up saitama -q sd -o out.png", 1000, 800);

        assert_eq!(up_first, fr_first);
        assert_eq!(up_first.operations.len(), 2);

        let up_first = plan("opai photo.jpg --up --sh --dn -o out.png", 1000, 800);
        let dn_first = plan("opai photo.jpg --dn --sh --up -o out.png", 1000, 800);

        assert_eq!(up_first, dn_first);
        assert_eq!(
            up_first.operations,
            vec![
                Denoise::stockholm(FloatPrecision::Fp32, strength(1.0)),
                Sharpen::moscow(FloatPrecision::Fp32, strength(1.0)),
                Upscale::tokyo(FloatPrecision::Fp32, scale(4.0)),
            ]
        );
    }

    #[test]
    fn a_grain_matches_the_photograph_at_the_full_amount_unless_asked_otherwise() {
        use opai::{Grain, GrainAmount, GrainMode};

        let amount = |value| GrainAmount::new(value).expect("in range");
        let cases = [
            ("--gr", Grain::sensor(amount(1.0), GrainMode::Match)),
            ("--gr --gr-amount 40", Grain::sensor(amount(0.4), GrainMode::Match)),
            ("--gr --gr-add --gr-amount 25", Grain::sensor(amount(0.25), GrainMode::Add)),
            ("--gr --gr-add", Grain::sensor(amount(1.0), GrainMode::Add)),
        ];

        for (flags, expected) in cases {
            for quality in ["hd", "sd"] {
                let planned = plan(&format!("opai photo.jpg {flags} -q {quality} -o out.png"), 1000, 800);
                assert_eq!(planned.operations, vec![expected.clone()], "{flags} -q {quality}");
            }
        }
    }

    #[test]
    fn the_chain_is_pushed_in_the_librarys_apply_order() {
        let plan = plan("opai photo.jpg --gr --lb --rs --up --sh --cb --la --cl --fr --dn -o out.png", 1000, 800);
        let families: Vec<_> = plan.operations.iter().map(Operation::family).collect();

        // Every family the enhancement flags name; a removal is added from `--remove`'s mask, by the run.
        assert_eq!(families, Family::APPLY_ORDER[1..]);
    }

    /// The chain autopilot plans for `suggested` with `models` at `quality` over a `width` x `height` photograph.
    fn autopilot(
        suggested: &[Suggestion],
        models: &AutopilotModels,
        quality: Quality,
        width: u32,
        height: u32,
    ) -> Plan {
        super::plan(
            &Selection::from_suggestions(suggested, models),
            &Qualities::all(quality, Source::Default),
            width,
            height,
        )
    }

    #[test]
    fn each_suggestion_with_the_default_models_plans_what_its_bare_flag_does() {
        let defaults = AutopilotModels::default();
        let cases = [
            (Suggestion::Denoise, "--dn"),
            (Suggestion::FaceRecovery, "--fr"),
            (Suggestion::Colorization, "--cl"),
            (Suggestion::LightAdjustment, "--la"),
            (Suggestion::ColorBalance, "--cb"),
            (Suggestion::Sharpen, "--sh"),
            (Suggestion::Upscale { scale: scale(4.0) }, "--up"),
        ];

        for quality in ["hd", "sd"] {
            let parsed = args(&format!("opai photo.jpg --up -q {quality} -o out.png"))
                .qualities()
                .uniform()
                .expect("one quality");
            for (suggestion, flag) in cases {
                let expected = plan(&format!("opai photo.jpg {flag} -q {quality} -o out.png"), 1000, 800);

                assert_eq!(autopilot(&[suggestion], &defaults, parsed, 1000, 800), expected, "{flag} -q {quality}");
            }
        }
    }

    #[test]
    fn each_family_runs_at_its_own_quality_from_the_guis_settings() {
        let gui = opai::settings::SharedSettings::from_gui("auto", [], [("upscale", "tokyo_fp16")]);
        let mut args = args("opai photo.jpg --dn --up --scale 2 -o out.png");
        args.settings = crate::settings::Settings::resolve(args.quality, args.processor, Some(&gui));

        let plan = super::plan(&args.selection, args.qualities(), 1000, 800);

        assert_eq!(
            plan.operations,
            vec![
                Denoise::stockholm(FloatPrecision::Fp32, strength(1.0)),
                Upscale::tokyo(FloatPrecision::Fp16, scale(2.0)),
            ]
        );
    }

    #[test]
    fn a_model_chosen_through_the_environment_runs_at_the_quality_asked_for() {
        let models =
            AutopilotModels::read(|name| (name == "OPAI_AP_UPSCALE").then(|| "kyoto".into()), None).expect("reads");

        let plan = autopilot(&[Suggestion::Upscale { scale: scale(4.0) }], &models, Quality::Sd, 1000, 800);

        assert_eq!(plan.operations, vec![Upscale::kyoto(FloatPrecision::Fp16, scale(4.0))]);
    }

    #[test]
    fn the_suggested_scale_is_used_whatever_the_photographs_size() {
        let suggested = [Suggestion::Upscale { scale: scale(2.0) }];

        let plan = autopilot(&suggested, &AutopilotModels::default(), Quality::Hd, 4000, 3000);

        assert_eq!(plan.operations, vec![Upscale::tokyo(FloatPrecision::Fp32, scale(2.0))]);
        assert!(plan.notes.is_empty(), "{:?}", plan.notes);
    }

    #[test]
    fn several_suggestions_plan_in_the_librarys_apply_order() {
        let suggested = [Suggestion::Denoise, Suggestion::ColorBalance, Suggestion::Upscale { scale: scale(4.0) }];

        let plan = autopilot(&suggested, &AutopilotModels::default(), Quality::Hd, 1000, 800);

        assert_eq!(
            plan.operations,
            vec![
                Denoise::stockholm(FloatPrecision::Fp32, strength(1.0)),
                ColorBalance::rio(FloatPrecision::Fp32, bias(0.5)),
                Upscale::tokyo(FloatPrecision::Fp32, scale(4.0)),
            ]
        );
    }

    #[test]
    fn no_suggestion_plans_an_empty_chain() {
        let plan = autopilot(&[], &AutopilotModels::default(), Quality::Hd, 1000, 800);

        assert_eq!(plan, Plan { operations: Vec::new(), applied: Vec::new(), notes: Vec::new() });
        assert_eq!(describe(&plan.applied), "no enhancement needed");
    }

    /// The words the command line `line` gives a 1000 x 800 photograph.
    fn words(line: &str) -> String {
        describe(&plan(line, 1000, 800).applied)
    }

    #[test]
    fn every_familys_bare_flag_is_named_with_its_default_model_and_parameter() {
        let cases = [
            ("--dn", "Denoise Stockholm strength 100"),
            ("--fr", "Face Recovery Athens"),
            ("--cl", "Colorize Delhi"),
            ("--la", "Light Adjust Paris bias 50"),
            ("--cb", "Color Balance Rio bias 50"),
            ("--sh", "Sharpen Moscow strength 100"),
            ("--up --scale 2", "Upscale Tokyo 2x"),
            ("--gr", "Grain Sensor amount 100, matching the photograph"),
            ("--gr --gr-add --gr-amount 30", "Grain Sensor amount 30, added"),
        ];

        for (flags, expected) in cases {
            assert_eq!(words(&format!("opai photo.jpg {flags} -o out.png")), expected, "{flags}");
        }
    }

    #[test]
    fn the_description_follows_the_chain_one_for_one_in_its_order() {
        let plan = plan("opai photo.jpg --gr --lb --rs --up --sh --cb --la --cl --fr --dn -o out.png", 1000, 800);
        let families: Vec<_> = plan.applied.iter().map(|applied| applied.family).collect();

        // Every family but the removal, which is `--rm`'s own and never planned from the flags.
        assert_eq!(families, Family::APPLY_ORDER[1..]);
        assert_eq!(
            describe(&plan.applied),
            "Denoise Stockholm strength 100, Face Recovery Athens, Colorize Delhi, Light Adjust Paris bias 50, \
             Color Balance Rio bias 50, Sharpen Moscow strength 100, Upscale Tokyo 4x, Restore Kazan strength 100, \
             Lens Blur Lisbon f/2.8, focus 0.5,0.5, Grain Sensor amount 100, matching the photograph"
        );
    }

    #[test]
    fn given_parameters_and_named_models_are_named_as_they_ran() {
        assert_eq!(
            words("opai photo.jpg --cb SaoPaulo --cb-bias -20 --sh --sh-str 80 -o out.png"),
            "Color Balance São Paulo bias -20, Sharpen Moscow strength 80"
        );
        assert_eq!(
            words("opai photo.jpg --dn malmo --dn-str 200 --sh petersburg -o out.png"),
            "Denoise Malmö strength 200, Sharpen Petersburg strength 100"
        );
        assert_eq!(words("opai photo.jpg --la lyon --la-bias -100 -o out.png"), "Light Adjust Lyon bias -100");
    }

    #[test]
    fn a_factor_is_written_without_a_whole_numbers_fraction() {
        assert_eq!(words("opai photo.jpg --up kyoto --scale 1.5 -o out.png"), "Upscale Kyoto 1.5x");
        assert_eq!(words("opai photo.jpg --up saitama --scale 8 -o out.png"), "Upscale Saitama 8x");
        assert_eq!(words("opai photo.jpg --rs --rs-scale 2 -o out.png"), "Restore Kazan 2x, strength 100");
        assert_eq!(words("opai photo.jpg --rs osaka --rs-str 40 -o out.png"), "Restore Osaka strength 40");
    }

    #[test]
    fn a_lens_blur_is_named_by_its_f_number_and_focus_point() {
        assert_eq!(words("opai photo.jpg --lb -o out.png"), "Lens Blur Lisbon f/2.8, focus 0.5,0.5");
        assert_eq!(
            words("opai photo.jpg --lb --lb-f f/2 --lb-focus 0.55,0.3 -o out.png"),
            "Lens Blur Lisbon f/2, focus 0.55,0.3"
        );
    }

    #[test]
    fn an_auto_chosen_scale_is_named_as_the_operation_runs_it() {
        let medium = plan("opai photo.jpg --up -o out.png", 2000, 1500);

        assert_eq!(medium.operations, vec![Upscale::tokyo(FloatPrecision::Fp32, scale(2.0))]);
        assert_eq!(describe(&medium.applied), "Upscale Tokyo 2x");
    }

    #[test]
    fn a_skipped_upscale_names_no_enhancement() {
        let large = plan("opai photo.jpg --up -o out.png", 4000, 3000);

        assert!(large.applied.is_empty());
        assert_eq!(describe(&large.applied), "no enhancement needed");
    }

    #[test]
    fn autopilots_choice_is_worded_as_the_equivalent_flags_are() {
        let suggested = [Suggestion::Denoise, Suggestion::ColorBalance, Suggestion::Upscale { scale: scale(4.0) }];

        let plan = autopilot(&suggested, &AutopilotModels::default(), Quality::Hd, 1000, 800);

        assert_eq!(describe(&plan.applied), words("opai photo.jpg --dn --cb --up --scale 4 -o out.png"));
        assert_eq!(
            describe(&plan.applied),
            "Denoise Stockholm strength 100, Color Balance Rio bias 50, Upscale Tokyo 4x"
        );
    }
}
