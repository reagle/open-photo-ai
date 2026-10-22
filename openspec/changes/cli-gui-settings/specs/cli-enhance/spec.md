## ADDED Requirements

### Requirement: The command follows the GUI's saved settings

The command SHALL read, once at startup and after its arguments have parsed, the settings file the GUI publishes (see
the `gui-settings` capability, "The settings the command line can follow are published for it"), and SHALL take from
it only settings it already has a flag or variable for: the processor, the quality each family runs at, and under
`--autopilot` each family's model and whether it is switched off. Each value SHALL be decided by the first of these
that gives one:

| Setting | 1. Flag | 2. Variable | 3. GUI settings | 4. Default |
|---|---|---|---|---|
| Processor | `-p` | - | the GUI's processor | `auto` |
| Quality of a family | `-q`, for every family | - | the tier of the family's saved model | `hd` |
| Autopilot model of a family | - | `OPAI_AP_*` | the family's saved model, or `off` where Autopilot may not suggest it | the bare flag's model |

The model the bare enhancement flags (`--up`, `--dn`, …) run SHALL NOT come from the file.

With `--no-gui-settings`, the file SHALL NOT be read, and every setting SHALL be decided as if it did not exist.

The file SHALL never fail the run. A file that is absent SHALL be read as no settings, silently. A file that cannot be
read or parsed, or that declares a version the command does not know, SHALL be ignored as a whole, and the command
SHALL record a warning in its log file naming the file and why. Within a file it can read, an entry it does not
recognise, or one naming a family, model, precision or processor the library does not publish, SHALL be ignored on its
own and recorded as a warning, and every other entry SHALL still apply. The command SHALL record in its log file which
file it read.

The command SHALL NOT write the file.

#### Scenario: No file

- **WHEN** the GUI has never saved its settings, and the user runs `opai photo.jpg --ap -o out.png`
- **THEN** the run behaves exactly as without this requirement

#### Scenario: Settings followed

- **WHEN** the GUI's settings save every family at SD and exclude colour balance and colorization from Autopilot, and
  the user runs `opai photo.jpg --ap -o out.png`
- **THEN** the processor line reads `quality SD`, and neither colour balance nor colorization is suggested

#### Scenario: A flag and a variable override the file

- **WHEN** the same settings are saved, and the user runs `OPAI_AP_COLOR=rio opai photo.jpg --ap -q hd -o out.png`
- **THEN** every family runs at HD, colour balance may be suggested and runs with Rio, and colorization stays off

#### Scenario: Ignoring the file

- **WHEN** the same settings are saved, and the user runs `opai photo.jpg --ap --no-gui-settings -o out.png`
- **THEN** every family runs at HD with its default model, and every family may be suggested

#### Scenario: A damaged file

- **WHEN** the file holds text that is not a settings file, or names version 2, and the user runs
  `opai photo.jpg --up -o out.png`
- **THEN** the run behaves as with no file, ends as it would have, and the log file records a warning naming the file

#### Scenario: One entry the library does not publish

- **WHEN** the file saves a model named `lisbon` for upscale, and Tokyo for denoise, and excludes sharpen from
  Autopilot
- **THEN** upscale and denoise fall back to their defaults, each with a warning in the log file, and sharpen stays off

## MODIFIED Requirements

### Requirement: Quality selects the precision each model runs at

`hd` SHALL run every requested model at its higher-fidelity published build, and `sd` at its lighter one:

| Model | `hd` | `sd` |
|---|---|---|
| Kyoto, Tokyo, Saitama, Athens, Santorini | FP32 | FP16 |
| Stockholm, Gothenburg, Malmö | FP32 | FP16 |
| Moscow, Petersburg, Novgorod | FP32 | FP16 |
| Delhi, Mumbai, Jaipur | FP32 | FP16 |
| Paris, Lyon | FP32 | FP16 |
| Rio, São Paulo | FP32 | FP16 |
| Osaka | FP16 | INT8 |

