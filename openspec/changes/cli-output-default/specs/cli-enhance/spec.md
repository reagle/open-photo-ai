## ADDED Requirements

### Requirement: Without an output, each result is written beside its source

When `--output` is not given, each image's result SHALL be written into the directory that holds its source file,
whether the source was named as an input or found inside a directory input, with or without `--recursive`. No
directory SHALL be created, and no result SHALL be written into the current directory unless its source is there. The
result's name and format SHALL follow the same export naming rule as under an output directory, with the run's
`--format`, `--prefix` and `--suffix`.

When `--output` is not given and neither `--prefix` nor `--suffix` is given, the suffix SHALL be `_opai`. Giving
either one, an empty value included, SHALL replace this default with what was given, the other affix staying empty.

Numbering and `--overwrite` SHALL apply to each destination as they do under an output directory: without
`--overwrite` an existing file is kept and the result takes the first free `_1` ... `_999` name; with it, the file at
the destination is replaced once the new one is complete.

`--overwrite` SHALL NOT replace a file that is another input of the run, whether named or found in a directory input:
the file SHALL be kept, the result SHALL take the first free numbered name as without `--overwrite`, and the command
SHALL record this in its log file as a warning. An image's own source MAY be replaced, as described below.

When more than one image is written in a run without `--output`, the summary's success line SHALL read
`✨ <count> images enhanced successfully; files saved beside their sources`, in place of naming an output directory.
When exactly one is written, it SHALL name the file written, as with an output.

A source directory that cannot be written to SHALL fail that image as any other write failure does, and the run SHALL
continue with the next image.

#### Scenario: A photograph in the current directory

- **WHEN** the user runs `opai photo.jpg --up` in the directory holding `photo.jpg`, and `photo_opai.jpg` does not
  exist
- **THEN** the result is written to `photo_opai.jpg` in that directory, and `photo.jpg` is unchanged
- **AND** the run ends with `✨ Image enhanced successfully; file saved at photo_opai.jpg`

#### Scenario: A photograph elsewhere

- **WHEN** the user runs `opai pictures/beach.jpg --dn` from another directory
- **THEN** the result is written to `pictures/beach_opai.jpg`, and nothing is written into the current directory

#### Scenario: A tree of photographs

- **WHEN** `photos` holds `a.jpg` and `2024/b.jpg`, and the user runs `opai photos -r --up`
- **THEN** the results are `photos/a_opai.jpg` and `photos/2024/b_opai.jpg`, and no directory is created
- **AND** the run ends with `✨ 2 images enhanced successfully; files saved beside their sources`

#### Scenario: A suffix given

- **WHEN** the user runs `opai photo.jpg --up --suffix -big`
- **THEN** the result is written to `photo-big.jpg`

#### Scenario: A prefix given

- **WHEN** the user runs `opai photo.jpg --up --prefix new-`
- **THEN** the result is written to `new-photo.jpg`

#### Scenario: A format given

- **WHEN** the user runs `opai shot.nef --dn --format webp`
- **THEN** the result is written as WebP to `shot_opai.webp` beside `shot.nef`

#### Scenario: An earlier result is kept

- **WHEN** `photo_opai.jpg` already exists and the user runs `opai photo.jpg --up`
- **THEN** `photo_opai.jpg` is unchanged, and the result is written to `photo_opai_1.jpg`

#### Scenario: Overwrite replaces the earlier result, not the source

- **WHEN** `photo_opai.jpg` already exists and the user runs `opai photo.jpg --up --overwrite`
- **THEN** `photo_opai.jpg` is replaced by the new result, and `photo.jpg` is unchanged

#### Scenario: Overwrite does not replace another input

- **WHEN** `a.jpg` and `a_opai.jpg` both exist and the user runs `opai a.jpg a_opai.jpg --dn --overwrite`, or gives
  the two in the other order
- **THEN** `a_opai.jpg` is kept unchanged and read as an input, `a.jpg`'s result is written to `a_opai_1.jpg`, and
  `a_opai.jpg`'s to `a_opai_opai.jpg`

#### Scenario: A re-run over a tree with overwrite

- **WHEN** `photos` holds `a.jpg` and the earlier result `a_opai.jpg`, and the user runs
  `opai photos -r --dn --overwrite`
- **THEN** `a_opai.jpg` is kept, `a.jpg`'s new result is written to `a_opai_1.jpg`, and `a_opai.jpg` is enhanced into
  `a_opai_opai.jpg`

