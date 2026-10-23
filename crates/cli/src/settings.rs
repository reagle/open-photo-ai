//! Where each setting of a run comes from: a flag, a variable, the GUI's saved settings, or the built-in default.
//!
//! The GUI publishes the preferences this command can follow into `opai::settings`'s file (see `opai::settings`). It is
//! read once, by [`crate::args::parse`], unless `--no-gui-settings` is given, and ranks below every flag and variable:
//!
//! ```text
//! processor          -p                    > GUI's processor              > auto
//! quality, a family  -q (every family)     > tier of the GUI's model      > hd
//! autopilot model    OPAI_AP_* (autopilot) > GUI's model, or off          > the bare flag's model
//! ```
//!
//! Each value keeps its [`Source`], which is what `--verbose` names on the processor line. Nothing here fails a run:
//! a file the library could not read is no file, and a processor this build or this machine cannot offer is `auto`.

use std::path::PathBuf;

use opai::Family;
use opai::settings::{Loaded, SharedSettings};

use crate::args::{Processor, Quality};

/// Where a value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A command-line flag, as the user would type it: `-q`, `-p`.
    Flag(&'static str),
    /// An environment variable, by name.
    Variable(&'static str),
    /// The GUI's saved settings.
    Gui,
    /// The built-in default.
    Default,
}

impl Source {
    /// How `--verbose` names it, inside parentheses: `from -q`, `from OPAI_AP_COLOR`, `from GUI settings`, `default`.
    pub fn describe(self) -> String {
        match self {
            Self::Flag(name) | Self::Variable(name) => format!("from {name}"),
            Self::Gui => "from GUI settings".to_owned(),
            Self::Default => "default".to_owned(),
        }
    }
}

/// A value and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sourced<T> {
    pub value: T,
    pub source: Source,
}

/// The quality each family runs at, in the library's apply order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Qualities([Sourced<Quality>; 7]);

impl Default for Qualities {
    fn default() -> Self {
        Self::all(Quality::Hd, Source::Default)
    }
}

impl Qualities {
    /// `quality` for every family, from `source`.
    pub const fn all(quality: Quality, source: Source) -> Self {
        Self([Sourced { value: quality, source }; 7])
    }

    /// Each family at `-q` where it was given, or else at the tier of the model `gui` saved for it, or else `hd`.
    fn resolve(flag: Option<Quality>, gui: Option<&SharedSettings>) -> Self {
        if let Some(quality) = flag {
            return Self::all(quality, Source::Flag("-q"));
        }

        Self(Family::APPLY_ORDER.map(|family| {
            gui.and_then(|gui| gui.model(family)).and_then(|choice| Quality::at(choice.position)).map_or(
                Sourced { value: Quality::Hd, source: Source::Default },
                |value| Sourced { value, source: Source::Gui },
            )
        }))
    }

    /// The quality `family` runs at; `hd` for detection, which runs at the precision the library prescribes anyway.
    pub fn of(&self, family: Family) -> Quality {
        family.applied_at().map_or(Quality::Hd, |index| self.0[index].value)
    }

    /// The one quality every family runs at, or `None` where they differ.
    pub fn uniform(&self) -> Option<Quality> {
        let first = self.0[0].value;
        self.0.iter().all(|quality| quality.value == first).then_some(first)
    }

    /// How the processor line names it: `HD`, `SD`, or `mixed` where the families differ.
    pub fn label(&self) -> &'static str {
        self.uniform().map_or("mixed", Quality::label)
    }

    /// How `--verbose` says where it came from, after [`label`](Self::label): `(from GUI settings)`, or for a mixed
    /// quality `(Denoise SD, Upscale SD from GUI settings; the rest HD by default)`.
    pub fn describe(&self) -> String {
        let first = self.0[0];
        if self.0.iter().all(|quality| *quality == first) {
            return format!("({})", first.source.describe());
        }

        let pairs = || Family::APPLY_ORDER.into_iter().zip(self.0);
        let from_gui: Vec<String> = pairs()
            .filter(|(_, quality)| quality.source == Source::Gui)
            .map(|(family, quality)| format!("{} {}", crate::look::family_label(family), quality.value.label()))
            .collect();
        let defaults: Vec<Quality> = pairs()
            .filter(|(_, quality)| quality.source != Source::Gui)
            .map(|(_, quality)| quality.value)
            .collect();

        // A quality differs between families only where the GUI chose some of them: `-q` sets all seven.
        match defaults.first() {
            None => format!("({} from GUI settings)", from_gui.join(", ")),
            Some(rest) => format!("({} from GUI settings; the rest {} by default)", from_gui.join(", "), rest.label()),
        }
    }
}