The face detection that feeds a face recovery SHALL run at the precision the library prescribes for it, whatever the
quality.

A face recovery SHALL run at maximum fidelity to the original face.

`--quality` given SHALL apply to every family of the run. Where it is not given, each family SHALL run at the quality
of the model saved for that family in the GUI's settings, by its position among that model's published precisions (the
first is `hd`, the second `sd`), as described in "The command follows the GUI's saved settings". A family with no
model saved there SHALL run at `hd`. The families of one run MAY therefore run at different qualities.

#### Scenario: Default quality

- **WHEN** the user runs `opai photo.jpg --up -o out.png` and the GUI has saved no model for upscale
- **THEN** Tokyo runs at FP32

#### Scenario: Quality from the GUI's settings

- **WHEN** the GUI has saved Tokyo at SD as upscale's model and nothing for denoise, and the user runs
  `opai photo.jpg --dn --up -o out.png`
- **THEN** Stockholm runs at FP32 and Tokyo at FP16
- **AND WHEN** the user runs `opai photo.jpg --dn --up -q hd -o out.png`
- **THEN** both run at FP32

#### Scenario: Standard quality on Osaka

- **WHEN** the user runs `opai photo.jpg --up osaka -q sd -o out.png`
- **THEN** Osaka runs its INT8 build

#### Scenario: Standard quality on a new family

- **WHEN** the user runs `opai photo.jpg --dn gothenburg -q sd -o out.png`
- **THEN** Gothenburg runs its FP16 build

### Requirement: The processor is chosen on the command line

`--processor`, alias `-p`, SHALL choose the execution provider every model of the run is asked to run on, face
detection included. Its values SHALL be:

| Value | Provider | Accepted in builds for |
|---|---|---|
| `auto` (default) | the best provider this machine supports, trying TensorRT, CUDA, CoreML, WebGPU and then the CPU | every OS |
| `cpu` | the CPU | every OS |
| `coreml` | CoreML | macOS only |
| `cuda` | CUDA | Windows and Linux only |
| `tensorrt` | TensorRT | Windows and Linux only |
| `webgpu` | WebGPU | every OS |

A value that is not accepted by the build for the running OS SHALL be refused by the argument parser as an unknown
value: clap's own error, with no failure line, and the usage status. `--help` and the parser's list of possible values
SHALL name only the values the running build accepts.

An accepted value SHALL then be checked against the providers this machine offers once initialization has finished.
This is the same report the GUI builds its processor list from. `auto` and `cpu` SHALL always pass. A value this
machine does not offer SHALL be refused before any image is loaded. The refusal SHALL be one failure line naming the
processor asked for and the processors this machine offers, and the command SHALL exit with the usage status. Nothing
SHALL be written.

A processor that passes the check but fails to run a model SHALL fall back to the CPU for that model, as the library
does, and the run SHALL go on. The log file SHALL record the fallback. The terminal SHALL NOT report it.

Where `--processor` is not given, the processor SHALL be the one saved in the GUI's settings, as described in "The
command follows the GUI's saved settings", and `auto` where none is saved. A processor taken from the GUI's settings
SHALL NOT be refused: where this build cannot name it, or this machine does not offer it, the run SHALL use `auto` and
record a warning in its log file, as the GUI itself falls back on a machine that no longer offers its saved processor.

The processor SHALL NOT change what initialization installs. The installed dependencies SHALL be what this machine
needs, whichever processor is chosen.

#### Scenario: No processor given

- **WHEN** the user runs `opai photo.jpg --up -o out.png`, and the GUI has saved no processor
- **THEN** the run uses the best provider this machine supports, as it did before this flag existed

#### Scenario: The GUI's processor

- **WHEN** the GUI has saved `cpu`, and the user runs `opai photo.jpg --up -o out.png`
- **THEN** the run is asked to run on the CPU
- **AND WHEN** the user runs `opai photo.jpg --up -p auto -o out.png`
- **THEN** the run uses the best provider this machine supports

#### Scenario: A GUI processor this machine does not offer

