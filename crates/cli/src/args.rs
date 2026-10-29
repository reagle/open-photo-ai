//! The command line: what `opai` accepts, its defaults, and the checks clap can make before anything is touched.

use std::ffi::OsString;
use std::path::PathBuf;
use std::str::FromStr;

use clap::parser::ValueSource;
use clap::{ArgAction, ArgMatches, CommandFactory, FromArgMatches, Parser, ValueEnum};
use opai::Scale;
use opai::export::{ExportFormat, FormatChoice};

use crate::autopilot::AutopilotModels;
use crate::settings::{self, Qualities, Settings};

/// Enhance photographs with Open Photo AI.
///
/// Each input is a photograph or a directory of them; a directory contributes the images directly inside it, or its
/// whole tree with `--recursive`. The enhancements run in one order whatever order the flags are given in: remove,
/// denoise, face recovery, colorization, light adjustment, colour balance, sharpen, upscale, restore, and then lens
/// blur.
///
/// The output is a file when its extension names a format opai writes and it is not an existing directory; it then
/// takes exactly one input photograph. Any other output is a directory, created if missing, that mirrors each
/// directory input. Without an output, each result is written beside its source, with the suffix `_opai` unless
/// `--prefix` or `--suffix` is given. An existing file is never overwritten unless `--overwrite` is given: the result
/// is numbered `_1`, `_2`, … instead.
#[derive(Debug, Parser)]
#[command(name = "opai", display_name = "Open Photo AI - CLI", version = opai::version(), disable_version_flag = true, disable_help_flag = true, max_term_width = 100, override_usage = "opai <INPUT>... [OPTIONS] [--output <OUTPUT>]\n       opai --gui <INPUT>... [-r]")]
pub struct Args {
    // Positional and first, so a bare enhancement flag is never handed an input as its model name. A user who writes
    // `opai --up photo.jpg` is told `photo.jpg` is not a model, which is clear and costs nothing.
    /// The photographs, and the directories of photographs, to enhance.
    #[arg(value_name = "INPUT", required = true, num_args = 1..)]
    pub inputs: Vec<PathBuf>,

    /// Analyse each photograph and apply the enhancements it calls for, each with its family's default model. Cannot
    /// be combined with the enhancement flags or their parameters.
    ///
    /// A family's model is chosen with its environment variable, set to one of that family's models or to `off`,
    /// which leaves the family out: OPAI_AP_DENOISE, OPAI_AP_FACE, OPAI_AP_COLORIZATION, OPAI_AP_LIGHT,
    /// OPAI_AP_COLOR, OPAI_AP_SHARPEN and OPAI_AP_UPSCALE.
    #[arg(
        help_heading = "Enhancements",
        long = "autopilot",
        visible_alias = "ap",
        conflicts_with = "Selection"
    )]
    pub autopilot: bool,

    /// The enhancements, and their parameters.
    #[command(flatten, next_help_heading = "Enhancements")]
    pub selection: Selection,

    // On `Args` rather than on `Selection`: it is not an enhancement, so it goes with `--autopilot` as well as with the
    // flags, and the refusal of a command with neither covers it already. Listed with the enhancements it limits.
    /// Limit the enhancements to the white part of a mask image: black keeps the photograph as it was and grey mixes
    /// the two. Its luminance is the weight and any transparency is ignored. It is resized to each photograph, whose
    /// aspect ratio must match it within 1%.
    #[arg(help_heading = "Enhancements", long, value_name = "FILE")]
    pub mask: Option<PathBuf>,

    /// What the mask is for under a lens blur: `both` limits the enhancements to it and keeps it in focus (the
    /// default), `enhancements` only limits them, `lens-blur` only keeps it in focus and applies every other
    /// enhancement to the whole photograph. With a lens blur the mask is blended before the blur, so what it leaves out
    /// is the photograph, blurred.
    #[arg(
        help_heading = "Enhancements",
        long = "mask-use",
        value_name = "USE",
        value_enum,
        ignore_case = true,
        requires = "mask"
    )]
    pub mask_use: Option<MaskUse>,

    // On `Args` rather than on `Selection`, as `--mask` is: a file does not fit the `Copy` value autopilot also builds,
    // and a removal goes with `--autopilot` as well as with the flags. It is an enhancement all the same, and counts as
    // one for the refusal of a command with none.
    /// Remove the white part of a mask image, filling it from the photograph around it, before any other enhancement:
    /// black is kept. Its luminance is read and any transparency is ignored. It is resized to each photograph, whose
    /// aspect ratio must match it within 1%.
    #[arg(help_heading = "Enhancements", long = "remove", visible_alias = "rm", value_name = "FILE")]
    pub remove: Option<PathBuf>,

    /// `hd` runs every model at its higher-fidelity build, `sd` at its lighter, faster one. Without it, each family
    /// runs at the quality of its model in the GUI's saved settings, or `hd`.
    #[arg(help_heading = "Processing", long, short = 'q', value_enum, ignore_case = true)]
    pub quality: Option<Quality>,

    /// The hardware to run on: `auto` (the best this machine has), or one processor by name. Without it, the GUI's
    /// saved processor, or `auto`. One this machine does not offer is refused once initialization has finished.
    #[arg(help_heading = "Processing", long, short = 'p', value_enum, ignore_case = true)]
    pub processor: Option<Processor>,

    /// Ignore the GUI's saved settings for this run: every setting not given here is the built-in default.
    #[arg(help_heading = "Processing", long)]
    pub no_gui_settings: bool,

    /// Take a directory input's whole tree, not only the images directly inside it.
    #[arg(help_heading = "Processing", long, short = 'r')]
    pub recursive: bool,

    /// The file or directory to write to. Without it, each result is written beside its source, named with the suffix
    /// `_opai` unless `--prefix` or `--suffix` is given.
    #[arg(help_heading = "Output", long, short = 'o', value_name = "OUTPUT")]
    pub output: Option<PathBuf>,

    /// The format every result is written in, unless the output is a file: `preserve` (each source's own, TIFF for
    /// camera RAW; the default), avif, bmp, gif, heic, jpeg, png, tiff or webp.
    #[arg(help_heading = "Output", long, value_name = "FORMAT", value_parser = parse_format)]
    pub format: Option<FormatChoice>,

    /// Written before each source's name, unless the output is a file.
    #[arg(help_heading = "Output", long, value_name = "TEXT", allow_hyphen_values = true)]
    pub prefix: Option<String>,

    /// Written after each source's name, before the extension, unless the output is a file. Without an output it
    /// defaults to `_opai`, unless `--prefix` is given; `--suffix ""` turns it off.
    #[arg(help_heading = "Output", long, value_name = "TEXT", allow_hyphen_values = true)]
    pub suffix: Option<String>,

    /// Replace a file already at a destination, once the new one is complete, instead of numbering the result.
    #[arg(help_heading = "Output", long)]
    pub overwrite: bool,

    /// Name, on each written image's line, the enhancements it received: each model, its strength, bias or upscale
    /// factor, and whether the run cache served them. Without it the line names only the path.
    #[arg(help_heading = "Output", long)]
    pub verbose: bool,

    /// Open the photographs in Open Photo AI instead of processing them. With --lens-blur, each opens with a Lens Blur
    /// at --lb-f, --lb-amount and --lb-focus and with Autopilot switched off; other options are ignored.
    #[arg(help_heading = "Options", long)]
    pub gui: bool,

    /// Send no usage or diagnostic data to the project for this run. Setting OPAI_ANALYTICS to 0, off, false or no
    /// does the same.
    #[arg(help_heading = "Options", long)]
    pub no_analytics: bool,

    // Declared here, not left to clap, so it sits in the same section as the other generic options.
    #[arg(
        help_heading = "Options",
        long,
        short = 'h',
        action = ArgAction::Help,
        help = "Print help (see more with '--help')",
        long_help = "Print help"
    )]
    help: (),

    /// Print version.
    #[arg(help_heading = "Options", long, short = 'v', action = ArgAction::Version)]
    version: (),

    /// The model autopilot runs each family with, read from the `OPAI_AP_*` variables by [`parse`] under
    /// `--autopilot` only, and `None` otherwise.
    #[arg(skip)]
    pub autopilot_models: Option<AutopilotModels>,

    /// The processor and the qualities the run uses, and where each came from: resolved by [`parse_from`] from the
    /// flags alone, and by [`parse`] over the GUI's saved settings too.
    #[arg(skip)]
    pub settings: Settings,

    /// Under `--gui`, the options given on the command line that it ignores, as the user would recognise them
    /// (`--upscale`, `-o`), in the order they were given; empty otherwise. Filled in by [`parse_from`].
    #[arg(skip)]
    pub ignored: Vec<String>,
}

impl Args {
    /// The processor the run asks for: `-p`, or the GUI's, or `auto`.
    pub const fn processor(&self) -> Processor {
        self.settings.processor.value
    }

    /// The quality each family runs at.
    pub const fn qualities(&self) -> &Qualities {
        &self.settings.qualities
    }
}

/// The upscale models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum UpscaleModel {
    Kyoto,
    // Retired: Osaka is a restore model now. Kept as a hidden value so `--up osaka` from an older script is refused by
    // `parse_from` with the flag that replaces it, rather than with a list that no longer names it; nothing builds it.
    #[value(hide = true)]
    Osaka,
    Tokyo,
    Saitama,
}

/// The lens blur models, as the command line names them: each is named for the depth model it is driven by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LensBlurModel {
    Lisbon,
}

/// What `--mask` is used for, as the command line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum MaskUse {
    /// Limit the enhancements to the mask and keep it in focus under a lens blur.
    #[default]
    Both,
    /// Limit the enhancements to the mask only.
    Enhancements,
    /// Keep the mask in focus under a lens blur only.
    LensBlur,
}

impl MaskUse {
    /// `mask` used as this says: whether it limits the enhancements, and whether a lens blur keeps it in focus.
    pub fn applied(self, mask: opai::Mask) -> opai::Mask {
        let (limits_enhancements, keeps_sharp) = match self {
            Self::Both => (true, true),
            Self::Enhancements => (true, false),
            Self::LensBlur => (false, true),
        };

        opai::Mask { limits_enhancements, keeps_sharp, ..mask }
    }
}