/// What became of the GUI's settings file for this run, as the log records it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum GuiFile {
    /// `--no-gui-settings` was given, or the arguments were parsed without reading it (as the tests do).
    #[default]
    NotRead,
    /// Its location could not be determined.
    Unlocated(String),
    /// There was none.
    Absent(PathBuf),
    /// It was not read, and why. The library has recorded a warning already.
    Ignored(PathBuf, String),
    /// It was read.
    Read(PathBuf),
}

/// The processor and the qualities a run uses, and where each came from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub processor: Sourced<Processor>,
    pub qualities: Qualities,
    pub gui_file: GuiFile,
}

impl Default for Sourced<Processor> {
    fn default() -> Self {
        Self { value: Processor::Auto, source: Source::Default }
    }
}

impl Settings {
    /// The settings `-q` and `-p` ask for, over what `gui` saved, over the defaults.
    pub fn resolve(quality: Option<Quality>, processor: Option<Processor>, gui: Option<&SharedSettings>) -> Self {
        let processor = match processor {
            Some(value) => Sourced { value, source: Source::Flag("-p") },
            None => gui.and_then(|gui| gui.processor).map_or_else(Sourced::default, |provider| {
                Processor::from_provider(provider).map_or_else(
                    || {
                        tracing::warn!(
                            "the GUI's saved processor {provider} is not one this build can run on; using auto"
                        );
                        Sourced::default()
                    },
                    |value| Sourced { value, source: Source::Gui },
                )
            }),
        };

        Self { processor, qualities: Qualities::resolve(quality, gui), gui_file: GuiFile::NotRead }
    }
}

/// The GUI's saved settings, read through `read` from the file `locate` names, and what became of the file.
pub fn load(
    locate: impl FnOnce() -> Result<PathBuf, String>,
    read: impl FnOnce(&std::path::Path) -> Loaded,
) -> (Option<SharedSettings>, GuiFile) {
    let path = match locate() {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!("the GUI's settings could not be located; running without them: {error}");
            return (None, GuiFile::Unlocated(error));
        }
    };

    match read(&path) {
        // Recorded by `run` once the run has started, beside the processor it settled on; the library has already
        // recorded why an ignored file was ignored.
        Loaded::Absent => (None, GuiFile::Absent(path)),
        Loaded::Ignored(reason) => (None, GuiFile::Ignored(path, reason)),
        Loaded::Read(settings) => (Some(settings), GuiFile::Read(path)),
    }
}

#[cfg(test)]
mod tests {
    use opai::ExecutionProvider;

    use super::*;

    /// Settings saved by a user who chose SD for denoise and upscale, CPU, and switched colour balance off.
    fn saved() -> SharedSettings {
        SharedSettings::from_gui(
            "cpu",
            ["color_balance"],
            [("denoise", "stockholm_fp16"), ("upscale", "osaka_int8"), ("sharpen", "moscow_fp32")],
        )
    }

    /// The same, at SD for every family.
    fn all_sd() -> SharedSettings {
        SharedSettings::from_gui(
            "auto",
            [],
            [
                ("denoise", "stockholm_fp16"),
                ("face_recovery", "athens_fp16"),
                ("colorization", "delhi_fp16"),
                ("light_adjustment", "paris_fp16"),
                ("color_balance", "rio_fp16"),
                ("sharpen", "moscow_fp16"),
                ("upscale", "tokyo_fp16"),
            ],
        )
    }