- **WHEN** the GUI's settings name CUDA, and the user runs `opai photo.jpg --up -o out.png` on a Mac
- **THEN** the run uses `auto`, the log file records a warning naming the saved processor, and the run is not refused

#### Scenario: Forcing the CPU

- **WHEN** the user runs `opai photo.jpg --up -p cpu -o out.png` on a machine with an accelerator
- **THEN** every model of the run, face detection included, is asked to run on the CPU, and the log file records that
  the image ran on CPU

#### Scenario: Case does not matter

- **WHEN** the user runs `opai photo.jpg --up --processor CoreML -o out.png` on macOS
- **THEN** the run uses CoreML

#### Scenario: A processor this OS can never offer

- **WHEN** the user runs `opai photo.jpg --up -p cuda -o out.png` with a macOS build
- **THEN** the argument parser's own error is printed, listing the possible values `auto`, `cpu`, `coreml` and
  `webgpu`, with no failure line, and the exit status is 2
- **AND** nothing is installed

#### Scenario: A processor this machine does not offer

- **WHEN** the user runs `opai photo.jpg --up -p tensorrt -o out.png` on a Linux machine with no NVIDIA GPU, whose
  machine offers the CPU only
- **THEN** initialization finishes, and the command ends with one failure line naming `tensorrt` and the processors
  this machine offers, `auto` and `cpu`
- **AND** no image is loaded, nothing is written, and the exit status is 2

#### Scenario: Help lists only this build's processors

- **WHEN** the user runs `opai --help` with a Windows build
- **THEN** `--processor` lists `auto`, `cpu`, `cuda`, `tensorrt` and `webgpu`, and does not list `coreml`

### Requirement: Autopilot chooses each image's enhancements

With `--autopilot`, alias `--ap`, the command SHALL analyse each image after loading it, the same analysis the GUI's
Autopilot runs, and SHALL apply to that image the enhancements the analysis suggests for it, in the library's apply
order. Different images in one run MAY receive different enhancements.

`--autopilot` SHALL NOT be combined with any flag that chooses an enhancement or its parameters: `--upscale`,
`--face-recovery`, `--denoise`, `--sharpen`, `--colorization`, `--light-adjustment`, `--color-balance`, `--scale`,
`--dn-str`, `--sh-str`, `--la-bias` or `--cb-bias`. Giving any of them with `--autopilot` SHALL fail with a usage
error that names both flags and says they cannot be used together, and nothing SHALL be installed or written. Every
other option SHALL work with `--autopilot` as it does without it, and `--quality` SHALL select the precision of the
models autopilot runs as it does for the enhancement flags.

Each suggested enhancement SHALL run with its family's model as follows:

| Family | Environment variable | Default model |
|---|---|---|
| Denoise | `OPAI_AP_DENOISE` | Stockholm |
| Face recovery | `OPAI_AP_FACE` | Athens |
| Colorization | `OPAI_AP_COLORIZATION` | Delhi |
| Light adjustment | `OPAI_AP_LIGHT` | Paris |
| Colour balance | `OPAI_AP_COLOR` | Rio |
| Sharpen | `OPAI_AP_SHARPEN` | Moscow |
| Upscale | `OPAI_AP_UPSCALE` | Tokyo |

The default model SHALL be the one the family's enhancement flag runs when given with no model. Where the family's
variable is set to one of that family's model names, spelled as the enhancement flag accepts it and matched without
regard to case, that model SHALL run instead. Where it is set to `off`, matched without regard to case, that family
SHALL NOT be suggested for any image, and its signal SHALL NOT be read. A variable that is unset or set to the empty
string SHALL leave the GUI's choice in place, as described in "The command follows the GUI's saved settings": the
family SHALL be treated as `off` where the GUI's settings say Autopilot may not suggest it, and SHALL otherwise run
with the model saved for it there. Only where the GUI's settings say neither SHALL the default model apply.