#### Scenario: In place, asked for explicitly

- **WHEN** the user runs `opai photo.jpg --dn --suffix "" --overwrite`
- **THEN** `photo.jpg` is replaced by its denoised result

#### Scenario: A source directory that cannot be written to

- **WHEN** the user runs `opai card/a.jpg home/b.jpg --up`, and `card` cannot be written to
- **THEN** `a.jpg` fails with a line naming the destination that could not be written, `home/b_opai.jpg` is written,
  and the run ends with the batch failure status

## MODIFIED Requirements

### Requirement: The command takes its inputs, its enhancements, and an output

The terminal application SHALL be invoked as `opai <input>... [options] [--output <output>]`. The inputs SHALL come
first: one or more paths, each a file or a directory, in any mix. The options SHALL be:

- `--upscale`, alias `--up`, with an optional model: `kyoto`, `osaka`, `tokyo` or `saitama`. With no model it SHALL
  mean `tokyo`.
- `--scale <factor>`: the upscale factor, from 1.0 to 8.0 inclusive. It SHALL be accepted only alongside `--upscale`.
- `--face-recovery`, alias `--fr`, with an optional model: `athens` or `santorini`. With no model it SHALL mean
  `athens`.
- `--denoise`, alias `--dn`, with an optional model: `stockholm`, `gothenburg` or `malmo`. With no model it SHALL mean
  `stockholm`.
- `--dn-str <strength>`: the denoise strength, a whole number from 1 to 300 inclusive, defaulting to 100. It SHALL be
  accepted only alongside `--denoise`.
- `--sharpen`, alias `--sh`, with an optional model: `moscow`, `petersburg` or `novgorod`. With no model it SHALL mean
  `moscow`.
- `--sh-str <strength>`: the sharpen strength, a whole number from 1 to 300 inclusive, defaulting to 100. It SHALL be
  accepted only alongside `--sharpen`.
- `--colorization`, alias `--cl`, with an optional model: `delhi`, `mumbai` or `jaipur`. With no model it SHALL mean
  `delhi`.
- `--light-adjustment`, alias `--la`, with an optional model: `paris` or `lyon`. With no model it SHALL mean `paris`.
- `--la-bias <bias>`: the light adjustment bias, a whole number from -100 to 100 inclusive, defaulting to 50. It SHALL
  be accepted only alongside `--light-adjustment`.
- `--color-balance`, alias `--cb`, with an optional model: `rio` or `saopaulo`. With no model it SHALL mean `rio`.
- `--cb-bias <bias>`: the colour balance bias, a whole number from -100 to 100 inclusive, defaulting to 50. It SHALL
  be accepted only alongside `--color-balance`.
- `--autopilot`, alias `--ap`: each image's enhancements are chosen by analysing it, as described in "Autopilot
  chooses each image's enhancements". It SHALL NOT be combined with any of the options above.
- `--quality`, alias `-q`: `hd` or `sd`, defaulting to `hd`.
- `--processor`, alias `-p`: the execution provider to run on, defaulting to `auto`. Its values are described in "The
  processor is chosen on the command line".
- `--recursive`, alias `-r`: a directory input contributes its whole tree rather than only the files directly in it.
- `--format <format>`: `preserve`, or one of `avif`, `bmp`, `gif`, `heic`, `jpeg`, `png`, `tiff` and `webp`, with
  `jpg`, `tif` and `heif` accepted as the same formats as `jpeg`, `tiff` and `heic`. It SHALL default to `preserve`.
  It SHALL be accepted whenever the output is not a file.
- `--prefix <text>` and `--suffix <text>`: written before and after each source's name. Both SHALL default to empty,
  and SHALL be accepted whenever the output is not a file.
- `--overwrite`: an existing file at a destination is replaced rather than kept.
- `--output`, alias `-o`: the file or directory to write to. It is optional: without it, each result is written beside
  its source, as described in "Without an output, each result is written beside its source".

A strength or bias SHALL be applied as that percentage of the model's own effect: a strength of 100 or a bias of 100
applies the model's output as it is, a strength of 200 applies twice that, and a negative bias shifts the image in the
opposite direction. A negative bias SHALL be accepted written as a plain number, as in `--la-bias -20`.

The order in which the enhancement flags are given SHALL NOT change the order they are applied in. That order is the
library's apply order: denoise, face recovery, colorization, light adjustment, colour balance, sharpen, and then
upscale. Without `--autopilot`, the same enhancements and parameters SHALL apply to every image in the run.