/// How a lens blur treats the edge of the subject in focus, as the command line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum LensBlurEdges {
    /// Matte the subject in focus and composite it over the blurred background.
    #[default]
    Matte,
    /// Let the depth alone decide what is sharp, as before matting existed.
    Depth,
    /// Matte the subject, but look for no holes in it: the background seen through it stays as the depth has it.
    Solid,
}

impl From<LensBlurEdges> for opai::LensBlurEdges {
    fn from(edges: LensBlurEdges) -> Self {
        match edges {
            LensBlurEdges::Matte => Self::Matte,
            LensBlurEdges::Depth => Self::Depth,
            LensBlurEdges::Solid => Self::Solid,
        }
    }
}

/// The face-recovery models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FaceModel {
    Athens,
    Santorini,
}

/// The denoise models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DenoiseModel {
    Stockholm,
    Gothenburg,
    Malmo,
}

/// The sharpen models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SharpenModel {
    Moscow,
    Petersburg,
    Novgorod,
}

/// The restore models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RestoreModel {
    Kazan,
    Osaka,
}

/// The colorization models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorizationModel {
    Delhi,
    Mumbai,
    Jaipur,
}

/// The light adjustment models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LightModel {
    Paris,
    Lyon,
}

/// The colour balance models, as the command line names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BalanceModel {
    Rio,
    // The library's codename, which clap would otherwise spell `sao-paulo`.
    #[value(name = "saopaulo")]
    SaoPaulo,
}

/// Which of its published builds each model runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Quality {
    /// The higher-fidelity build: FP32, or FP16 for Osaka.
    Hd,
    /// The lighter build: FP16, or INT8 for Osaka.
    Sd,
}

impl Quality {
    /// How the processor line names it: `HD` or `SD`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hd => "HD",
            Self::Sd => "SD",
        }
    }

    /// Which of a model's published precisions this names, by position: the first build the catalogue lists, or the
    /// second.
    pub const fn position(self) -> usize {
        match self {
            Self::Hd => 0,
            Self::Sd => 1,
        }
    }

    /// The quality a precision's `position` among a model's published ones names, or `None` past the second.
    pub const fn at(position: usize) -> Option<Self> {
        match position {
            0 => Some(Self::Hd),
            1 => Some(Self::Sd),
            _ => None,
        }
    }
}

// `#[cfg]` is resolved before `ValueEnum` expands, so a value no machine running this OS could ever offer is missing
// from the parser, its `[possible values: …]` and `--help` alike. Whether this machine offers the rest is `opai`'s to
// say, once initialization has run.
/// The execution providers, as the command line names them. Only the ones this OS can ever offer exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Processor {
    #[default]
    Auto,
    Cpu,
    // The explicit names stop clap from spelling these `core-ml` and `tensor-rt`.
    #[cfg(target_os = "macos")]
    #[value(name = "coreml")]
    CoreMl,
    #[cfg(not(target_os = "macos"))]
    Cuda,
    #[cfg(not(target_os = "macos"))]
    #[value(name = "tensorrt")]
    TensorRt,
    #[value(name = "webgpu")]
    WebGpu,
}

impl Processor {
    /// The name the command line takes for this processor.
    pub fn name(self) -> String {
        self.to_possible_value().expect("no processor is skipped").get_name().to_owned()
    }

    /// The processor `provider` is named by on this build's command line, or `None` where this OS has no value for it.
    pub fn from_provider(provider: opai::ExecutionProvider) -> Option<Self> {
        Self::value_variants()
            .iter()
            .copied()
            .find(|processor| opai::ExecutionProvider::from(*processor) == provider)
    }
}

impl From<Processor> for opai::ExecutionProvider {
    fn from(processor: Processor) -> Self {
        match processor {
            Processor::Auto => Self::Auto,
            Processor::Cpu => Self::Cpu,
            #[cfg(target_os = "macos")]
            Processor::CoreMl => Self::CoreMl,
            #[cfg(not(target_os = "macos"))]
            Processor::Cuda => Self::Cuda,
            #[cfg(not(target_os = "macos"))]
            Processor::TensorRt => Self::TensorRt,
            Processor::WebGpu => Self::WebGpu,
        }
    }
}

/// The command line this process was started with, or the process ended with clap's message and exit status.
///
/// Read through `wild`, which expands the wildcards `cmd` and PowerShell leave as written, and on every other platform
/// hands the arguments over unchanged. Under `--autopilot`, the `OPAI_AP_*` variables are read here too, so a bad one
/// is refused as a bad flag is, before anything is installed; without it they are not looked at.
///
/// The GUI's saved settings are read here too, unless `--no-gui-settings` is given, and rank below every flag and
/// variable; a file that cannot be read is recorded in the log and ignored. See [`crate::settings`].
pub fn parse() -> Args {
    let mut args = parse_from(wild::args_os()).unwrap_or_else(|error| error.exit());

    // `--gui` hands the inputs to the application, whose own settings apply there.
    if args.gui {
        return args;
    }

    let (gui, gui_file) = if args.no_gui_settings {
        (None, settings::GuiFile::NotRead)
    } else {
        settings::load(
            || opai::settings::path(opai::APP_NAME).map_err(|error| error.to_string()),
            opai::settings::read_at,
        )
    };

    args.settings = Settings { gui_file, ..Settings::resolve(args.quality, args.processor, gui.as_ref()) };

    if args.autopilot {
        let models =
            AutopilotModels::read(|name| std::env::var_os(name), gui.as_ref()).unwrap_or_else(|error| error.exit());
        args.autopilot_models = Some(models);
    }

    args
}

/// Which enhancement runs with which model and parameters: what the flags ask for, or what autopilot chose for one
/// photograph, and what [`plan`](crate::chain::plan) reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, clap::Args)]
pub struct Selection {
    // The enhancements are declared in the library's apply order, which is the order `--help` lists them in.
    /// Remove the noise from the photograph, with MODEL or Stockholm where none is named.
    #[arg(
        long = "denoise",
        visible_alias = "dn",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "stockholm",
        ignore_case = true
    )]
    pub denoise: Option<DenoiseModel>,

    /// The denoise strength, from 1 to 300 percent of the model's own effect; default 100.
    #[arg(long = "dn-str", value_name = "STRENGTH", value_parser = parse_strength, requires = "denoise")]
    pub denoise_strength: Option<u16>,

    /// Restore the faces in the photograph, with MODEL or Athens where none is named.
    #[arg(
        long = "face-recovery",
        visible_alias = "fr",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "athens",
        ignore_case = true
    )]
    pub face_recovery: Option<FaceModel>,

    /// Colorize the photograph, with MODEL or Delhi where none is named.
    #[arg(
        long = "colorization",
        visible_alias = "cl",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "delhi",
        ignore_case = true
    )]
    pub colorization: Option<ColorizationModel>,

    /// Adjust the photograph's light, with MODEL or Paris where none is named.
    #[arg(
        long = "light-adjustment",
        visible_alias = "la",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "paris",
        ignore_case = true
    )]
    pub light_adjustment: Option<LightModel>,

    /// The light adjustment bias, from -100 to 100 percent of the model's own effect; default 50.
    #[arg(
        long = "la-bias",
        value_name = "BIAS",
        value_parser = parse_bias,
        requires = "light_adjustment",
        allow_negative_numbers = true
    )]
    pub light_bias: Option<i16>,

    /// Balance the photograph's colours, with MODEL or Rio where none is named.
    #[arg(
        long = "color-balance",
        visible_alias = "cb",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "rio",
        ignore_case = true
    )]
    pub color_balance: Option<BalanceModel>,

    /// The colour balance bias, from -100 to 100 percent of the model's own effect; default 50.
    #[arg(
        long = "cb-bias",
        value_name = "BIAS",
        value_parser = parse_bias,
        requires = "color_balance",
        allow_negative_numbers = true
    )]
    pub balance_bias: Option<i16>,

    /// Sharpen the photograph, with MODEL or Moscow where none is named.
    #[arg(
        long = "sharpen",
        visible_alias = "sh",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "moscow",
        ignore_case = true
    )]
    pub sharpen: Option<SharpenModel>,

    /// The sharpen strength, from 1 to 300 percent of the model's own effect; default 100.
    #[arg(long = "sh-str", value_name = "STRENGTH", value_parser = parse_strength, requires = "sharpen")]
    pub sharpen_strength: Option<u16>,

    /// Upscale the photograph, with MODEL or Tokyo where none is named.
    #[arg(
        long = "upscale",
        visible_alias = "up",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "tokyo",
        ignore_case = true
    )]
    pub upscale: Option<UpscaleModel>,

    /// The factor to upscale by, from 1.0 to 8.0. Without it, the factor is what the photograph needs: 4x up to
    /// 1 MP, 2x up to 4 MP, and no upscale above that.
    #[arg(long, value_name = "FACTOR", value_parser = parse_scale, requires = "upscale")]
    pub scale: Option<Scale>,

    /// Restore the photograph generatively, after every other enhancement but a lens blur, with MODEL or Kazan where
    /// none is named. It redraws detail rather than recovering it, so it can change faces, textures and lettering.
    #[arg(
        long = "restore",
        visible_alias = "rs",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "kazan",
        ignore_case = true
    )]
    pub restore: Option<RestoreModel>,

    /// The size to restore at: 1 (the photograph's own; the default), 2 or 4 times it. Above 1 it cannot be combined
    /// with --upscale, since the restore enlarges the photograph itself.
    #[arg(long = "rs-scale", value_name = "FACTOR", value_parser = parse_restore_scale, requires = "restore")]
    pub restore_scale: Option<Scale>,

    /// The restore strength, from 1 to 100 percent of the model's own output; default 100.
    #[arg(long = "rs-str", value_name = "STRENGTH", value_parser = parse_restore_strength, requires = "restore")]
    pub restore_strength: Option<u16>,

    /// Blur the photograph as a wider lens aperture would have, last, with the depth estimated by MODEL or Lisbon
    /// where none is named.
    #[arg(
        long = "lens-blur",
        visible_alias = "lb",
        value_name = "MODEL",
        num_args = 0..=1,
        default_missing_value = "lisbon",
        ignore_case = true
    )]
    pub lens_blur: Option<LensBlurModel>,

    /// The f-number to simulate, in 35 mm full-frame-equivalent terms, from 1.0 to 32 (`1.4` or `f/1.4`); default
    /// 2.8. At or above the photograph's own the photograph is left as it is.
    #[arg(long = "lb-f", value_name = "F", value_parser = parse_f_number, requires = "lens_blur")]
    pub lens_blur_f: Option<f64>,

    /// How much blur to add, from 0 (the photograph as taken) to 100 (f/1.4 full-frame equivalent), the Blur amount
    /// slider of the app. The amount is laid out evenly in blur between the photograph's own aperture and f/1.4, so
    /// the same amount gives the same look on a photograph of any aperture. Instead of --lb-f.
    #[arg(
        long = "lb-amount",
        value_name = "AMOUNT",
        value_parser = parse_blur_amount,
        requires = "lens_blur",
        conflicts_with = "lens_blur_f",
        allow_hyphen_values = true
    )]
    pub lens_blur_amount: Option<f64>,

    /// The point to focus on, as fractions of the photograph's width and height from 0 to 1 (`0.5,0.3` is the middle
    /// across and 30 percent down); default 0.5,0.5, the centre.
    // Hyphen values allowed so that `-0.1,0.5`, which is no number clap would read as one, reaches `parse_focus` and is
    // refused with the range rather than as an unexpected argument.
    #[arg(
        long = "lb-focus",
        value_name = "X,Y",
        value_parser = parse_focus,
        requires = "lens_blur",
        allow_hyphen_values = true
    )]
    pub lens_blur_focus: Option<(f64, f64)>,

    /// How the edge of the subject in focus is made: `matte` mattes it (hair, fur and leaves stay sharp with the
    /// background between them blurred, and the background seen through holes in the subject, between a canopy's leaves,
    /// is found and blurred too; about 15 to 30 s for a new focus point, cached afterwards), `solid` mattes it without
    /// looking for holes, `depth` leaves it to the depth alone. Default matte.
    #[arg(long = "lb-edges", value_name = "EDGES", value_enum, ignore_case = true, requires = "lens_blur")]
    pub lens_blur_edges: Option<LensBlurEdges>,
}