A denoise or sharpen SHALL run at strength 100, a light adjustment or colour balance at bias 50, and an upscale at the
scale the analysis suggests for the image.

These variables SHALL be read only by the command line application and only when `--autopilot` is given. Without
`--autopilot`, they SHALL have no effect and SHALL NOT be checked. The GUI SHALL NOT read them.

An image for which the analysis suggests nothing SHALL still be written, with its content unchanged, as an image whose
only requested enhancement was a skipped upscale is.

When a signal of the analysis cannot be read, such as the face detector failing to install or open, the image SHALL
NOT fail: the enhancements suggested by the signals that were read SHALL run, and the command SHALL record the failure
in its log file as a warning. A Ctrl-C during the analysis SHALL stop the run as described in "A run can be stopped".

The command SHALL record in its log file, for each image, which enhancements the analysis suggested. This SHALL NOT be
printed to the terminal beyond what the image's row already shows, except that with `--verbose` the image's end SHALL
name the enhancements it received, as described in "Under --verbose, each image's end names the enhancements it
received".

#### Scenario: Different images get different enhancements

- **WHEN** the user runs `opai photos --ap -o out`, and `photos` holds a small grainy photograph and a large sharp one
  with a colour cast
- **THEN** the first is denoised and upscaled, the second is colour-balanced, and both are written under `out`

#### Scenario: Default models

- **WHEN** the user runs `opai small.jpg --ap -o out.png` with no `OPAI_AP_*` variable set and no GUI settings, and
  the analysis suggests an upscale
- **THEN** the upscale runs with Tokyo at FP32, at the suggested scale

#### Scenario: A model chosen through the environment

- **WHEN** the user runs `OPAI_AP_UPSCALE=Osaka opai small.jpg --ap -q sd -o out.png`, and the analysis suggests an
  upscale
- **THEN** the upscale runs with Osaka's INT8 build

#### Scenario: A family switched off

- **WHEN** the user runs `OPAI_AP_SHARPEN=off opai blurred.jpg --ap -o out.png`
- **THEN** no sharpen is applied, whatever the photograph's blur, and the other families are still suggested

#### Scenario: Families and models from the GUI's settings

- **WHEN** the GUI's settings exclude colour balance and colorization from Autopilot and save Osaka at SD for upscale,
  and the user runs `opai small.jpg --ap -o out.png` with no `OPAI_AP_*` variable set
- **THEN** neither colour balance nor colorization is suggested, and a suggested upscale runs Osaka's INT8 build

#### Scenario: A variable overrides the GUI's settings

- **WHEN** the GUI's settings exclude colour balance from Autopilot, and the user runs
  `OPAI_AP_COLOR=saopaulo opai cast.jpg --ap -o out.png`
- **THEN** colour balance may be suggested, and runs with São Paulo

#### Scenario: Combined with an enhancement flag

- **WHEN** the user runs `opai photo.jpg --ap --up -o out.png`
- **THEN** the command fails with a usage error saying `--autopilot` cannot be used with `--upscale`, and nothing is
  installed or written

#### Scenario: Combined with a parameter flag

- **WHEN** the user runs `opai photo.jpg --ap --dn-str 150 -o out.png`
- **THEN** the command fails with a usage error saying `--autopilot` cannot be used with `--dn-str`

#### Scenario: Variables ignored without autopilot

- **WHEN** the user runs `OPAI_AP_UPSCALE=paris opai photo.jpg --up -o out.png`
- **THEN** the variable is not checked, and the photograph is upscaled with Tokyo

#### Scenario: Nothing to do

- **WHEN** the user runs `opai large.jpg --ap -o out.png`, and the analysis suggests nothing
- **THEN** `out.png` is written with the photograph's content unchanged
- **AND** the command ends with its success line

#### Scenario: The face detector cannot be obtained

- **WHEN** the user runs `opai small-portrait.jpg --ap -o out.png`, and the face detection model cannot be installed
- **THEN** the image is still upscaled and written, without face recovery
- **AND** the log file records the failed detection as a warning