    #[test]
    fn without_a_file_everything_is_the_default() {
        let settings = Settings::resolve(None, None, None);

        assert_eq!(settings.processor, Sourced { value: Processor::Auto, source: Source::Default });
        assert_eq!(settings.qualities, Qualities::all(Quality::Hd, Source::Default));
        assert_eq!(settings.qualities.label(), "HD");
        assert_eq!(settings.qualities.describe(), "(default)");
    }

    #[test]
    fn the_file_is_followed_where_no_flag_is_given() {
        let settings = Settings::resolve(None, None, Some(&saved()));

        assert_eq!(settings.processor, Sourced { value: Processor::Cpu, source: Source::Gui });
        assert_eq!(settings.qualities.of(Family::Denoise), Quality::Sd);
        assert_eq!(settings.qualities.of(Family::Upscale), Quality::Sd, "Osaka's INT8 is its second build");
        assert_eq!(settings.qualities.of(Family::Sharpen), Quality::Hd, "Moscow's FP32 is its first");
        assert_eq!(settings.qualities.of(Family::LightAdjustment), Quality::Hd, "never chosen");
        assert_eq!(settings.qualities.label(), "mixed");
        assert_eq!(
            settings.qualities.describe(),
            "(Denoise SD, Sharpen HD, Upscale SD from GUI settings; the rest HD by default)"
        );
    }

    #[test]
    fn every_family_at_sd_in_the_file_reads_as_sd() {
        let settings = Settings::resolve(None, None, Some(&all_sd()));

        assert_eq!(settings.qualities.label(), "SD");
        assert_eq!(settings.qualities.describe(), "(from GUI settings)");
        assert_eq!(settings.processor, Sourced { value: Processor::Auto, source: Source::Gui });
    }

    #[test]
    fn a_flag_overrides_the_file_for_every_family() {
        let settings = Settings::resolve(Some(Quality::Hd), Some(Processor::Auto), Some(&all_sd()));

        assert_eq!(settings.qualities, Qualities::all(Quality::Hd, Source::Flag("-q")));
        assert_eq!(settings.qualities.describe(), "(from -q)");
        assert_eq!(settings.processor, Sourced { value: Processor::Auto, source: Source::Flag("-p") });
    }

    #[test]
    fn a_processor_this_build_cannot_name_is_auto() {
        // Whichever of CoreML or CUDA this OS's build has no value for.
        let foreign = if cfg!(target_os = "macos") { ExecutionProvider::Cuda } else { ExecutionProvider::CoreMl };
        let gui = SharedSettings { processor: Some(foreign), ..SharedSettings::default() };

        assert_eq!(Settings::resolve(None, None, Some(&gui)).processor, Sourced::default());
    }

    #[test]
    fn the_sources_are_worded_for_the_processor_line() {
        assert_eq!(Source::Flag("-q").describe(), "from -q");
        assert_eq!(Source::Variable("OPAI_AP_COLOR").describe(), "from OPAI_AP_COLOR");
        assert_eq!(Source::Gui.describe(), "from GUI settings");
        assert_eq!(Source::Default.describe(), "default");
    }

    #[test]
    fn loading_reports_what_became_of_the_file() {
        let path = PathBuf::from("/nowhere/gui-settings.json");

        let (settings, file) = load(|| Ok(path.clone()), |_| Loaded::Read(saved()));
        assert_eq!((settings, file), (Some(saved()), GuiFile::Read(path.clone())));

        let (settings, file) = load(|| Ok(path.clone()), |_| Loaded::Absent);
        assert_eq!((settings, file), (None, GuiFile::Absent(path.clone())));

        let (settings, file) = load(|| Ok(path.clone()), |_| Loaded::Ignored("version 2".to_owned()));
        assert_eq!((settings, file), (None, GuiFile::Ignored(path, "version 2".to_owned())));

        let (settings, file) = load(|| Err("no home".to_owned()), |_| unreachable!("nothing to read"));
        assert_eq!((settings, file), (None, GuiFile::Unlocated("no home".to_owned())));
    }
}