impl Selection {
    /// This selection with a `--lb-amount` turned into the `--lb-f` it comes to on a photograph with the `lens` its
    /// EXIF settles (the assumed f/8 where there is none), and a sentence saying what it came to for the log.
    ///
    /// An amount is against the photograph's own aperture, which only the photograph has, so a selection is resolved
    /// for each. The f-number is the library's ([`opai::OwnLens::f_number_for_amount`]), the same the app's slider
    /// writes, so nothing downstream knows the difference. A selection with no amount is returned as it is.
    pub fn with_blur_amount_resolved(mut self, lens: Option<opai::OwnLens>) -> (Self, Option<String>) {
        let Some(amount) = self.lens_blur_amount.take() else { return (self, None) };

        let own = lens.unwrap_or_else(|| opai::OwnLens::from_exif(&opai::LensExif::default(), 1.0));
        let f_number = own.f_number_for_amount(amount);
        self.lens_blur_f = Some(f_number);

        let note = if own.can_blur_more() {
            format!(
                "the lens blur's amount {amount} is f/{f_number} (full-frame equivalent), against the photograph's own f/{}",
                own.own_f_number
            )
        } else {
            format!(
                "the lens blur's amount has no effect: the photograph was taken at f/{} (full-frame equivalent), as wide \
                 as the blur goes",
                own.own_f_number
            )
        };

        (self, Some(note))
    }

    /// Whether no enhancement is asked for.
    pub const fn is_empty(&self) -> bool {
        self.denoise.is_none()
            && self.face_recovery.is_none()
            && self.colorization.is_none()
            && self.light_adjustment.is_none()
            && self.color_balance.is_none()
            && self.sharpen.is_none()
            && self.upscale.is_none()
            && self.restore.is_none()
            && self.lens_blur.is_none()
    }

    /// Whether a restore enlarges the photograph, which an upscale cannot be combined with.
    pub fn restore_enlarges(&self) -> bool {
        self.restore.is_some() && self.restore_scale.is_some_and(|scale| scale.get() > Scale::MIN)
    }
}

/// `arguments` parsed and checked, every refusal a clap error so it is reported and exits as clap's own are.
pub fn parse_from<I, T>(arguments: I) -> Result<Args, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let mut command = Args::command();
    let matches = command.try_get_matches_from_mut(arguments)?;
    let mut args = Args::from_arg_matches(&matches).map_err(|error| error.format(&mut command))?;
    args.settings = Settings::resolve(args.quality, args.processor, None);

    // `--gui` hands the inputs to the application and does nothing else, so it needs no enhancement; what else was
    // given is collected to be warned about.
    if args.gui {
        args.ignored = ignored_by_gui(&command, &matches);
        return Ok(args);
    }

    // Not a clap `ArgGroup`: its refusal lists the group's members as "required arguments", where what the user needs
    // to hear is that an enhancement is.
    if !args.autopilot && args.selection.is_empty() && args.remove.is_none() {
        return Err(Args::command().error(
            clap::error::ErrorKind::MissingRequiredArgument,
            "at least one enhancement is required: --remove (--rm), --denoise (--dn), --face-recovery (--fr), \
             --colorization (--cl), --light-adjustment (--la), --color-balance (--cb), --sharpen (--sh), \
             --upscale (--up), --restore (--rs), --lens-blur (--lb) or --autopilot (--ap)",
        ));
    }

    // Osaka moved from upscale to restore. A script still asking for it as an upscale is told where it went, rather
    // than being upscaled by some other model or shown a list that no longer names it.
    if args.selection.upscale == Some(UpscaleModel::Osaka) {
        return Err(Args::command().error(
            clap::error::ErrorKind::InvalidValue,
            "Osaka is a restore model now: use --restore osaka (--rs osaka) instead of --upscale osaka, with \
             --rs-scale 2 or 4 to enlarge",
        ));
    }

    // Not a clap `conflicts_with`: the conflict is with one value of --rs-scale, not the flag, and what the user needs
    // to hear is why. Two enlargements in one chain would restore an upscale's output at up to sixteen times the
    // photograph's pixels; a front end offers one or the other, and the GUI disables its upscale the same way.
    if args.selection.upscale.is_some() && args.selection.restore_enlarges() {
        let scale = args.selection.restore_scale.map_or(Scale::MIN, Scale::get);

        return Err(Args::command().error(
            clap::error::ErrorKind::ArgumentConflict,
            format!(
                "--upscale (--up) cannot be used with --rs-scale {scale}: the restore enlarges the photograph itself. \
                 Leave out --up, or restore at 1x after the upscale"
            ),
        ));
    }

    // Autopilot never chooses a lens blur, so it is refused with that too.
    if args.mask_use == Some(MaskUse::LensBlur) && args.selection.lens_blur.is_none() {
        return Err(Args::command().error(
            clap::error::ErrorKind::ArgumentConflict,
            "--mask-use lens-blur needs --lens-blur (--lb): without a lens blur the mask would have no effect",
        ));
    }

    Ok(args)
}

/// The arguments, by clap id, that `--gui` is never warned about: the inputs and `--recursive`, which choose what it
/// opens, `--gui` itself, and the lens blur it asks the application to add. Every other argument is warned about when
/// given, so an option added later is covered without being listed; one `--gui` should accept silently is added here.
const NOT_IGNORED_BY_GUI: &[&str] = &[
    "inputs",
    "recursive",
    "gui",
    "verbose",
    "lens_blur",
    "lens_blur_f",
    "lens_blur_amount",
    "lens_blur_focus",
];

/// The options `--gui` does not use, each named by its long flag, in the order given: every one written on the command
/// line except those in [`NOT_IGNORED_BY_GUI`]. A default or an environment variable is not counted, only what the
/// user wrote.
fn ignored_by_gui(command: &clap::Command, matches: &ArgMatches) -> Vec<String> {
    let mut given: Vec<(usize, String)> = command
        .get_arguments()
        .filter(|argument| !NOT_IGNORED_BY_GUI.contains(&argument.get_id().as_str()))
        .filter(|argument| matches.value_source(argument.get_id().as_str()) == Some(ValueSource::CommandLine))
        .map(|argument| {
            let id = argument.get_id().as_str();
            let name = argument.get_long().map_or_else(
                || argument.get_short().map_or_else(|| id.to_owned(), |short| format!("-{short}")),
                |long| format!("--{long}"),
            );
            (matches.index_of(id).unwrap_or(usize::MAX), name)
        })
        .collect();

    given.sort();
    given.into_iter().map(|(_, name)| name).collect()
}

/// A `--format` value, in any case, with every extension a format is written under — `jpg`, `tif`, `heif` — read as
/// the format it names.
///
/// The aliases are this command's: the library's [`FormatChoice`] reads the one spelling the GUI stores, and widening it
/// would make the GUI's wire accept words its window never sends.
fn parse_format(text: &str) -> Result<FormatChoice, String> {
    let word = text.trim().to_lowercase();

    FormatChoice::from_str(&word)
        .ok()
        .or_else(|| ExportFormat::preserving(&word).map(FormatChoice::Format))
        .ok_or_else(|| {
            let names: Vec<_> = ExportFormat::ALL.into_iter().map(ExportFormat::name).collect();
            format!("{text:?} is not a format; use preserve, {}", names.join(", "))
        })
}

/// A `--scale` value, refused with the accepted range where the library's [`Scale`] refuses it.
fn parse_scale(text: &str) -> Result<Scale, String> {
    let range = || format!("the scale must be from {:.1} to {:.1}", Scale::MIN, Scale::MAX);

    let value: f64 = text.trim().parse().map_err(|_| format!("{text:?} is not a number; {}", range()))?;
    Scale::new(value).map_err(|_| range())
}

/// The sizes `--rs-scale` takes: the ones a front end offers, each a whole factor the restore resamples to.
const RESTORE_SCALES: [u8; 3] = [1, 2, 4];

/// A `--rs-scale` value: one of [`RESTORE_SCALES`], refused naming them otherwise.
fn parse_restore_scale(text: &str) -> Result<Scale, String> {
    let sizes = || RESTORE_SCALES.map(|size| size.to_string()).join(", ");

    text.trim()
        .parse::<u8>()
        .ok()
        .filter(|size| RESTORE_SCALES.contains(size))
        .and_then(|size| Scale::new(f64::from(size)).ok())
        .ok_or_else(|| format!("the restore size must be one of {}", sizes()))
}

/// The highest `--rs-str`, the library's [`opai::Restore::MAX_STRENGTH`] as a percentage: a restoration is blended
/// back towards the photograph, never pushed past the model's own output.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole, positive percentage"
)]
const RESTORE_STRENGTH_MAX: u16 = (opai::Restore::MAX_STRENGTH * 100.0) as u16;