#### Scenario: What autopilot chose is shown

- **WHEN** the user runs `opai photo.jpg --ap -o out.png --verbose`, and the analysis suggests a light adjustment and
  a colour balance
- **THEN** the image's row ends
  `Enhancements applied to photo.jpg: Light Adjust Paris bias 50, Color Balance Rio bias 50`

### Requirement: Progress is shown while the command works

Everything the command shows SHALL be written to standard error. Standard output SHALL stay empty.

When standard error is a terminal, the command SHALL show, in this order:

1. **A dependency line**, only if initialization downloads or expands at least one dependency. It SHALL read
   `💾 Downloading dependency <name>... <mark> <percent>% Complete`. The name SHALL be the dependency's name as the GUI
   shows it (for example `ONNX Runtime`, `NVIDIA CUDA`, `NVIDIA TensorRT`). The percentage SHALL be a whole number
   covering that dependency's whole install, download and expansion together, and SHALL NOT show `100` before that
   install has finished. The mark SHALL be a braille spinner that keeps turning while the line is drawn, whether or
   not the percentage changes, and SHALL be `✓` while the percentage reads `100`. The line SHALL be rewritten in place
   as the install progresses, and when the next dependency starts, its name, spinner and percentage SHALL replace the
   previous ones. When initialization ends, the line SHALL remain as it was last drawn, which after a finished install
   is the last dependency's `✓ 100% Complete`. A dependency that was already installed SHALL NOT be shown. Where every
   dependency was already installed, there SHALL be no dependency line.
2. **The processor line**, once initialization has finished and the processor was accepted:
   `🧠 Running inference using the processor <processor>; quality <quality>`. The processor SHALL be the one asked for
   with `--processor`, spelled `Auto`, `CPU`, `CoreML`, `CUDA`, `TensorRT` or `WebGPU`; under `auto` it SHALL read
   `Auto`, not the provider the run resolves to. The quality SHALL be `HD` or `SD`, or `mixed` where the families of
   the run do not all run at one quality (see "Quality selects the precision each model runs at"). With `--verbose`,
   the processor and the quality SHALL each be followed by where the value came from, in parentheses:
   `(from -p)`/`(from -q)`, `(from GUI settings)` or `(default)`; a `mixed` quality SHALL list, inside them, each
   family the GUI's settings set and its quality, and the quality the rest run at. With `--verbose` and `--autopilot`,
   the processor line SHALL be followed by an autopilot line, `🤖 Autopilot models: <family> <model> (<source>), …`,
   naming, in the library's apply order, each family's model, or `off`, and where it came from: the variable's name,
   `from GUI settings`, or `default`.