Model names, format names and processor names SHALL be matched without regard to case.

#### Scenario: Bare flags take their default models

- **WHEN** the user runs `opai photo.jpg --up --fr -o out.png`
- **THEN** the photograph is face-restored with Athens and upscaled with Tokyo, and written to `out.png`

#### Scenario: Bare new flags take their default models and parameters

- **WHEN** the user runs `opai photo.jpg --dn --sh --cl --la --cb -o out.png`
- **THEN** the photograph is denoised with Stockholm at strength 100, colorized with Delhi, light-adjusted with Paris
  at bias 50, colour-balanced with Rio at bias 50, and sharpened with Moscow at strength 100

#### Scenario: A named model

- **WHEN** the user runs `opai photo.jpg --up osaka -o out.png`
- **THEN** the photograph is upscaled with Osaka

#### Scenario: A named model for a new family

- **WHEN** the user runs `opai photo.jpg --cb SaoPaulo -o out.png`
- **THEN** the photograph is colour-balanced with São Paulo

#### Scenario: Each parameter applies to its own enhancement

- **WHEN** the user runs `opai photo.jpg --dn --dn-str 200 --sh --sh-str 80 -o out.png`
- **THEN** the photograph is denoised at strength 200 and sharpened at strength 80

#### Scenario: A negative bias

- **WHEN** the user runs `opai photo.jpg --la --la-bias -20 -o out.png`
- **THEN** the photograph is light-adjusted at bias -20

#### Scenario: Flag order does not change the result

- **WHEN** the user runs the same photograph once with `--up --fr` and once with `--fr --up`
- **THEN** both runs apply face recovery first and then the upscale, and produce the same image

#### Scenario: Flag order does not change the apply order of the new families

- **WHEN** the user runs `opai photo.jpg --up --sh --dn -o out.png`
- **THEN** the photograph is denoised first, then sharpened, then upscaled

#### Scenario: Several inputs

- **WHEN** the user runs `opai a.jpg b.png scans --dn -o out`
- **THEN** `a.jpg`, `b.png` and the images directly inside `scans` are each denoised and written under `out`

#### Scenario: No output given

- **WHEN** the user runs `opai --autopilot --quality sd --suffix _opai photo.jpg` in the directory holding `photo.jpg`
- **THEN** the command does not fail for want of `--output`, and the result is written to `photo_opai.jpg` in that
  directory

#### Scenario: A format alias

- **WHEN** the user runs `opai photos --up --format JPG -o out`
- **THEN** every image is written as JPEG with the extension `jpg`

#### Scenario: An unknown format

- **WHEN** the user runs `opai photos --up --format raw -o out`
- **THEN** the command fails with a usage error listing the accepted formats, and nothing is installed or written

#### Scenario: An unknown model

- **WHEN** the user runs `opai photo.jpg --up paris -o out.png`
- **THEN** the command fails with a usage error that lists the accepted upscale models, and nothing is installed or
  written

#### Scenario: A scale without an upscale

- **WHEN** the user runs `opai photo.jpg --fr --scale 2 -o out.png`
- **THEN** the command fails with a usage error saying `--scale` requires `--upscale`

#### Scenario: A scale out of range

- **WHEN** the user runs `opai photo.jpg --up --scale 9 -o out.png`
- **THEN** the command fails with a usage error naming the accepted range, 1.0 to 8.0

#### Scenario: A parameter without its enhancement

- **WHEN** the user runs `opai photo.jpg --sh --dn-str 150 -o out.png`
- **THEN** the command fails with a usage error saying `--dn-str` requires `--denoise`

#### Scenario: A strength out of range

- **WHEN** the user runs `opai photo.jpg --dn --dn-str 0 -o out.png` or `opai photo.jpg --dn --dn-str 301 -o out.png`
- **THEN** the command fails with a usage error naming the accepted range, 1 to 300

#### Scenario: A bias out of range

- **WHEN** the user runs `opai photo.jpg --cb --cb-bias 101 -o out.png`
- **THEN** the command fails with a usage error naming the accepted range, -100 to 100

#### Scenario: A fractional parameter

- **WHEN** the user runs `opai photo.jpg --dn --dn-str 150.5 -o out.png`
- **THEN** the command fails with a usage error saying the strength must be a whole number from 1 to 300