/// A `--rs-str` value: a whole percentage, refused with the accepted range otherwise.
fn parse_restore_strength(text: &str) -> Result<u16, String> {
    text.trim()
        .parse()
        .ok()
        .filter(|value| (STRENGTH_MIN..=RESTORE_STRENGTH_MAX).contains(value))
        .ok_or_else(|| format!("the strength must be a whole number from {STRENGTH_MIN} to {RESTORE_STRENGTH_MAX}"))
}

/// A `--lb-f` value: a number, bare or after `f/`, from [`opai::FNumber::MIN`] to [`opai::FNumber::MAX`].
fn parse_f_number(text: &str) -> Result<f64, String> {
    let range = || format!("the f-number must be from {} to {}", opai::FNumber::MIN, opai::FNumber::MAX);
    let bare = text.trim();
    let bare = bare.strip_prefix("f/").or_else(|| bare.strip_prefix("F/")).unwrap_or(bare);

    let value: f64 = bare.trim().parse().map_err(|_| format!("{text:?} is not a number; {}", range()))?;
    opai::FNumber::new(value).map(opai::FNumber::get).map_err(|_| range())
}

/// A `--lb-amount` value: a number from 0 to 100.
fn parse_blur_amount(text: &str) -> Result<f64, String> {
    let range = || "the blur amount must be from 0 to 100".to_string();

    let value: f64 = text.trim().parse().map_err(|_| format!("{text:?} is not a number; {}", range()))?;
    if (0.0..=100.0).contains(&value) { Ok(value) } else { Err(range()) }
}

/// A `--lb-focus` value: two fractions from 0 to 1, across and down, separated by a comma.
fn parse_focus(text: &str) -> Result<(f64, f64), String> {
    let wanted = || format!("{text:?} is not a point; give two fractions from 0 to 1 as X,Y, such as 0.5,0.3");
    let (x, y) = text.split_once(',').ok_or_else(wanted)?;
    let fraction = |part: &str, axis: &'static str| {
        let value: f64 = part.trim().parse().map_err(|_| wanted())?;
        // Named for the axis, which is what the library publishes it under, rather than for the type serving both.
        opai::Fraction::new(value)
            .map(opai::Fraction::get)
            .map_err(|refused| format!("the focus point must be two fractions from 0 to 1; {}", refused.named(axis)))
    };

    Ok((fraction(x, opai::FOCUS_X)?, fraction(y, opai::FOCUS_Y)?))
}

/// The lowest `--dn-str`, `--sh-str` or `--rs-str`. Not the library's 0: a strength of 0 applies nothing, which on a command line is
/// a mistake rather than a choice.
const STRENGTH_MIN: u16 = 1;

/// The highest `--dn-str` or `--sh-str`, the library's [`opai::Strength::MAX`] as a percentage.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole, positive percentage"
)]
const STRENGTH_MAX: u16 = (opai::Strength::MAX * 100.0) as u16;

/// The lowest `--la-bias` or `--cb-bias`, the library's [`opai::Bias::MIN`] as a percentage.
#[allow(clippy::cast_possible_truncation, reason = "a whole percentage")]
const BIAS_MIN: i16 = (opai::Bias::MIN * 100.0) as i16;

/// The highest `--la-bias` or `--cb-bias`, the library's [`opai::Bias::MAX`] as a percentage.
#[allow(clippy::cast_possible_truncation, reason = "a whole percentage")]
const BIAS_MAX: i16 = (opai::Bias::MAX * 100.0) as i16;

/// A `--dn-str` or `--sh-str` value: a whole percentage, refused with the accepted range otherwise.
fn parse_strength(text: &str) -> Result<u16, String> {
    text.trim()
        .parse()
        .ok()
        .filter(|value| (STRENGTH_MIN..=STRENGTH_MAX).contains(value))
        .ok_or_else(|| format!("the strength must be a whole number from {STRENGTH_MIN} to {STRENGTH_MAX}"))
}