3. A blank line.
4. **A live view of the batch**, drawn in place for the whole run, whatever the number of images, one included. It
   SHALL hold:
   1. **The batch's progress bar.** It SHALL be labelled `Enhancing` for the whole run. After the label, the line
      SHALL show a counter `[n/N]`, where `N` is the number of images in the run and `n` the number of images that
      have ended, written or failed, with `n` zero-padded to the width of `N`. The bar SHALL fill as `(n + p) / N`,
      where `p` is the progress of the image being worked on as its row shows it (see below), so that it moves forward
      within each image and never moves backwards, including when one image ends and the next starts. The line SHALL
      end with that progress as a percentage with one decimal place, which SHALL NOT show `100.0%` before the last
      image has ended. After the percentage, the line SHALL show an estimate of the time the batch has left,
      `ETA <time>`, at the rate the batch has run so far since its view started, kept current while the view is drawn
      even when no progress is reported. It SHALL read `ETA --` until at least 1% of the batch has run.
   2. A blank row.
   3. **The list.** One row per image started so far, in the order the images run, showing only the five most recent.
      Images not yet started SHALL NOT be listed. The view SHALL take two rows more than the list can hold: five list
      rows for a run of five or more images, otherwise one per image. Each row SHALL show the image's own progress as
      a whole percentage, right-aligned in three columns, as in `[  0%]`, `[ 42%]` and `[100%]`. The percentage SHALL
      cover the image's face detection, when there is one, and every operation of its chain, SHALL never decrease, and
      SHALL be held at `99%` until the image's result is written. A row SHALL read as follows, where `<path>` is the
      image's path as it was given or found:
      - `🖼️ [<p>%] Preparing to enhance <path>` before the image's run has reported anything, and for the whole time
        an image with no enhancement left to run is processed, because the only one requested was an upscale skipped
        as unnecessary or because autopilot suggested none
      - `🖼️ [<p>%] Choosing enhancements for <path>` with `--autopilot`, from when the image is loaded until its
        analysis has ended, at the percentage `0`
      - `🖼️ [<p>%] Downloading <family> for <path>` while an operation's model is being fetched
      - `🖼️ [<p>%] Applying <family> to <path>` while an operation runs or is served from the run cache, and after the
        last one until the result is written
      - `🖼️ [100%] Enhancements applied to <path>` once its result is written, whatever the number of enhancements,
        none included, followed with `--verbose` by `: <applied>`, as described in "Under --verbose, each image's end
        names the enhancements it received"
      - `🧨 [<p>%] Failed to enhance <path>` once it has failed, keeping the percentage it had reached
      - `🧨 [<p>%] Stopped enhancing <path>` when the run was stopped while working on it, keeping the percentage it
        had reached

      The words between the percentage and the path SHALL be padded with spaces to the width of the longest wording,
      `Downloading Face Recovery for`, so that every row's path starts in the same column, and at least one space
      SHALL separate them from the path. The plain lines SHALL be padded the same way.

      `<family>` SHALL be the enhancement's name in the GUI's words: `Denoise`, `Face Recovery`, `Colorize`,
      `Light Adjust`, `Color Balance`, `Sharpen` or `Upscale`. The face detection SHALL read as `Face Recovery`.
5. A blank line.
6. **The summary**, below the view, which SHALL remain as it was last drawn:
   1. When at least one image was written, a success line. If exactly one image was written, it SHALL read
      `✨ Image enhanced successfully; file saved at <file>`, naming the file actually written. If more than one was
      written, it SHALL read `✨ <count> images enhanced successfully; files saved under <output>`, naming the output
      directory as it was given.
   2. One failure line, `🧨 <reason>`, for each image that failed, in the order they ran. Each reason SHALL name the
      image.
   3. The closing failure line, when the run did not end in success (see "Exit status says how the run ended").

On a colour terminal, the dependency's name and its percentage SHALL be bold white, the spinner magenta and the `✓`
green. In the processor line, the processor SHALL be green and the quality yellow. The bar SHALL be drawn in a colour
gradient. In a `🖼️` row, the percentage, its `%` sign included, SHALL be blue, the enhancement's name, or the word
`Enhancements`, magenta, and the path cyan and underlined. A `🧨` row SHALL be red throughout. The success line SHALL
be bold yellow throughout. When the `NO_COLOR` environment variable is set to a non-empty value, the same lines SHALL
be drawn without colour.

When standard error is not a terminal, the command SHALL print, without colour, without a dependency line, view or
progress bar, and without any cursor-movement or redraw sequences:

1. The processor line, and with `--verbose` and `--autopilot` the autopilot line, followed by a blank line.
2. One line for each image, when that image ends, with the words its row ends with in the view, its percentage
   included. With `--verbose`, a written image's line SHALL end with `: <applied>` after the path, as its row does. A
   failed image's line SHALL add `: <reason>` after the path.
3. The summary's success line, when at least one image was written.
4. The closing failure line, when the run did not end in success.

The failure lines of the view's summary SHALL NOT be repeated in this mode, since each image's own line already
carries its reason.

When standard error is a terminal that cannot draw the live view, the command SHALL print the lines it would print
when standard error is not a terminal, coloured as on a colour terminal.

A processor refused by "The processor is chosen on the command line" SHALL end the command before the processor line
is printed.

The command SHALL run on the execution provider chosen with `--processor`, as "The processor is chosen on the command
line" describes. It SHALL record in its log file, for each image, the path written, the size and format of the image
written, and the provider or providers the run used. If every step was served from the run cache, it SHALL record that
instead. If no enhancement was left to run, it SHALL record that no enhancement ran, rather than that the run was
served from the cache. None of this SHALL be printed to the terminal; the processor line names only what was asked
for.

#### Scenario: First run on a new machine

- **WHEN** the command runs where the runtime is not yet installed, on a terminal
- **THEN** the first line shows `💾 Downloading dependency ONNX Runtime...` with a turning spinner and a rising
  `<n>% Complete`
- **AND** when initialization ends, that line remains as `💾 Downloading dependency ONNX Runtime... ✓ 100% Complete`,
  and the processor line follows directly under it

#### Scenario: Several dependencies downloaded

- **WHEN** initialization downloads the runtime and then a GPU library, on a terminal
- **THEN** the single dependency line names the runtime with its percentage, and is then rewritten to name the GPU
  library with its own percentage and a turning spinner

#### Scenario: Everything already installed

- **WHEN** the command runs where every dependency is already installed, on a terminal
- **THEN** no dependency line is shown, and the processor line is the first thing printed, followed by a blank line
  and the view

#### Scenario: The processor line names what was asked for

- **WHEN** the user runs `opai photo.jpg --up -q sd -o out.png` on a terminal
- **THEN** the processor line reads `🧠 Running inference using the processor Auto; quality SD`
- **AND WHEN** the user runs `opai photo.jpg --up -p cpu -o out.png`
- **THEN** it reads `🧠 Running inference using the processor CPU; quality HD`

#### Scenario: Where the settings came from

- **WHEN** the GUI's settings save every family's model at SD and exclude colour balance from Autopilot, and the user
  runs `opai photo.jpg --ap --verbose -o out.png` on a terminal with no `OPAI_AP_*` variable set
- **THEN** the processor line reads
  `🧠 Running inference using the processor Auto (from GUI settings); quality SD (from GUI settings)`
- **AND** the autopilot line names `Color Balance off (from GUI settings)`
- **AND WHEN** the user adds `-q hd`
- **THEN** the quality reads `HD (from -q)`

#### Scenario: A refused processor prints no processor line

- **WHEN** the user asks for a processor this machine does not offer
- **THEN** the command ends with its failure line, and no processor line is printed

#### Scenario: Face recovery and upscale

- **WHEN** the user runs `opai photo.jpg --fr --up -o out.png` on a terminal, and `out.png` does not exist
- **THEN** the view is three rows tall: the bar, a blank row, and the image's row
- **AND** the bar is labelled `Enhancing` with the counter `[0/1]`, and the row reads
  `🖼️ [  0%] Preparing to enhance photo.jpg`
- **AND** the row reads `Applying Face Recovery to photo.jpg` while the faces are found and restored, then
  `Applying Upscale to photo.jpg`, its percentage never decreasing and not passing `99%`
- **AND** once `out.png` is written, the row reads `🖼️ [100%] Enhancements applied to photo.jpg`, and the bar reads
  `[1/1]` at `100.0%`
- **AND** the run ends with `✨ Image enhanced successfully; file saved at out.png`

#### Scenario: A numbered result is named

- **WHEN** `out.png` already exists and the user runs `opai photo.jpg --up -o out.png` on a terminal
- **THEN** the run ends with `✨ Image enhanced successfully; file saved at out_1.png`

#### Scenario: The list scrolls

- **WHEN** a batch of eight images is run on a terminal, and the seventh is being enhanced
- **THEN** the list shows the rows of images 3 to 7, the first four of them ended and the last with its own
  percentage, and no row for image 8
- **AND** the bar's counter reads `[6/8]`

#### Scenario: The bar fills across the batch