/// A `--la-bias` or `--cb-bias` value: a whole percentage, refused with the accepted range otherwise.
fn parse_bias(text: &str) -> Result<i16, String> {
    text.trim()
        .parse()
        .ok()
        .filter(|value| (BIAS_MIN..=BIAS_MAX).contains(value))
        .ok_or_else(|| format!("the bias must be a whole number from {BIAS_MIN} to {BIAS_MAX}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse(line: &str) -> Result<Args, clap::Error> {
        parse_from(line.split_whitespace())
    }

    #[test]
    fn bare_flags_take_their_default_models_and_quality() {
        let args = parse("opai photo.jpg --up --fr -o out.png").expect("parses");

        assert_eq!(args.inputs, [PathBuf::from("photo.jpg")]);
        assert_eq!(args.selection.upscale, Some(UpscaleModel::Tokyo));
        assert_eq!(args.selection.face_recovery, Some(FaceModel::Athens));
        assert_eq!(args.qualities().uniform(), Some(Quality::Hd));
        assert_eq!(args.selection.scale, None);
        assert_eq!(args.output, Some(PathBuf::from("out.png")));
    }

    #[test]
    fn analytics_is_on_unless_the_flag_turns_it_off() {
        assert!(!parse("opai photo.jpg --up -o out.png").expect("parses").no_analytics);
        assert!(parse("opai photo.jpg --up --no-analytics -o out.png").expect("parses").no_analytics);
    }

    #[test]
    fn the_long_names_are_the_aliases_equals() {
        let args = parse("opai photo.jpg --upscale --face-recovery --quality sd --output out.png").expect("parses");

        assert_eq!(args.selection.upscale, Some(UpscaleModel::Tokyo));
        assert_eq!(args.selection.face_recovery, Some(FaceModel::Athens));
        assert_eq!(args.qualities().uniform(), Some(Quality::Sd));
    }

    #[test]
    fn a_named_model_is_taken_whatever_its_case() {
        let args = parse("opai photo.jpg --up KYOTO --fr Santorini --rs OSAKA -q SD -o out.png").expect("parses");

        assert_eq!(args.selection.upscale, Some(UpscaleModel::Kyoto));
        assert_eq!(args.selection.face_recovery, Some(FaceModel::Santorini));
        assert_eq!(args.selection.restore, Some(RestoreModel::Osaka));
        assert_eq!(args.qualities().uniform(), Some(Quality::Sd));

        for (name, model) in [
            ("kyoto", UpscaleModel::Kyoto),
            ("tokyo", UpscaleModel::Tokyo),
            ("saitama", UpscaleModel::Saitama),
        ] {
            let args = parse(&format!("opai photo.jpg --up {name} -o out.png")).expect("parses");
            assert_eq!(args.selection.upscale, Some(model), "{name}");
        }
    }

    #[test]
    fn an_unknown_model_is_a_usage_error_listing_the_accepted_ones() {
        let error = parse("opai photo.jpg --up paris -o out.png").expect_err("paris is not an upscale model");

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
        assert_eq!(error.exit_code(), 2);
        let message = error.to_string();
        for model in ["kyoto", "tokyo", "saitama"] {
            assert!(message.contains(model), "{model} is not listed in: {message}");
        }
        assert!(!message.contains("osaka"), "Osaka is still offered as an upscaler: {message}");

        let error = parse("opai photo.jpg --fr tokyo -o out.png").expect_err("tokyo is not a face model");
        assert_eq!(error.kind(), ErrorKind::InvalidValue);
    }

    #[test]
    fn osaka_as_an_upscale_is_refused_naming_the_flag_that_replaced_it() {
        for line in ["--up osaka", "--upscale OSAKA --scale 2", "--up=osaka"] {
            let error = parse(&format!("opai photo.jpg {line} -o out.png")).expect_err(line);

            assert_eq!(error.kind(), ErrorKind::InvalidValue, "{line}: {error}");
            assert_eq!(error.exit_code(), 2);
            assert!(error.to_string().contains("use --restore osaka (--rs osaka)"), "{line}: {error}");
        }

        let args = parse("opai photo.jpg --rs osaka --rs-scale 2 -o out.png").expect("Osaka restores");
        assert_eq!(args.selection.restore, Some(RestoreModel::Osaka));
    }

    #[test]
    fn a_scale_requires_an_upscale() {
        let error = parse("opai photo.jpg --fr --scale 2 -o out.png").expect_err("--scale without --up");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
        assert!(error.to_string().contains("--upscale"), "{error}");
    }

    #[test]
    fn a_scale_out_of_range_is_refused_naming_the_range() {
        for scale in ["9", "0.5", "abc"] {
            let error = parse(&format!("opai photo.jpg --up --scale {scale} -o out.png")).expect_err(scale);

            assert_eq!(error.kind(), ErrorKind::ValueValidation, "{scale}");
            assert!(error.to_string().contains("from 1.0 to 8.0"), "{error}");
        }

        let args = parse("opai photo.jpg --up --scale 2.5 -o out.png").expect("2.5 is in range");
        assert_eq!(args.selection.scale.map(Scale::get), Some(2.5));
    }

    #[test]
    fn no_enhancement_is_a_usage_error() {
        let error = parse("opai photo.jpg -o out.png").expect_err("nothing to do");

        assert_eq!(error.exit_code(), 2);
        let message = error.to_string();
        assert!(message.contains("at least one enhancement is required"), "{message}");
        for (flag, alias) in [
            ("--denoise", "--dn"),
            ("--face-recovery", "--fr"),
            ("--colorization", "--cl"),
            ("--light-adjustment", "--la"),
            ("--color-balance", "--cb"),
            ("--sharpen", "--sh"),
            ("--upscale", "--up"),
            ("--restore", "--rs"),
            ("--lens-blur", "--lb"),
            ("--autopilot", "--ap"),
        ] {
            assert!(message.contains(&format!("{flag} ({alias})")), "{flag} is not listed in: {message}");
        }
    }

    #[test]
    fn autopilot_alone_is_enough() {
        let args = parse("opai photo.jpg --ap -o out.png").expect("parses");
        assert!(args.autopilot);
        assert_eq!(args.selection, Selection::default());

        assert!(parse("opai photo.jpg --autopilot -q sd -p cpu -o out.png").expect("parses").autopilot);
        assert!(!parse("opai photo.jpg --up -o out.png").expect("parses").autopilot);
    }

    #[test]
    fn the_environment_is_not_read_by_parse_from() {
        assert_eq!(parse("opai photo.jpg --up -o out.png").expect("parses").autopilot_models, None);
        assert_eq!(parse("opai photo.jpg --ap -o out.png").expect("parses").autopilot_models, None);
    }

    #[test]
    fn autopilot_cannot_be_combined_with_an_enhancement_or_a_parameter() {
        // Each flag with what it needs to parse on its own, and the name clap gives it in the refusal. A parameter is
        // given with its enhancement, so the refusal is about autopilot rather than about a missing enhancement.
        let cases = [
            ("--dn", "--denoise"),
            ("--fr", "--face-recovery"),
            ("--cl", "--colorization"),
            ("--la", "--light-adjustment"),
            ("--cb", "--color-balance"),
            ("--sh", "--sharpen"),
            ("--up", "--upscale"),
            ("--rs", "--restore"),
            ("--scale 2", "--scale"),
            ("--dn-str 150", "--dn-str"),
            ("--sh-str 150", "--sh-str"),
            ("--la-bias 20", "--la-bias"),
            ("--cb-bias 20", "--cb-bias"),
            ("--lb", "--lens-blur"),
            ("--lb --lb-f 2", "--lens-blur"),
            ("--lb --lb-amount 50", "--lens-blur"),
        ];

        for (flags, name) in cases {
            let error = parse(&format!("opai photo.jpg --ap {flags} -o out.png")).expect_err(flags);

            assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{flags}: {error}");
            assert_eq!(error.exit_code(), 2);
            let message = error.to_string();
            assert!(message.contains("--autopilot") && message.contains(name), "{flags}: {message}");
            assert!(message.contains("cannot be used with"), "{flags}: {message}");
        }
    }

    #[test]
    fn a_new_family_alone_is_enough() {
        let args = parse("opai photo.jpg --cl -o out.png").expect("parses");

        assert_eq!(args.selection.colorization, Some(ColorizationModel::Delhi));
        assert_eq!(args.selection.upscale, None);
        assert_eq!(args.selection.face_recovery, None);
    }

    #[test]
    fn bare_new_flags_take_their_default_models_and_no_parameters() {
        let args = parse("opai photo.jpg --dn --sh --cl --la --cb -o out.png").expect("parses");

        assert_eq!(args.selection.denoise, Some(DenoiseModel::Stockholm));
        assert_eq!(args.selection.sharpen, Some(SharpenModel::Moscow));
        assert_eq!(args.selection.colorization, Some(ColorizationModel::Delhi));
        assert_eq!(args.selection.light_adjustment, Some(LightModel::Paris));
        assert_eq!(args.selection.color_balance, Some(BalanceModel::Rio));
        assert_eq!(
            (
                args.selection.denoise_strength,
                args.selection.sharpen_strength,
                args.selection.light_bias,
                args.selection.balance_bias
            ),
            (None, None, None, None)
        );

        let long = parse("opai photo.jpg --denoise --sharpen --colorization --light-adjustment --color-balance -o x")
            .expect("parses");
        assert_eq!(long.selection.denoise, Some(DenoiseModel::Stockholm));
        assert_eq!(long.selection.sharpen, Some(SharpenModel::Moscow));
        assert_eq!(long.selection.colorization, Some(ColorizationModel::Delhi));
        assert_eq!(long.selection.light_adjustment, Some(LightModel::Paris));
        assert_eq!(long.selection.color_balance, Some(BalanceModel::Rio));
    }

    #[test]
    fn a_named_new_model_is_taken_whatever_its_case() {
        let args = parse("opai photo.jpg --dn MALMO --sh Petersburg --cl jaiPUR --la LYON --cb SaoPaulo -o out.png")
            .expect("parses");

        assert_eq!(args.selection.denoise, Some(DenoiseModel::Malmo));
        assert_eq!(args.selection.sharpen, Some(SharpenModel::Petersburg));
        assert_eq!(args.selection.colorization, Some(ColorizationModel::Jaipur));
        assert_eq!(args.selection.light_adjustment, Some(LightModel::Lyon));
        assert_eq!(args.selection.color_balance, Some(BalanceModel::SaoPaulo));

        let args = parse("opai photo.jpg --dn gothenburg --sh novgorod --cl mumbai -o out.png").expect("parses");
        assert_eq!(args.selection.denoise, Some(DenoiseModel::Gothenburg));
        assert_eq!(args.selection.sharpen, Some(SharpenModel::Novgorod));
        assert_eq!(args.selection.colorization, Some(ColorizationModel::Mumbai));
    }

    #[test]
    fn an_unknown_new_model_is_a_usage_error_listing_the_accepted_ones() {
        let cases: [(&str, &[&str]); 5] = [
            ("--dn tokyo", &["stockholm", "gothenburg", "malmo"]),
            ("--sh tokyo", &["moscow", "petersburg", "novgorod"]),
            ("--cl tokyo", &["delhi", "mumbai", "jaipur"]),
            ("--la tokyo", &["paris", "lyon"]),
            ("--cb sao-paulo", &["rio", "saopaulo"]),
        ];

        for (flags, accepted) in cases {
            let error = parse(&format!("opai photo.jpg {flags} -o out.png")).expect_err(flags);

            assert_eq!(error.kind(), ErrorKind::InvalidValue, "{flags}: {error}");
            let message = error.to_string();
            for model in accepted {
                assert!(message.contains(model), "{model} is not listed in: {message}");
            }
        }
    }

    #[test]
    fn a_strength_out_of_range_or_fractional_is_refused_naming_the_range() {
        for (flags, range) in [
            ("--dn --dn-str 0", "1 to 300"),
            ("--dn --dn-str 301", "1 to 300"),
            ("--dn --dn-str 150.5", "1 to 300"),
            ("--sh --sh-str 1000", "1 to 300"),
            ("--cb --cb-bias 101", "-100 to 100"),
            ("--la --la-bias -101", "-100 to 100"),
            ("--la --la-bias 0.5", "-100 to 100"),
        ] {
            let error = parse(&format!("opai photo.jpg {flags} -o out.png")).expect_err(flags);

            assert_eq!(error.kind(), ErrorKind::ValueValidation, "{flags}: {error}");
            assert!(error.to_string().contains(&format!("a whole number from {range}")), "{error}");
        }
    }

    #[test]
    fn a_bare_restore_is_kazan_at_the_photographs_own_size() {
        let args = parse("opai photo.jpg --rs -o out.png").expect("parses");

        assert_eq!(args.selection.restore, Some(RestoreModel::Kazan));
        assert_eq!((args.selection.restore_scale, args.selection.restore_strength), (None, None));
        assert!(!args.selection.restore_enlarges());

        let args = parse("opai photo.jpg --restore KAZAN --rs-scale 4 --rs-str 60 -o out.png").expect("parses");
        assert_eq!(args.selection.restore, Some(RestoreModel::Kazan));
        assert_eq!(args.selection.restore_scale.map(Scale::get), Some(4.0));
        assert_eq!(args.selection.restore_strength, Some(60));
        assert!(args.selection.restore_enlarges());
    }

    #[test]
    fn a_restore_size_is_one_of_the_three_offered() {
        for size in ["1", "2", "4"] {
            let args = parse(&format!("opai photo.jpg --rs --rs-scale {size} -o out.png")).expect(size);
            assert_eq!(args.selection.restore_scale.map(Scale::get), size.parse().ok(), "{size}");
        }

        for size in ["3", "8", "1.5", "0", "two"] {
            let error = parse(&format!("opai photo.jpg --rs --rs-scale {size} -o out.png")).expect_err(size);

            assert_eq!(error.kind(), ErrorKind::ValueValidation, "{size}: {error}");
            assert!(error.to_string().contains("one of 1, 2, 4"), "{error}");
        }
    }

    #[test]
    fn a_restore_strength_stops_at_the_models_own_output() {
        for strength in ["0", "101", "50.5"] {
            let error = parse(&format!("opai photo.jpg --rs --rs-str {strength} -o out.png")).expect_err(strength);

            assert_eq!(error.kind(), ErrorKind::ValueValidation, "{strength}: {error}");
            assert!(error.to_string().contains("a whole number from 1 to 100"), "{error}");
        }
    }

    #[test]
    fn an_enlarging_restore_cannot_follow_an_upscale() {
        for flags in ["--up --rs --rs-scale 2", "--rs-scale 4 --rs --up kyoto --scale 2"] {
            let error = parse(&format!("opai photo.jpg {flags} -o out.png")).expect_err(flags);

            assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{flags}: {error}");
            assert_eq!(error.exit_code(), 2);
            let message = error.to_string();
            assert!(message.contains("--upscale (--up) cannot be used with --rs-scale"), "{message}");
            assert!(message.contains("restore at 1x"), "{message}");
        }

        // At 1x the restore follows the upscale and restores the photograph at its final size.
        let args = parse("opai photo.jpg --up --rs --rs-scale 1 -o out.png").expect("a 1x restore after an upscale");
        assert!(args.selection.upscale.is_some() && args.selection.restore.is_some());
    }

    #[test]
    fn each_parameter_is_read_for_its_own_enhancement() {
        let args = parse("opai photo.jpg --dn --dn-str 200 --sh --sh-str 80 -o out.png").expect("parses");
        assert_eq!(args.selection.denoise_strength, Some(200));
        assert_eq!(args.selection.sharpen_strength, Some(80));

        let args = parse("opai photo.jpg --la --la-bias -20 -o out.png").expect("parses");
        assert_eq!(args.selection.light_bias, Some(-20));
        assert_eq!(args.output, Some(PathBuf::from("out.png")));

        let args = parse("opai photo.jpg --cb --cb-bias -100 --la --la-bias 100 -o out.png").expect("parses");
        assert_eq!(args.selection.balance_bias, Some(-100));
        assert_eq!(args.selection.light_bias, Some(100));
    }

    #[test]
    fn a_parameter_requires_its_enhancement() {
        for (flags, required) in [
            ("--sh --dn-str 150", "--denoise"),
            ("--dn --sh-str 150", "--sharpen"),
            ("--cb --la-bias 20", "--light-adjustment"),
            ("--la --cb-bias 20", "--color-balance"),
            ("--up --rs-scale 2", "--restore"),
            ("--sh --rs-str 50", "--restore"),
            ("--up --lb-f 2", "--lens-blur"),
            ("--up --lb-amount 50", "--lens-blur"),
            ("--up --lb-focus 0.3,0.6", "--lens-blur"),
            ("--up --lb-edges matte", "--lens-blur"),
        ] {
            let error = parse(&format!("opai photo.jpg {flags} -o out.png")).expect_err(flags);

            assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument, "{flags}: {error}");
            assert!(error.to_string().contains(required), "{error}");
        }
    }

    #[test]
    fn a_mask_is_read_with_an_enhancement_or_with_autopilot() {
        let args = parse("opai photo.jpg --sh --mask subject.png -o out.png").expect("parses");
        assert_eq!(args.mask, Some(PathBuf::from("subject.png")));

        assert!(parse("opai photo.jpg --ap --mask subject.png -o out.png").expect("parses").mask.is_some());
        assert_eq!(parse("opai photo.jpg --sh -o out.png").expect("parses").mask, None);
    }

    #[test]
    fn a_mask_is_used_for_both_unless_told_otherwise() {
        let args = parse("opai photo.jpg --sh --lb --mask subject.png -o out.png").expect("parses");
        assert_eq!(args.mask_use, None);

        for (word, mask_use) in [
            ("both", MaskUse::Both),
            ("enhancements", MaskUse::Enhancements),
            ("Lens-Blur", MaskUse::LensBlur),
        ] {
            let line = format!("opai photo.jpg --sh --lb --mask subject.png --mask-use {word} -o out.png");
            assert_eq!(parse(&line).expect("parses").mask_use, Some(mask_use), "{word}");
        }

        let applied = |mask_use: MaskUse| {
            let mask = mask_use.applied(opai::Mask::default());
            (mask.limits_enhancements, mask.keeps_sharp)
        };
        assert_eq!(applied(MaskUse::Both), (true, true));
        assert_eq!(applied(MaskUse::Enhancements), (true, false));
        assert_eq!(applied(MaskUse::LensBlur), (false, true));
    }

    #[test]
    fn a_mask_use_needs_a_mask_and_focus_alone_needs_a_lens_blur() {
        let error = parse("opai photo.jpg --sh --lb --mask-use both -o out.png").expect_err("no mask");
        assert_eq!(error.exit_code(), 2, "{error}");
        assert!(error.to_string().contains("--mask"), "{error}");

        for line in [
            "opai photo.jpg --sh --mask subject.png --mask-use lens-blur -o out.png",
            "opai photo.jpg --ap --mask subject.png --mask-use lens-blur -o out.png",
        ] {
            let error = parse(line).expect_err("nothing to keep in focus");
            assert_eq!(error.exit_code(), 2, "{error}");
            assert!(error.to_string().contains("the mask would have no effect"), "{error}");
        }

        assert!(parse("opai photo.jpg --sh --mask subject.png --mask-use enhancements -o out.png").is_ok());
        assert!(parse("opai photo.jpg --lb --mask subject.png --mask-use lens-blur -o out.png").is_ok());
    }

    #[test]
    fn a_removal_mask_is_an_enhancement_on_its_own_and_goes_with_the_others() {
        let args = parse("opai photo.jpg --rm tag.png -o out.png").expect("a removal alone is an enhancement");
        assert_eq!(args.remove, Some(PathBuf::from("tag.png")));
        assert!(args.selection.is_empty());

        let args = parse("opai photo.jpg --remove tag.png --sh --mask subject.png -o out.png").expect("parses");
        assert_eq!((args.remove.is_some(), args.mask.is_some()), (true, true));
        assert!(parse("opai photo.jpg --ap --rm tag.png -o out.png").expect("parses").remove.is_some());
        assert_eq!(parse("opai photo.jpg --sh -o out.png").expect("parses").remove, None);
    }

    #[test]
    fn a_removal_without_its_file_is_a_usage_error() {
        let error = parse("opai photo.jpg --rm -o out.png").expect_err("no file named");
        assert_eq!(error.exit_code(), 2, "{error}");

        let error = parse("opai photo.jpg -o out.png").expect_err("nothing asked for");
        assert!(error.to_string().contains("--remove (--rm)"), "{error}");
    }

    #[test]
    fn a_mask_without_an_enhancement_is_a_usage_error() {
        let error = parse("opai photo.jpg --mask subject.png -o out.png").expect_err("nothing to mask");

        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("at least one enhancement is required"), "{error}");

        let error = parse("opai photo.jpg --sh --mask -o out.png").expect_err("no file named");
        assert_eq!(error.exit_code(), 2, "{error}");
    }

    /// The one place the CLI's model names, default models, parameter ranges and documented defaults are checked
    /// against what the library publishes, so a variant added or renamed there fails here rather than going unoffered.
    #[test]
    fn the_new_families_match_the_librarys_catalogue() {
        use opai::{Family, ParameterKind};

        fn name(value: &impl ValueEnum) -> String {
            value.to_possible_value().expect("no variant is skipped").get_name().to_owned()
        }

        fn names<T: ValueEnum>() -> Vec<String> {
            T::value_variants().iter().map(name).collect()
        }

        let command = Args::command();
        let bare = parse("opai photo.jpg --dn --sh --cl --la --cb -o out.png").expect("parses");
        let families = [
            (Family::Denoise, names::<DenoiseModel>(), bare.selection.denoise.as_ref().map(name)),
            (Family::Sharpen, names::<SharpenModel>(), bare.selection.sharpen.as_ref().map(name)),
            (
                Family::Colorization,
                names::<ColorizationModel>(),
                bare.selection.colorization.as_ref().map(name),
            ),
            (
                Family::LightAdjustment,
                names::<LightModel>(),
                bare.selection.light_adjustment.as_ref().map(name),
            ),
            (Family::ColorBalance, names::<BalanceModel>(), bare.selection.color_balance.as_ref().map(name)),
        ];

        for (family, names, default_model) in families {
            let entry = opai::catalogue().iter().find(|entry| entry.family == family).expect("published");
            let published: Vec<_> = entry.variants.iter().map(|variant| variant.codename).collect();
            assert_eq!(names, published, "{family:?}");
            assert_eq!(default_model.as_deref(), Some(published[0]), "{family:?}'s default model");

            // Every variant of these families takes the same parameter, so the first one speaks for the family.
            let documented = match family {
                Family::Denoise | Family::Sharpen => Some(("strength", 100.0)),
                Family::LightAdjustment | Family::ColorBalance => Some(("bias", 50.0)),
                _ => None,
            };
            let parameters = entry.variants[0].parameters;
            let Some((name, default)) = documented else {
                assert!(parameters.is_empty(), "{family:?} takes {parameters:?}");
                continue;
            };
            let [parameter] = parameters else { panic!("{family:?} takes {parameters:?}") };
            let ParameterKind::Range { min, max, default: published_default } = parameter.kind else {
                panic!("{family:?}'s {name} is not a range")
            };
            assert_eq!(parameter.name, name, "{family:?}");
            assert_eq!(published_default * 100.0, default, "{family:?}'s documented default");

            if name == "strength" {
                assert_eq!(max * 100.0, f64::from(STRENGTH_MAX), "{family:?}");
                assert!(f64::from(STRENGTH_MIN) > min * 100.0, "{family:?}");
            } else {
                assert_eq!((min * 100.0, max * 100.0), (f64::from(BIAS_MIN), f64::from(BIAS_MAX)), "{family:?}");
            }

            let help = argument_help(&command, family);
            assert!(help.contains(&format!("default {default}")), "{family:?}: {help}");
        }
    }

    /// The help line of the parameter flag `family` takes.
    fn argument_help(command: &clap::Command, family: opai::Family) -> String {
        use opai::Family;

        let id = match family {
            Family::Denoise => "denoise_strength",
            Family::Sharpen => "sharpen_strength",
            Family::LightAdjustment => "light_bias",
            Family::ColorBalance => "balance_bias",
            _ => unreachable!("{family:?} takes no parameter flag"),
        };
        let argument = command.get_arguments().find(|argument| argument.get_id() == id).expect(id);
        argument.get_help().expect("documented").to_string()
    }

    #[test]
    fn a_bare_lens_blur_is_lisbon_at_its_defaults() {
        let args = parse("opai photo.jpg --lb -o out.jpg").expect("parses");

        assert_eq!(args.selection.lens_blur, Some(LensBlurModel::Lisbon));
        assert_eq!((args.selection.lens_blur_f, args.selection.lens_blur_focus), (None, None));
        assert!(!args.selection.is_empty(), "a lens blur alone is an enhancement");
    }

    #[test]
    fn a_lens_blur_takes_its_f_number_bare_or_after_f_and_its_focus_as_two_fractions() {
        let args =
            parse("opai photo.jpg --lens-blur LISBON --lb-f f/1.4 --lb-focus 0.3,0.6 -o out.jpg").expect("parses");

        assert_eq!(args.selection.lens_blur, Some(LensBlurModel::Lisbon));
        assert_eq!(args.selection.lens_blur_f, Some(1.4));
        assert_eq!(args.selection.lens_blur_focus, Some((0.3, 0.6)));

        for (text, expected) in [("2", 2.0), ("F/2.8", 2.8), (" 32 ", 32.0), ("1", 1.0)] {
            assert_eq!(parse_f_number(text), Ok(expected), "{text}");
        }
        assert_eq!(parse_focus("0, 1"), Ok((0.0, 1.0)));
    }

    #[test]
    fn a_lens_blur_value_out_of_range_is_refused_stating_the_range() {
        for (flags, expected) in [
            ("--lb-f 0.5", "the f-number must be from 1 to 32"),
            ("--lb-f 40", "the f-number must be from 1 to 32"),
            ("--lb-f wide", "the f-number must be from 1 to 32"),
            ("--lb-focus 1.2,0.5", "two fractions from 0 to 1"),
            ("--lb-focus 0.5", "two fractions from 0 to 1"),
            ("--lb-focus 0.5,-0.1", "two fractions from 0 to 1"),
            // Leading with a hyphen is no reason to read it as another flag.
            ("--lb-focus -0.1,0.5", "two fractions from 0 to 1"),
        ] {
            let error = parse(&format!("opai photo.jpg --lb {flags} -o out.jpg")).expect_err(flags);

            assert_eq!(error.kind(), ErrorKind::ValueValidation, "{flags}: {error}");
            assert!(error.to_string().contains(expected), "{flags}: {error}");
        }
    }

    #[test]
    fn a_lens_blur_takes_a_blur_amount_from_0_to_100_instead_of_an_f_number() {
        let args = parse("opai photo.jpg --lb --lb-amount 50 -o out.jpg").expect("parses");
        assert_eq!((args.selection.lens_blur_amount, args.selection.lens_blur_f), (Some(50.0), None));

        for (text, expected) in [("0", 0.0), ("100", 100.0), (" 37.5 ", 37.5)] {
            assert_eq!(parse_blur_amount(text), Ok(expected), "{text}");
        }

        for (flags, expected) in [
            ("--lb-amount 101", "the blur amount must be from 0 to 100"),
            ("--lb-amount -1", "the blur amount must be from 0 to 100"),
            ("--lb-amount lots", "the blur amount must be from 0 to 100"),
        ] {
            let error = parse(&format!("opai photo.jpg --lb {flags} -o out.jpg")).expect_err(flags);

            assert!(error.to_string().contains(expected), "{flags}: {error}");
        }
    }

    /// The lens of a photograph taken at the equivalent f-number `own`.
    fn lens_at(own: f32) -> opai::OwnLens {
        opai::OwnLens { own_f_number: own, focal_35mm: None, origin: opai::LensOrigin::Exif, crop_factor: None }
    }

    /// The `--lb-f` the command line `line` comes to on a photograph with `lens`, and what the log is told.
    fn resolved(line: &str, lens: Option<opai::OwnLens>) -> (Option<f64>, Option<String>) {
        let (selection, note) = parse(line).expect("parses").selection.with_blur_amount_resolved(lens);
        assert_eq!(selection.lens_blur_amount, None, "{line}: the amount is spent");

        (selection.lens_blur_f, note)
    }

    #[test]
    fn a_blur_amount_comes_to_the_f_number_the_apps_slider_writes_for_it() {
        // The same numbers as the library's `a_blur_amount_runs_from_...` and the app's `blurAmount.test.ts`.
        let bonsai = Some(lens_at(6.0));
        for (amount, expected) in [(50, 2.27), (100, 1.4), (0, 6.0), (25, 3.294)] {
            let line = format!("opai photo.jpg --lb --lb-amount {amount} -o out.jpg");

            assert_eq!(resolved(&line, bonsai).0, Some(expected), "{line}");
        }
        assert_eq!(resolved("opai photo.jpg --lb --lb-amount 50 -o out.jpg", Some(lens_at(12.0))).0, Some(2.507));

        // The same amount is another aperture on another photograph, and the f/8 assumed where the lens is unknown.
        assert_eq!(resolved("opai photo.jpg --lb --lb-amount 50 -o out.jpg", None).0, Some(2.383));
    }

    #[test]
    fn a_resolved_blur_amount_says_what_it_came_to() {
        let (_, note) = resolved("opai photo.jpg --lb --lb-amount 50 -o out.jpg", Some(lens_at(6.0)));

        assert_eq!(
            note.as_deref(),
            Some("the lens blur's amount 50 is f/2.27 (full-frame equivalent), against the photograph's own f/6")
        );
    }

    #[test]
    fn a_blur_amount_on_a_photograph_as_wide_as_the_blur_goes_adds_nothing_and_says_so() {
        let (f_number, note) = resolved("opai photo.jpg --lb --lb-amount 100 -o out.jpg", Some(lens_at(1.2)));

        assert_eq!(f_number, Some(1.2));
        let note = note.expect("says so");
        assert!(note.contains("no effect") && note.contains("f/1.2"), "{note}");
    }

    #[test]
    fn a_selection_with_no_blur_amount_is_left_as_it_is() {
        let selection = parse("opai photo.jpg --lb --lb-f 2 -o out.jpg").expect("parses").selection;

        let (resolved, note) = selection.with_blur_amount_resolved(Some(lens_at(6.0)));

        assert_eq!((resolved, note), (selection, None));
    }

    #[test]
    fn a_blur_amount_and_an_f_number_are_two_ways_to_say_one_thing() {
        let error = parse("opai photo.jpg --lb --lb-f 2 --lb-amount 50 -o out.jpg").expect_err("both");

        assert_eq!(error.kind(), ErrorKind::ArgumentConflict, "{error}");
    }

    #[test]
    fn a_refused_focus_names_the_axis_it_was_refused_for() {
        assert!(parse_focus("1.2,0.5").expect_err("outside").contains("focus_x must be between 0 and 1"));
        assert!(parse_focus("0.5,-0.1").expect_err("outside").contains("focus_y must be between 0 and 1"));
    }

    #[test]
    fn the_lens_blurs_edges_are_matted_unless_the_depth_is_asked_for() {
        let args = parse("opai photo.jpg --lb -o out.jpg").expect("parses");
        assert_eq!(args.selection.lens_blur_edges, None);

        for (flag, edges) in [
            ("matte", LensBlurEdges::Matte),
            ("depth", LensBlurEdges::Depth),
            ("DEPTH", LensBlurEdges::Depth),
            ("solid", LensBlurEdges::Solid),
        ] {
            let args = parse(&format!("opai photo.jpg --lb --lb-edges {flag} -o out.jpg")).expect(flag);
            assert_eq!(args.selection.lens_blur_edges, Some(edges));
        }

        let error = parse("opai photo.jpg --lb --lb-edges hair -o out.jpg").expect_err("not an edge");
        assert_eq!(error.kind(), ErrorKind::InvalidValue, "{error}");
        assert_eq!(opai::LensBlurEdges::from(LensBlurEdges::default()), opai::LensBlurEdges::default());
    }

    #[test]
    fn the_lens_blur_flags_match_the_librarys_catalogue() {
        use opai::{Family, ParameterKind};

        let entry = opai::catalogue().iter().find(|entry| entry.family == Family::LensBlur).expect("published");
        let published: Vec<_> = entry.variants.iter().map(|variant| variant.codename).collect();
        let named: Vec<_> = LensBlurModel::value_variants()
            .iter()
            .map(|model| model.to_possible_value().expect("named").get_name().to_owned())
            .collect();
        assert_eq!(named, published);

        // The defaults the help states, as the library publishes them.
        let defaults: Vec<_> = entry.variants[0]
            .parameters
            .iter()
            .map(|parameter| match parameter.kind {
                ParameterKind::Range { min, max, default } => (parameter.name, min, max, default),
                ParameterKind::Faces | ParameterKind::Mask => panic!("{} is not a range", parameter.name),
            })
            .collect();
        assert_eq!(defaults, [("f_number", 1.0, 32.0, 2.8), ("focus_x", 0.0, 1.0, 0.5), ("focus_y", 0.0, 1.0, 0.5)]);
    }

    #[test]
    fn the_output_is_optional() {
        let args = parse("opai photo.jpg --ap").expect("no -o parses");

        assert_eq!(args.output, None);
        assert_eq!(args.suffix, None);
    }

    #[test]
    fn an_empty_suffix_is_read_as_given() {
        // Not through `parse`, which splits on whitespace and so cannot pass an empty argument.
        let args = parse_from(["opai", "photo.jpg", "--dn", "--suffix", "", "--overwrite"]).expect("parses");

        assert_eq!(args.suffix.as_deref(), Some(""));
        assert_eq!(args.output, None);
    }

    #[test]
    fn gui_needs_neither_an_output_nor_an_enhancement() {
        let args = parse("opai --gui a.jpg photos").expect("parses");

        assert!(args.gui);
        assert_eq!(args.inputs, ["a.jpg", "photos"].map(PathBuf::from));
        assert_eq!(args.output, None);
        assert!(args.ignored.is_empty(), "{:?}", args.ignored);

        let args = parse("opai photos --gui -r").expect("parses");
        assert!(args.gui && args.recursive);
        assert!(args.ignored.is_empty(), "-r is used by --gui: {:?}", args.ignored);
    }

    #[test]
    fn gui_still_needs_an_input() {
        let error = parse("opai --gui").expect_err("no input");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn gui_collects_every_other_option_given_in_order_by_its_long_name() {
        let args = parse(
            "opai a.jpg --gui --up osaka -q sd -p cpu --no-gui-settings --format png --prefix x --suffix y \
             --overwrite -o out --verbose --no-analytics --dn-str 50 --dn",
        )
        .expect("parses");

        assert_eq!(
            args.ignored,
            [
                "--upscale",
                "--quality",
                "--processor",
                "--no-gui-settings",
                "--format",
                "--prefix",
                "--suffix",
                "--overwrite",
                "--output",
                "--no-analytics",
                "--dn-str",
                "--denoise",
            ]
        );

        assert_eq!(parse("opai a.jpg --ap --gui").expect("parses").ignored, ["--autopilot"]);
    }

    #[test]
    fn gui_takes_a_lens_blur_but_not_its_edges() {
        let args = parse("opai a.jpg --gui --lb --lb-f 1.4 --lb-focus 0.3,0.6").expect("parses");

        assert!(args.ignored.is_empty(), "{:?}", args.ignored);
        assert_eq!(args.selection.lens_blur_f, Some(1.4));
        assert_eq!(args.selection.lens_blur_focus, Some((0.3, 0.6)));

        let args = parse("opai a.jpg --gui --lb --lb-amount 80 --lb-edges depth").expect("parses");
        assert_eq!(args.ignored, ["--lb-edges"]);
        assert_eq!(args.selection.lens_blur_amount, Some(80.0));
    }

    #[test]
    fn without_gui_nothing_is_collected() {
        assert!(parse("opai a.jpg --up -q sd -o out").expect("parses").ignored.is_empty());
    }

    #[test]
    fn help_lists_gui_and_its_usage() {
        let help = Args::command().term_width(10_000).max_term_width(10_000).render_help().to_string();

        assert!(help.contains("opai --gui <INPUT>... [-r]"), "{help}");
        assert!(help.contains("--gui"), "{help}");
        assert!(help.contains("Open the photographs in Open Photo AI instead of processing them"), "{help}");
    }

    #[test]
    fn a_bare_flag_before_another_flag_takes_its_default() {
        let args = parse("opai photo.jpg --up -o x").expect("parses");

        assert_eq!(args.inputs, [PathBuf::from("photo.jpg")]);
        assert_eq!(args.selection.upscale, Some(UpscaleModel::Tokyo));
        assert_eq!(args.output, Some(PathBuf::from("x")));
    }

    #[test]
    fn an_input_after_a_bare_flag_is_read_as_its_model() {
        let error = parse("opai --up photo.jpg -o out.png").expect_err("photo.jpg is not a model");

        assert_eq!(error.kind(), ErrorKind::InvalidValue);
        assert!(error.to_string().contains("photo.jpg"), "{error}");
    }

    #[test]
    fn several_inputs_are_taken_in_order() {
        let args = parse("opai a.jpg b.png scans --dn -o out").expect("parses");

        assert_eq!(args.inputs, ["a.jpg", "b.png", "scans"].map(PathBuf::from));
        assert!(!args.recursive && !args.overwrite);
        assert_eq!((args.format, args.prefix, args.suffix), (None, None, None));
    }

    #[test]
    fn an_input_is_required() {
        let error = parse("opai --up -o out").expect_err("no input");

        assert_eq!(error.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn the_batch_flags_are_read() {
        let args = parse("opai photos -r --up --prefix new- --suffix -opai --overwrite -o out").expect("parses");
        assert!(args.recursive && args.overwrite);
        assert_eq!(args.prefix.as_deref(), Some("new-"));
        assert_eq!(args.suffix.as_deref(), Some("-opai"));

        let args = parse("opai photos --recursive --up -o out").expect("parses");
        assert!(args.recursive);
    }

    #[test]
    fn every_format_spelling_and_alias_is_read_whatever_its_case() {
        for (word, choice) in [
            ("preserve", FormatChoice::Preserve),
            ("PRESERVE", FormatChoice::Preserve),
            ("avif", ExportFormat::Avif.into()),
            ("BMP", ExportFormat::Bmp.into()),
            ("gif", ExportFormat::Gif.into()),
            ("heic", ExportFormat::Heic.into()),
            ("HEIF", ExportFormat::Heic.into()),
            ("jpeg", ExportFormat::Jpeg.into()),
            ("JPG", ExportFormat::Jpeg.into()),
            ("Png", ExportFormat::Png.into()),
            ("tiff", ExportFormat::Tiff.into()),
            ("Tif", ExportFormat::Tiff.into()),
            ("WebP", ExportFormat::Webp.into()),
        ] {
            let args = parse(&format!("opai photos --up --format {word} -o out")).expect(word);
            assert_eq!(args.format, Some(choice), "{word}");
        }
    }

    #[test]
    fn an_unknown_format_is_refused_listing_the_accepted_ones() {
        let error = parse("opai photos --up --format raw -o out").expect_err("raw is not a format");

        assert_eq!(error.kind(), ErrorKind::ValueValidation);
        assert_eq!(error.exit_code(), 2);
        let message = error.to_string();
        for name in ["preserve", "avif", "bmp", "gif", "heic", "jpeg", "png", "tiff", "webp"] {
            assert!(message.contains(name), "{name} is not listed in: {message}");
        }
    }

    /// The processors this OS's build accepts, in the order `--help` lists them.
    #[cfg(target_os = "macos")]
    const PROCESSORS: [&str; 4] = ["auto", "cpu", "coreml", "webgpu"];
    #[cfg(not(target_os = "macos"))]
    const PROCESSORS: [&str; 5] = ["auto", "cpu", "cuda", "tensorrt", "webgpu"];

    /// The processors this OS's build has no value for.
    #[cfg(target_os = "macos")]
    const FOREIGN_PROCESSORS: [&str; 2] = ["cuda", "tensorrt"];
    #[cfg(not(target_os = "macos"))]
    const FOREIGN_PROCESSORS: [&str; 1] = ["coreml"];

    #[test]
    fn no_processor_is_auto() {
        let args = parse("opai photo.jpg --up -o out.png").expect("parses");

        assert_eq!(args.processor(), Processor::Auto);
    }

    #[test]
    fn a_processor_is_taken_whatever_its_case() {
        let args = parse("opai photo.jpg --up -p CPU -o out.png").expect("parses");
        assert_eq!(args.processor(), Processor::Cpu);

        let args = parse("opai photo.jpg --up --processor WebGPU -o out.png").expect("parses");
        assert_eq!(args.processor(), Processor::WebGpu);

        for name in PROCESSORS {
            let args = parse(&format!("opai photo.jpg --up -p {} -o out.png", name.to_uppercase())).expect(name);
            assert_eq!(args.processor().name(), name);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn coreml_is_a_processor_on_macos() {
        let args = parse("opai photo.jpg --up -p coreml -o out.png").expect("parses");

        assert_eq!(args.processor(), Processor::CoreMl);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn cuda_and_tensorrt_are_processors_off_macos() {
        let args = parse("opai photo.jpg --up -p cuda -o out.png").expect("parses");
        assert_eq!(args.processor(), Processor::Cuda);

        let args = parse("opai photo.jpg --up -p TensorRT -o out.png").expect("parses");
        assert_eq!(args.processor(), Processor::TensorRt);
    }

    #[test]
    fn a_processor_this_os_never_offers_is_an_unknown_value_listing_this_builds() {
        for name in FOREIGN_PROCESSORS {
            let error = parse(&format!("opai photo.jpg --up -p {name} -o out.png")).expect_err(name);

            assert_eq!(error.kind(), ErrorKind::InvalidValue, "{name}: {error}");
            assert_eq!(error.exit_code(), 2);
            let message = error.to_string();
            assert!(message.contains(&format!("[possible values: {}]", PROCESSORS.join(", "))), "{message}");
        }
    }

    #[test]
    fn help_groups_the_options_under_headings_in_order() {
        let help = Args::command().term_width(10_000).max_term_width(10_000).render_help().to_string();
        // By line, not by text: the usage line and the descriptions name flags too.
        let lines: Vec<_> = help.lines().map(str::trim_start).collect();
        let position = |text: &str| {
            let line = lines.iter().position(|line| line.starts_with(text) || line.contains(&format!(", {text}")));
            line.unwrap_or_else(|| panic!("{text} is not in the help: {help}"))
        };
        let order = [
            "Enhancements:",
            "--autopilot",
            "--denoise",
            "--dn-str",
            "--upscale",
            "--scale",
            "Processing:",
            "--quality",
            "--processor",
            "--no-gui-settings",
            "--recursive",
            "Output:",
            "--output",
            "--format",
            "--prefix",
            "--suffix",
            "--overwrite",
            "--verbose",
            "Options:",
            "--gui",
            "--no-analytics",
            "--help",
            "--version",
        ];

        let positions: Vec<_> = order.iter().map(|text| position(text)).collect();
        assert!(positions.is_sorted(), "{help}");
    }

    #[test]
    fn help_lists_exactly_this_builds_processors() {
        // Wide enough that nothing wraps, so a line holds a whole description whatever the terminal this runs in.
        let help = Args::command().term_width(10_000).max_term_width(10_000).render_help().to_string();
        let line = help.lines().find(|line| line.contains("--processor")).expect("--processor is in the help");

        assert!(line.trim_start().starts_with("-p, --processor <PROCESSOR>"), "{line}");
        assert!(help.contains(&format!("[possible values: {}]", PROCESSORS.join(", "))), "{help}");
        for name in FOREIGN_PROCESSORS {
            assert!(!help.contains(name), "{name} is in the help: {help}");
        }
    }

    #[test]
    fn every_processor_is_the_provider_of_the_same_name() {
        use opai::ExecutionProvider;

        let names: Vec<_> = Processor::value_variants().iter().map(|processor| processor.name()).collect();
        assert_eq!(names, PROCESSORS);

        for processor in Processor::value_variants() {
            let provider = ExecutionProvider::from(*processor);

            assert_eq!(provider.as_str().to_lowercase(), processor.name(), "{processor:?}");
            assert_eq!(Processor::from_provider(provider), Some(*processor));
        }
        for name in FOREIGN_PROCESSORS {
            let provider: ExecutionProvider = name.parse().expect("the library publishes it");
            assert_eq!(Processor::from_provider(provider), None, "{name}");
        }
    }

    /// Every `opai …` command in `docs/CLI_REFERENCE.md`, from its code blocks and its inline code, without trailing
    /// comments or the environment variables set before it.
    fn documented_commands() -> Vec<String> {
        let reference = include_str!("../../../docs/CLI_REFERENCE.md");
        let mut commands = Vec::new();
        let mut fenced = false;

        for line in reference.lines() {
            if line.trim_start().starts_with("```") {
                fenced = !fenced;
                continue;
            }

            let candidates: Vec<&str> =
                if fenced { vec![line.trim()] } else { line.split('`').skip(1).step_by(2).collect() };
            for candidate in candidates {
                let mut command = candidate.trim_start_matches("NO_COLOR=1 ");
                // Autopilot's model variables, as many as are set before the command.
                while let Some(rest) = command.strip_prefix("OPAI_AP_") {
                    command = rest.split_once(' ').map_or("", |(_, rest)| rest);
                }
                let command = command.split(" #").next().unwrap_or_default().trim();
                if command.starts_with("opai ") {
                    commands.push(command.to_owned());
                }
            }
        }

        commands
    }

    #[test]
    fn every_example_in_the_reference_parses() {
        // The synopsis, and the commands the reference shows failing or ending the parse on purpose.
        let not_runs = ["opai <INPUT>...", "opai --up photo.jpg", "opai --help", "opai --version"];

        let commands = documented_commands();
        assert!(commands.len() > 20, "the reference's examples were not found: {commands:?}");

        for command in commands {
            if not_runs.iter().any(|prefix| command.starts_with(prefix)) {
                continue;
            }
            if let Err(error) = parse(&command) {
                panic!("`{command}` does not parse: {error}");
            }
        }
    }

    #[test]
    fn verbose_is_off_unless_asked_for_and_works_with_autopilot() {
        assert!(!parse("opai photo.jpg --up").expect("parses").verbose);
        assert!(parse("opai photo.jpg --up --verbose").expect("parses").verbose);

        let args = parse("opai photo.jpg --ap --verbose").expect("parses");
        assert!(args.autopilot && args.verbose);
    }

    #[test]
    fn the_version_is_the_librarys() {
        let error = parse("opai --version").expect_err("--version ends the parse");

        assert_eq!(error.kind(), ErrorKind::DisplayVersion);
        assert_eq!(error.to_string().trim_end(), format!("Open Photo AI - CLI {}", opai::version()));

        let error = parse("opai -v").expect_err("-v ends the parse");

        assert_eq!(error.kind(), ErrorKind::DisplayVersion);
    }
}