- **WHEN** a batch of four images is run on a terminal, and the third image's row reads `[ 50%]`
- **THEN** the bar's counter reads `[2/4]` and its percentage is `62.5%`
- **AND** when the third image ends and the fourth starts, the bar does not move backwards

#### Scenario: The bar starts again for each image

- **WHEN** the first of two images finishes and the second starts, on a terminal
- **THEN** the bar does not start again: it reads `Enhancing` with `[1/2]` at `50.0%`, without having moved backwards
- **AND** the second image's row starts again from `🖼️ [  0%] Preparing to enhance <path>`

#### Scenario: An image with nothing to run

- **WHEN** the user runs `opai large.jpg --up -o out.png` on a terminal, and the upscale is skipped as unnecessary
- **THEN** the row reads `🖼️ [  0%] Preparing to enhance large.jpg` until the file is written
- **AND** then `🖼️ [100%] Enhancements applied to large.jpg`

#### Scenario: An image being analysed

- **WHEN** the user runs `opai photo.jpg --ap -o out.png` on a terminal
- **THEN** once the image is loaded, its row reads `🖼️ [  0%] Choosing enhancements for photo.jpg`, its path in the
  same column as every other wording's
- **AND** once the analysis has ended, the row moves on to `Downloading <family> for` or `Applying <family> to` for
  each suggested enhancement, or stays at `Preparing to enhance` when none was suggested, until the file is written

#### Scenario: A batch that succeeds

- **WHEN** the user runs `opai photos --up -o out`, and `photos` holds five images, all written
- **THEN** the bar ends at `[5/5]` and `100.0%`, and the run ends with
  `✨ 5 images enhanced successfully; files saved under out`

#### Scenario: A failure in a batch

- **WHEN** a batch of three images is run on a terminal, and the second fails while being written
- **THEN** its row reads `🧨 [ 99%] Failed to enhance <path>` in red, the bar's counter counts it as ended, and the
  third image still runs
- **AND** below the view, the run prints `✨ 2 images enhanced successfully; files saved under <output>`, then
  `🧨 <reason>` naming the second image, then `🧨 1 of 3 images failed`

#### Scenario: A failure that scrolled out of the list

- **WHEN** the first of eight images fails, on a terminal
- **THEN** its row is no longer shown once the sixth image starts
- **AND** its failure line is still printed below the view when the run ends

#### Scenario: Model downloaded during the run

- **WHEN** a requested model is not yet on disk
- **THEN** the image's row reads `🖼️ [<p>%] Downloading <family> for <path>` while the model is fetched, and then
  `Applying <family> to <path>` when it runs

#### Scenario: Paths line up in the list

- **WHEN** the list shows a written image, a failed one and one being upscaled, on a terminal
- **THEN** the rows read, with every path starting in the same column:
  ```
  🖼️ [100%] Enhancements applied to       photos/c.jpg
  🧨 [ 42%] Failed to enhance             photos/e.jpg
  🖼️ [ 42%] Applying Upscale to           photos/g.jpg
  ```

#### Scenario: Nothing printed outside the layout

- **WHEN** a `--fr --up` run finds faces and writes its output on a terminal
- **THEN** the terminal shows no face count, no provider the run resolved to, no image size and no format
- **AND** the log file records them, with the path written

#### Scenario: Output piped to a file

- **WHEN** standard error is redirected to a file, and a batch of three images runs on `auto` at `hd` where the second
  fails at 42%
- **THEN** the file holds `🧠 Running inference using the processor Auto; quality HD`, a blank line,
  `🖼️ [100%] Enhancements applied to <path>`, `🧨 [ 42%] Failed to enhance <path>: <reason>`,
  `🖼️ [100%] Enhancements applied to <path>`, the success line, and `🧨 1 of 3 images failed`, in that order
- **AND** it holds no colour, no dependency line, no progress bar and no redraw sequences

#### Scenario: Colours turned off

- **WHEN** the command runs on a terminal with `NO_COLOR=1`
- **THEN** the dependency line, the processor line, the view and the summary are drawn as usual, without colour, and
  the spinner still turns
