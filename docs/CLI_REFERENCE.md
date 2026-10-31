# CLI Reference

`opai` is the command-line version of **Open Photo AI**. It runs the same AI models as the GUI. One command can enhance a single image or a whole batch: any mix of files, wildcards and directories, written to one file, into an output directory, or beside each source.

- [Synopsis](#synopsis)
- [Quick start](#quick-start)
- [Arguments and global options](#arguments-and-global-options)
- [Where the results go](#where-the-results-go)
- [Enhancements](#enhancements)
- [Masks](#masks)
- [Remove](#remove)
- [Autopilot](#autopilot)
- [GUI settings](#gui-settings)
- [Processing order](#processing-order)
- [Supported formats](#supported-formats)
- [What happens during a run](#what-happens-during-a-run)
- [Terminal output](#terminal-output)
- [Stopping a run](#stopping-a-run)
- [Exit codes](#exit-codes)
- [Log file](#log-file)
- [Common errors](#common-errors)

## Synopsis

```bash
opai <INPUT>... <ENHANCEMENT FLAGS...> [OPTIONS] [--output <OUTPUT>]
opai --gui <INPUT>... [-r]
```

| Command          | What it does                                            |
|------------------|---------------------------------------------------------|
| `opai --help`    | Full help, with every flag, model and default (`-h` for a summary). |
| `opai --version` | Prints the version, e.g. `Open Photo AI - CLI 26.10.0`. (`-v` also works.) |

The rules in short:

- The `INPUT`s come **first**, before any flag. Each one is an image or a directory of images.
- You need **at least one** enhancement flag, or [`--autopilot`](#autopilot) to let `opai` choose them per image.
- `--output` / `-o` is **optional**. It's a [file or a directory](#where-the-results-go); without it, each result is written [beside its source](#no--o-beside-each-source), named `<name>_opai.<ext>`.
- With [`--gui`](#--gui), the images are opened in the GUI instead, and no enhancement is needed.
- The order of the enhancement flags doesn't matter. The enhancements always run in a [fixed order](#processing-order).
- An existing file is **never overwritten** unless you pass `--overwrite`. The result is [numbered](#numbering-and---overwrite) instead.

## Quick start

```bash
# Let autopilot enhance a photo, writing photo_opai.jpg beside it
opai photo.jpg --ap

# Upscale with the default model (Tokyo), at the scale the photo needs
opai photo.jpg --up -o photo-large.png

# Restore faces with the default model (Athens), then upscale 2x with Kyoto, and save as WebP
opai portrait.jpg --fr --up kyoto --scale 2 -o portrait.webp

# Restore with Osaka at SD quality, enlarging to twice the size
opai scan.png --rs osaka --rs-scale 2 -q sd -o scan.avif

# Denoise at twice the normal strength with Gothenburg, then adjust light with Paris
opai night.jpg --dn gothenburg --dn-str 200 --la --la-bias -20 -o night-clean.jpg

# Colorize an old black & white photo, restore its faces and upscale it
opai grandparents.tif --cl --fr --up -o grandparents-restored.png

# Denoise every JPEG in the current directory into the directory "enhanced"
opai *.jpg --dn -o enhanced

# Upscale a whole tree of photos, mirroring its folders, and save everything as WebP
opai photos -r --up --format webp -o photos-large

# Denoise a directory in place, replacing each photo with its result
opai photos --dn --overwrite -o photos

# Upscale on the CPU only, e.g. to rule out a GPU problem
opai photo.jpg --up -p cpu -o photo-large.png

# Let autopilot choose each photo's enhancements, as the GUI's Autopilot does
opai photos --ap -o enhanced

# Open a whole tree of photos in the GUI instead, to enhance them there
opai photos -r --gui

# Remove what a mask marks in white (a sign, a passer-by), then sharpen
opai street.jpg --rm passer-by.png --sh -o street-clean.jpg
```

## Arguments and global options

### `INPUT...` (required, positional)

One or more images or directories, in any mix. See [Supported formats](#supported-formats).

- **A file** is enhanced as it is. Its extension must be one `opai` reads, so a JPEG named `photo.dat` is rejected.
- **A directory** contributes every image directly inside it, or its whole tree with [`--recursive`](#-r---recursive). Only files whose extension (any case) is one `opai` reads are picked up; other files are ignored silently. Links to directories found inside it are not followed.
- **Wildcards** such as `*.jpg` work in every shell, including `cmd` and PowerShell on Windows, where `opai` expands them itself. A wildcard that matches nothing is kept as written, and fails as a missing input.
- **Order:** the images are enhanced in the order the inputs were given, and in name order within each directory.
- **Duplicates:** an image reached twice (named twice, named and also found in a directory, or found in two overlapping directories) is enhanced once, where it was first reached.

The inputs must come **before** the enhancement flags. Enhancement flags can take an optional model name, so the CLI reads the word after a flag as that model. For example, `opai --up photo.jpg -o out.png` fails because `photo.jpg` is read as an upscale model:

```
error: invalid value 'photo.jpg' for '--upscale [<MODEL>]'
  [possible values: kyoto, tokyo, saitama]
```

### `-o, --output <OUTPUT>`

Where the results go: one file, or a directory. Without it, each result is written beside its source. See [Where the results go](#where-the-results-go).

### `--gui`

Opens the images in the GUI instead of processing them, and ignores every other option but a sharpen and a lens blur. The inputs are expanded exactly as for a run, `-r` included, and every image found is added to the GUI's open images. If the GUI is already open, they are added to that window; if not, it is started first.

```
$ opai photos -r --gui
🖼️ Opening 12 images in Open Photo AI
```

Any other option you give is ignored, with a warning that names it, because the GUI uses its own settings. Set them there:

```
$ opai beach.jpg --up osaka -q sd -o out --gui
⚠️ --upscale, --quality and --output are ignored with --gui; set them in Open Photo AI instead
🖼️ Opening 1 image in Open Photo AI
```

The two exceptions are a sharpen and a lens blur. With [`--sharpen`](#sharpen---sharpen-model---sh-model), each image opens with a Sharpen of that model (Moscow for a bare `--sh`, as in a run) at `--sh-str`, or the default strength of 100 without it. Its precision is the one a run without `-q` would use: the quality (HD or SD) of the Sharpen model saved in the GUI's settings, or HD; `-q` itself is ignored with a warning. With [`--lens-blur`](#lens-blur---lens-blur-model---lb-model), each image opens with a Lens Blur at `--lb-f` or `--lb-amount` and `--lb-focus`; what is left out is the GUI's own default for a new Lens Blur, and `--lb-edges` and `--lb-refine` are ignored with a warning, since the GUI has no such setting (its Refine brush is painted in the window). Both may be given together, and each image then gets both, in the order a run applies them (the Sharpen first). Either one switches Autopilot off in the GUI, as a run refuses `--ap` with an enhancement, and no other enhancement is added.

```
$ opai beach.jpg --gui --lb --lb-f 1.4 --lb-focus 0.4,0.5
🖼️ Opening 1 image in Open Photo AI
$ opai beach.jpg --gui --sh novgorod --sh-str 150 --lb
🖼️ Opening 1 image in Open Photo AI
```

`--no-gui-settings` is ignored with a warning too, since the GUI always uses its own [settings](#gui-settings). `--verbose` is accepted without a warning and changes nothing.

`--gui` works while the GUI is open, unlike a normal run (see [One instance at a time](#one-instance-at-a-time)), and it sends no usage data. Options that can't be combined (such as `--ap` with an enhancement flag) are still refused, as they would be in a run.

On macOS the images are handed to the GUI with `open -b io.vinicius.opai`, so it must be installed where macOS can find it, for example in Applications. On Windows and Linux, `opai` starts the GUI's program (`OpenPhotoAI`), found beside `opai` or on your `PATH`.

### `-r, --recursive`

A directory input contributes its **whole tree**, not only the images directly inside it. The results [mirror](#mirroring-directory-inputs) the tree's folders.

### `--format <FORMAT>` (default: `preserve`)

The format every result is written in. Not accepted with an output **file**, whose extension chooses the format.

| Value | Result |
|-------|--------|
| `preserve` (default) | Each image keeps its own format and extension, written in lowercase (`a.JPG` gives `a.jpg`). Camera RAW (and anything else `opai` can't write) is saved as TIFF. An [Ultra HDR](#output-formats) JPEG stays one. |
| `avif`, `bmp`, `gif`, `heic`, `jpeg`, `png`, `tiff`, `webp` | Every result in this format. JPEG is saved with the extension `.jpg`. **`jpeg` is SDR:** an Ultra HDR input's gain map is left out, for the sites and apps that mishandle Ultra HDR. |
| `uhdr` | Every result as JPEG (`.jpg`) **keeping an HDR gain map**: an Ultra HDR JPEG wherever the input is one, and an ordinary JPEG wherever it is not. The GUI's *JPG (Ultra HDR)*. |

The value is case-insensitive, and `jpg`, `tif` and `heif` are accepted for `jpeg`, `tiff` and `heic`.

**Changed:** before `uhdr` existed, `--format jpeg` kept an input's gain map. It now means what the GUI's plain *JPG* means, an SDR JPEG, and says so for each such input (`written as SDR (gain map left out)`); use `uhdr` for the old behaviour. `preserve` and an output file ending in `.jpg` keep the gain map as before.

### `--prefix <TEXT>`, `--suffix <TEXT>`

Text added before and after each image's name, before the extension: `--prefix new- --suffix -opai` turns `beach.jpg` into `new-beach-opai.jpg`. Not accepted with an output **file**. Both are empty by default, except that [without `-o`](#no--o-beside-each-source) the suffix is `_opai` unless either one is given.

### `--overwrite`

Replaces a file that already exists at a destination instead of [numbering](#numbering-and---overwrite) the result.

### `--verbose`

Names, on each written image's line, the enhancements it received: each model, its strength, bias or upscale factor, and whether the [run cache](#run-cache) served them, as in `🖼️ [100%] Enhancements applied to photo.jpg: Denoise Stockholm strength 100, Upscale Tokyo 2x`. Without it, the line ends with the path. It has no short form: `-v` prints the version. See [Terminal output](#terminal-output).

### `--no-gui-settings`

Ignores the [GUI's saved settings](#gui-settings) for this run: every setting you don't give on the command line is the built-in default.

### `--autopilot`, `--ap`

Analyses each image and applies the enhancements it calls for, instead of the ones you name. It can't be combined with the enhancement flags or their options. See [Autopilot](#autopilot).

### `-q, --quality <hd|sd>` (default: `hd`, or the GUI's)

Picks which build of each model to run. It applies to **every** enhancement in the command. Without it, each enhancement runs at the quality of the model chosen for it in the [GUI's settings](#gui-settings), and at `hd` where none was chosen, so one run can mix `hd` and `sd`; the processor line then reads `quality mixed`.

| Quality      | Most models | Osaka (restore) | Trade-off                                     |
|--------------|-------------|-----------------|-----------------------------------------------|
| `hd` (default) | FP32      | FP16            | Highest quality. Slower, uses more memory.    |
| `sd`         | FP16        | INT8            | Lighter and usually faster. Slightly lower quality. |

The value is case-insensitive (`-q SD` works).

### `-p, --processor <PROCESSOR>` (default: `auto`, or the GUI's)

Picks the hardware every model of the run is asked to run on, face detection included. Without it, the processor chosen in the [GUI's settings](#gui-settings) is used, or `auto`. Which values exist depends on the operating system `opai` was built for:

| Processor        | Runs on                                                                              | macOS | Windows / Linux |
|------------------|--------------------------------------------------------------------------------------|:-----:|:---------------:|
| `auto` (default) | The best this machine supports. See [Hardware acceleration](#hardware-acceleration). | ✓     | ✓               |
| `cpu`            | The CPU only.                                                                        | ✓     | ✓               |
| `coreml`         | Apple's CoreML (macOS 12 or newer).                                                  | ✓     |                 |
| `cuda`           | NVIDIA CUDA.                                                                         |       | ✓               |
| `tensorrt`       | NVIDIA TensorRT.                                                                     |       | ✓               |
| `webgpu`         | WebGPU: the GPU through Metal, Direct3D 12 or Vulkan.                                | ✓     | ✓               |

The value is case-insensitive (`-p CPU` and `--processor WebGPU` work). `opai --help` lists only the values your build accepts, and any other value is refused straight away, before anything is installed.

A value your build accepts is then checked against what **this machine** offers, once [dependencies are installed](#what-happens-during-a-run). This is the same check the GUI uses to build its list of processors: CUDA needs a supported NVIDIA GPU and driver, TensorRT also needs a GPU that can run it, CoreML needs macOS 12 or newer, and WebGPU needs a GPU the runtime can reach. If the machine doesn't offer the processor, the run stops with one line naming the processors it does offer, for example `🧨 the processor webgpu is not available on this machine; use auto, cpu or coreml`, and exits with [code 2](#exit-codes). No image is loaded and nothing is written. A processor that comes from the [GUI's settings](#gui-settings) is never refused: if this build or this machine doesn't offer it, the run uses `auto` and the [log file](#log-file) records a warning.

## Where the results go

### A file or a directory

`-o` is read like this, in order:

1. **An existing directory** is a directory, whatever its name. `-o shots.png` writes into the directory `shots.png` if it exists.
2. **A path whose extension** (any case: `.png`, `.PNG`) is one `opai` [writes](#output-formats) is a **file**, saved in that format.
3. **Anything else** is a **directory**. It's created, with any missing parents, when the first result is written into it. `-o out` and `-o out.xyz` are both directories.

An output **file** takes exactly **one input file**: more than one input, or a directory input, fails with exit code `2`. So do `--format`, `--prefix` and `--suffix`, which are not accepted with an output file. The file's own directory must already exist.

An output **directory** fails with exit code `2` if a file (not a directory) already exists at that path.

To write into a new directory whose name ends in an image extension, create it first.

### Mirroring directory inputs

Under an output directory:

- **An image found in a directory input** is written at its path relative to that input. With `opai photos -r --up -o out`, `photos/a.jpg` becomes `out/a.jpg` and `photos/2024/summer/b.jpg` becomes `out/2024/summer/b.jpg`. Missing folders are created.
- **A file named as an input** goes directly into the output directory. `opai trips/2024/a.jpg home/b.jpg --up -o out` writes `out/a.jpg` and `out/b.jpg`.
- **The output directory is never taken as input.** `opai photos -r --up -o photos/enhanced` skips everything already in `photos/enhanced`, so a rerun doesn't enhance its own earlier results. (A file inside it that you name explicitly is still enhanced.)
- **In place:** with `-o` naming the input directory itself (`opai photos --dn -o photos`), each result lands beside its source.

The file name is the same one the GUI's export would use: the prefix, the source's name, the suffix, and the extension of the [`--format`](#--format-format-default-preserve) chosen.

### No `-o`: beside each source

Without `-o`, each result is written into the directory that holds its source, and no directory is created:

- **A file named as an input** gets its result beside it. `opai photo.jpg --ap` writes `photo_opai.jpg` in the current directory; `opai pictures/beach.jpg --dn` writes `pictures/beach_opai.jpg` and nothing in the current directory.
- **An image found in a directory input**, with or without `-r`, gets its result beside itself in that tree. `opai photos -r --up` writes `photos/a_opai.jpg` and `photos/2024/b_opai.jpg`.

The name follows the same [export rule](#--format-format-default-preserve) as under an output directory, with one difference: when neither `--prefix` nor `--suffix` is given, the suffix is **`_opai`**, because the result's name is the only thing that tells it from its source. Giving either one replaces that default, the other staying empty:

| Command | Result for `photo.jpg` |
|---------|------------------------|
| `opai photo.jpg --up` | `photo_opai.jpg` |
| `opai photo.jpg --up --suffix -big` | `photo-big.jpg` |
| `opai photo.jpg --up --prefix new-` | `new-photo.jpg` |
| `opai shot.nef --dn --format webp` | `shot_opai.webp` (for `shot.nef`) |
| `opai photo.jpg --dn --suffix "" --overwrite` | `photo.jpg`, replaced in place |

[Numbering](#numbering-and---overwrite) works as everywhere else: if `photo_opai.jpg` already exists, the result is `photo_opai_1.jpg`, and with `--overwrite` the earlier `photo_opai.jpg` is replaced. The source itself is only replaced when you ask for it twice: an empty `--suffix ""` (or a `--prefix`/`--suffix` that gives the source's own name) **and** `--overwrite`. `--overwrite` never replaces **another input** of the run: with `opai a.jpg a_opai.jpg --dn --overwrite`, `a.jpg`'s result would be `a_opai.jpg`, which you also gave, so it's kept and the result is numbered (`a_opai_1.jpg`) instead, with a warning in the [log file](#log-file). The same holds when a re-run over a directory finds its earlier `_opai` results.

A later run over the same directory sees the earlier `_opai` results as inputs too, just as it would with `-o` naming that directory, so `opai photos --ap` run twice also enhances `photos/a_opai.jpg` (into `photos/a_opai_opai.jpg`). Use `-o` to keep results apart.

A source directory that can't be written to (a read-only card or share) fails that image with a line naming the destination, and the run goes on with the next one; use `-o` to write elsewhere.

For more than one image, the summary line reads `✨ <count> images enhanced successfully; files saved beside their sources`. For one, it names the file written, as with `-o`.

### Numbering and `--overwrite`

**Without `--overwrite`**, a file that already exists at a destination is kept, and the result is written to the first free name made by adding `_1`, `_2`, … up to `_999` before the extension, exactly as the GUI's export does. This applies to an output file too: if `out.png` exists, `opai photo.jpg --up -o out.png` writes `out_1.png`. Two inputs with the same name going to the same place (`opai one/a.jpg two/a.jpg --up -o out`) give `out/a.jpg` and `out/a_1.jpg`. In place, each result is numbered beside its source (`photos/a_1.jpg`).

**With `--overwrite`**, the existing file is replaced, without a prompt. The new file is written under a hidden name beside it first, and only renamed over it once it's complete, so a failed or stopped write leaves the original exactly as it was and nothing beside it. This makes in-place enhancement safe: `opai photos --dn --overwrite -o photos` replaces each photo with its denoised result, in its own format. This works when the result's name is the source's own name: the name follows the [export rule](#--format-format-default-preserve), so the extension is written in lowercase and camera RAW becomes TIFF. A camera RAW source (`b.nef`) keeps its file and gets `b.tiff` beside it. An uppercase extension (`a.JPG`) is written as `a.jpg`, which replaces the source on a filesystem that ignores case (the macOS and Windows defaults) but sits beside it on one that doesn't (most Linux filesystems). With two inputs writing to the same name, the second result replaces the first.

For a single image, the summary line names the file actually written, including its number (`file saved at out_1.png`). For a batch, the [log file](#log-file) records each file written.

## Enhancements

Each enhancement is turned on by its flag. A flag with no model name uses that enhancement's **default model**. Add a model name after the flag to pick a different one:

```bash
opai photo.jpg --up -o out.png         # upscale with Tokyo (the default)
opai photo.jpg --up kyoto -o out.png   # upscale with Kyoto
```

- Every flag has a long name and a short alias; they do the same thing (`--upscale` = `--up`).
- Model names are case-insensitive (`--up KYOTO` = `--up kyoto`).
- Use plain ASCII for model names: `malmo` (not `malmö`), `petersburg` (not `st-petersburg`), `saopaulo` (not `sao-paulo` or `são-paulo`).
- You can combine any number of enhancements in one command, but each one only once.

### Summary

Default models are in **bold**.

| Enhancement      | Flag                 | Alias  | Models                                         | Extra options              |
|------------------|----------------------|--------|------------------------------------------------|----------------------------|
| Denoise          | `--denoise`          | `--dn` | **stockholm**, gothenburg, malmo               | `--dn-str` (1 to 300)      |
| Face Recovery    | `--face-recovery`    | `--fr` | **athens**, santorini                          | none                       |
| Colorization     | `--colorization`     | `--cl` | **delhi**, mumbai, jaipur                      | none                       |
| Light Adjustment | `--light-adjustment` | `--la` | **paris**, lyon                                | `--la-bias` (-100 to 100)  |
| Color Balance    | `--color-balance`    | `--cb` | **rio**, saopaulo                              | `--cb-bias` (-100 to 100)  |
| Sharpen          | `--sharpen`          | `--sh` | **moscow**, petersburg, novgorod               | `--sh-str` (1 to 300)      |
| Upscale          | `--upscale`          | `--up` | **tokyo**, kyoto, saitama                      | `--scale` (1.0 to 8.0)     |
| Restore          | `--restore`          | `--rs` | **kazan**, osaka                               | `--rs-scale` (1, 2 or 4), `--rs-str` (1 to 100) |
| Lens Blur        | `--lens-blur`        | `--lb` | **lisbon**                                     | `--lb-f` (1.0 to 32) or `--lb-amount` (0 to 100), `--lb-focus` (X,Y), `--lb-edges` (matte, depth), `--lb-refine` (FILE) |
| Grain            | `--grain`            | `--gr` | **sensor**                                     | `--gr-amount` (0 to 100), `--gr-add` |

`--mask <FILE>` limits whichever enhancements are asked for (or chosen by `--ap`) to part of the photograph, and under a lens blur keeps that part in focus; `--mask-use` narrows it to one of the two. See [Masks](#masks).

`--remove <FILE>` (alias `--rm`) removes the part of the photograph a mask image marks in white, filling it from its surroundings, before any other enhancement. It counts as an enhancement on its own. See [Remove](#remove).

The sections below explain each enhancement briefly. For more detail on when to pick each model, see the [Enhancements section of the README](../README.md#-enhancements).

### Strength and bias

Four enhancements take an extra option that controls how strong the effect is. Each option only works together with its own enhancement flag. For example, `--dn-str` without `--dn` fails with:

```
error: the following required arguments were not provided:
  --denoise [<MODEL>]
```

There are two kinds:

**Strength** (`--dn-str`, `--sh-str`, `--rs-str`): how much of the model's effect to apply. Restore's stops at `100`: a generative result is only ever blended back towards the image, never pushed past what the model drew.

| Value          | Effect                                                          |
|----------------|-----------------------------------------------------------------|
| `1`–`99`       | Weaker: the result is blended back towards the original image.  |
| `100` (default) | The model's own result.                                        |
| `101`–`300`    | Stronger: the effect is pushed past what the model produced.    |

**Bias** (`--la-bias`, `--cb-bias`): which direction to adjust in, and how far.

| Value           | Effect                                                |
|-----------------|-------------------------------------------------------|
| `-100`–`-1`     | Adjusts in the **opposite** direction to the model.   |
| `0`             | No change. The image stays as it was.                 |
| `50` (default)  | Half of the model's correction.                       |
| `100`           | The model's full correction.                          |

Both must be **whole numbers**. Values like `150.5` or `0.5` are rejected. A strength of `0` is also rejected, because it would have no effect; to skip an enhancement, leave its flag out.

### Denoise: `--denoise [MODEL]`, `--dn [MODEL]`

Removes noise from the image.

| Model         | Use it for                                                                           |
|---------------|--------------------------------------------------------------------------------------|
| `stockholm` (default) | Fast, high-quality removal of real camera sensor noise.                      |
| `gothenburg`  | Best quality on real-world sensor noise (low light, high ISO).                       |
| `malmo`       | Removing rain streaks from outdoor photos.                                           |

Option: `--dn-str <STRENGTH>`, a whole number from 1 to 300 (default 100). See [Strength and bias](#strength-and-bias).

### Face Recovery: `--face-recovery [MODEL]`, `--fr [MODEL]`

Finds the faces in the image and restores them.

| Model        | Use it for                                                                                   |
|--------------|----------------------------------------------------------------------------------------------|
| `athens` (default) | Keeping the person's identity. Runs at maximum fidelity to the original face.          |
| `santorini`  | Sharper, more polished faces. Can change the person's look on very low-quality inputs.       |

How it works:

1. Faces are detected automatically. You can't choose which faces to restore.
2. Only faces up to the size of a 512 × 512 pixel square (measured by area) are restored. Larger faces already have more detail than the model can add, and restoring them would soften them.
3. If no faces are found, or face detection fails, the rest of the run continues normally and the faces are left unchanged. The [log file](#log-file) records how many faces were found and restored.

No extra options.

### Colorization: `--colorization [MODEL]`, `--cl [MODEL]`

Adds color to black & white photos.

| Model        | Use it for                                                                      |
|--------------|---------------------------------------------------------------------------------|
| `delhi` (default) | Most photos. Rich, confident, lifelike colors.                              |
| `mumbai`     | Softer, more vintage-looking colors.                                            |
| `jaipur`     | Difficult photos where consistency matters. More muted, but few odd color patches. |

No extra options.

### Light Adjustment: `--light-adjustment [MODEL]`, `--la [MODEL]`

Fixes exposure and lighting.

| Model     | Use it for                                                                                  |
|-----------|---------------------------------------------------------------------------------------------|
| `paris` (default) | Uneven light: night scenes, backlit subjects, faces in shadow. Very fast.           |
| `lyon`    | Photos that are too bright or too dark overall.                                             |

Option: `--la-bias <BIAS>`, a whole number from -100 to 100 (default 50). Negative values work as you'd expect, e.g. `--la-bias -20`. See [Strength and bias](#strength-and-bias).

### Color Balance: `--color-balance [MODEL]`, `--cb [MODEL]`

Corrects white balance and color casts.

| Model      | Use it for                                                                                     |
|------------|------------------------------------------------------------------------------------------------|
| `rio` (default) | A color cast over the whole photo (orange indoors, blue in shade). The faster of the two. |
| `saopaulo` | Mixed light in one photo, like a warm lamp and daylight from a window.                         |

Option: `--cb-bias <BIAS>`, a whole number from -100 to 100 (default 50). See [Strength and bias](#strength-and-bias).

### Sharpen: `--sharpen [MODEL]`, `--sh [MODEL]`

Removes blur.

| Model        | Use it for                                                                     |
|--------------|--------------------------------------------------------------------------------|
| `moscow` (default) | Out-of-focus blur (the lens focused on the wrong thing).                 |
| `petersburg` | Motion blur and camera shake, fast and light on resources.                     |
| `novgorod`   | Motion blur and camera shake, best quality but slower.                         |

Option: `--sh-str <STRENGTH>`, a whole number from 1 to 300 (default 100). See [Strength and bias](#strength-and-bias).

### Upscale: `--upscale [MODEL]`, `--up [MODEL]`

Increases the image's resolution.

| Model      | Use it for                                                                                        |
|------------|---------------------------------------------------------------------------------------------------|
| `tokyo` (default) | A natural upscale that stays faithful to the original and doesn't invent detail.           |
| `kyoto`    | Real-world photos with noise, blur or compression. Fast, robust results.                          |
| `saitama`  | Cartoons, drawings, line art and digital illustrations.                                           |

Osaka was an upscale model and is now a [Restore](#restore---restore-model---rs-model) model: `--up osaka` fails with `error: Osaka is a restore model now: use --restore osaka (--rs osaka) …`. Use `--rs osaka --rs-scale 2` (or `4`) for what `--up osaka --scale 2` did.

Option: `--scale <FACTOR>`, the upscale factor, from `1.0` to `8.0`. Decimals are allowed (e.g. `2.5`).

**Without `--scale`**, the factor is chosen from the input image's size:

| Input size (width × height)              | Factor     |
|------------------------------------------|------------|
| Up to 1,048,576 pixels (1 MP, e.g. 1024 × 1024) | 4×  |
| Up to 4,194,304 pixels (4 MP, e.g. 2048 × 2048) | 2×  |
| Above 4 MP                                | No upscale |

> [!IMPORTANT]
> If the image is larger than 4 MP and you don't pass `--scale`, the upscale step is **skipped**. The terminal doesn't warn about this; it's only recorded in the [log file](#log-file). Any other enhancements in the command still run, and the file is still written at the original resolution. To upscale a large image anyway, pass `--scale`.

### Restore: `--restore [MODEL]`, `--rs [MODEL]`

Restores detail generatively: the model **redraws** plausible detail where the photo lost it, rather than recovering what was there. Use it when the faithful models can't do enough, such as an out-of-focus face or a photo from an old camera that was never sharp. It can change faces, textures and lettering, so lower `--rs-str` if the result looks invented. Restore runs after every other enhancement but [Grain](#grain---grain-model---gr-model), which you can add after it to give the restored photo its own noise back.

| Model      | Use it for                                                                                        |
|------------|---------------------------------------------------------------------------------------------------|
| `kazan` (default) | Soft, blurred or low-detail photos. PiSA-SR on Stable Diffusion 2.1; slow and a large download. Its builds are FP32 (`hd`) and FP16 (`sd`). |
| `osaka`    | The furthest-reaching restoration, on photos that are soft or heavily compressed. SeedVR2; slow, and needs a powerful GPU and a lot of memory (see the README). Its builds are FP16 (`hd`) and INT8 (`sd`). |

Options:

- `--rs-scale <FACTOR>`, the size to restore at: `1` (the photo's own size, the default), `2` or `4`. Above `1` the photo is enlarged first and restored at the new size, so it **cannot be combined with `--up`**: use one or the other. At `1` it can follow an upscale, and then restores the upscaled image.
- `--rs-str <STRENGTH>`, a whole number from 1 to 100 (default 100). See [Strength and bias](#strength-and-bias).

```bash
# Restore a soft photo at its own size, then at twice the size with a little less of the model's redraw
opai soft.jpg --rs -o soft-restored.png
opai soft.jpg --rs --rs-scale 2 --rs-str 70 -o soft-2x.png
```

### Lens Blur: `--lens-blur [MODEL]`, `--lb [MODEL]`

Gives the photo the shallow depth of field of a wider lens aperture: the point you focus on stays sharp, and everything nearer or farther is blurred as much as a real lens would blur it, with bright points spreading into discs. The model estimates how far from the camera every pixel is, and the blur follows that depth. Lens Blur runs after any upscale or restore, with only [Grain](#grain---grain-model---gr-model) after it.

| Model      | Use it for                                                                                        |
|------------|---------------------------------------------------------------------------------------------------|
| `lisbon` (default) | Any photo. Lisbon is a depth model (MoGe-2 ViT-B), two graphs of 324 MB, one per orientation; on a Mac its first use compiles the graph for about a minute. It is published at FP16 only, so `-q hd` and `-q sd` both run it. |

Options:

- `--lb-f <F>`, the f-number to simulate, from `1.0` to `32`, written bare (`1.4`) or with its prefix (`f/1.4`); default `2.8`. Smaller is a wider aperture and more blur.
- `--lb-amount <AMOUNT>`, how much blur to add, from `0` to `100`, instead of `--lb-f`: the Blur amount slider of the app. `0` is the photo as taken (nothing is added) and `100` is f/1.4 full-frame equivalent, the widest the blur goes; `50` is halfway. The amount is laid out evenly in blur (in `1/f-number`) between the photo's own aperture and f/1.4, so the same amount gives the same look on photos of any aperture, and is the f-number the app's slider would write: the log file says which one (`the lens blur's amount 50 is f/2.27 (full-frame equivalent), against the photograph's own f/6`). A photo already taken at f/1.4 equivalent or wider cannot be blurred more, so the amount has no effect on it. Giving both `--lb-amount` and `--lb-f` is refused.
- `--lb-focus <X,Y>`, the point to keep sharp, as two fractions of the photo's width and height from `0` to `1`: `0,0` is the top-left corner, `0.5,0.5` the centre (the default), `0.6,0.3` is 60% across and 30% down. To find one, hover over the subject in an image viewer that shows the cursor's pixel position and divide by the photo's width and height.
- `--lb-edges <EDGES>`, how the edge of the subject in focus is made: `matte` (the default) or `depth`. See below.
- `--lb-refine <FILE>`, a brush image that corrects what is kept sharp and what is blurred, as the app's Refine brush does: white is kept sharp, black is blurred, mid-grey (128) is left as it is. See below.

**The f-number is the 35 mm full-frame equivalent**: the lens's own f-number times the camera's crop factor, so `--lb-f 1.4` gives the same look on a phone photo as on a camera's. A photo already has the blur of the aperture it was taken at, so only the difference is added: at or above the photo's own equivalent f-number (a phone photo is typically around f/6 to f/10 equivalent), the photo is **left unchanged**. `--lb-amount` is the same aperture said as a percentage of the way from the photo's own to f/1.4.

How much blur, in one paragraph: a thin lens of the photo's 35 mm-equivalent focal length, focused at the depth found at the focus point, turns every other point into a disc whose size grows with its distance in front of or behind that depth (in `1/distance`), and shrinks as the f-number grows; the disc is measured against the full-frame diagonal, so it is the same fraction of the photo at any resolution. The blur the photo already has is subtracted in quadrature. The depth is sharpened along the photo's own edges so hair and leaves keep their outline.

**The lens is read from the photo's EXIF**: the focal length, its 35 mm equivalent, the f-number and the sensor's focal-plane resolution. The depth model reads distances to suit its own estimate of the field of view, so where the EXIF gives the focal length and it differs, every distance is scaled by the one over the other (the same object the same size in the photo is twice as far at twice the focal length), held to between a quarter and eight times. Where the focal length is missing, the depth model's own estimate of the field of view is used; where the f-number or the crop factor is missing, the photo is assumed to have been taken at **f/8 equivalent**. Which lens was used is recorded in the [log file](#log-file). EXIF is read from JPEG, TIFF, PNG, WebP, HEIF and the TIFF-based RAW formats (NEF, ARW, DNG, ORF, PEF and others); CR3 and RAF are not TIFF-based, so their lens is estimated or assumed.

The depth is estimated once per photo (and per crop) and kept in the [run cache](#run-cache), so a second run with another `--lb-f`, `--lb-amount` or `--lb-focus`, or with other enhancements before it, does not estimate it again. Where the EXIF alone shows that `--lb-f` is at or above the photo's own equivalent f-number, the photo is left unchanged without estimating the depth at all.

**The subject's edges are matted** (`--lb-edges matte`, the default). A depth map cannot say that an edge pixel is half a strand of hair and half the street behind it, so on its own it either keeps such pixels sharp, background and all, or blurs the strand's colour into the background as a dark haze. Instead, the subject in focus (everything from a little nearer than the focus point to half as far again behind it) is matted by a second model, **Porto** (ViTMatte-S, 104 MB), over a band along its edge; the background is filled in behind the subject and blurred, and the subject is laid over it, so hair, fur and leaf edges stay sharp with the background between them blurred. Everything within that range of distances is kept sharp, as one subject. The matte costs about 15 s for a new focus point (a few seconds to load the model, then about 0.5 s per 1024-pixel tile on a Mac's GPU or 0.8 s on the CPU, 15 to 25 tiles on a 12 to 24 MP photo) and is kept in the run cache afterwards, so it is computed once per photo and focus point: focus points within about 2.5 percent of the same distance share one, another `--lb-f` reuses it, and another `--lb-focus` on something else mattes again. It is skipped where the blur is under 3 pixels everywhere. `--lb-edges depth` renders from the depth alone, as before matting existed, for comparison.

**Refining the edges** (`--lb-refine <FILE>`). Where the depth and the matte still get part of the photo wrong (a few stray hairs blurred into the sky, a patch of background at the subject's distance kept sharp), a brush image puts it right, as the app's Refine brush does. Paint it at the photo's size or any size of the same shape (within 1%), starting from mid-grey (128): **white** is kept sharp, where the trimap had called it background it is matted by Porto, and the matte is raised to at least the white's share so a faint hair stays sharp whatever Porto answers; **black** is background, matted out and blurred at least as much as the farthest of the scene; grey between is a share of either, so a soft brush feathers. Its luminance is read and any transparency is ignored. The matte for a new image is computed again (the depth and Bilbao's matte come from the run cache).

Porto's graph is not yet published with the other models. Where it is not installed, the lens blur renders from the depth alone and the [log file](#log-file) notes it once; where it is installed and fails, the log file records a warning and that image's lens blur renders from the depth alone.

```bash
# Blur the background behind a subject 60% across and 30% down, at f/1.4
opai portrait.jpg --lb --lb-f f/1.4 --lb-focus 0.6,0.3 -o portrait-f1.4.jpg

# Half the blur the lens goes to, whatever aperture the photo was taken at
opai portrait.jpg --lb --lb-amount 50 -o portrait-half.jpg

# Denoise, upscale 2x, then blur at f/2.8 focused on the centre
opai photo.jpg --dn --up --scale 2 --lb -o photo-blurred.png

# The same blur with the edges left to the depth alone, to compare
opai portrait.jpg --lb --lb-f f/1.4 --lb-focus 0.6,0.3 --lb-edges depth -o portrait-f1.4-depth.jpg

# The same blur with stray hairs painted white (kept sharp) and a wrongly sharp patch painted black, on mid-grey
opai portrait.jpg --lb --lb-f f/1.4 --lb-focus 0.6,0.3 --lb-refine portrait-refine.png -o portrait-f1.4-refined.jpg
```

### Grain: `--grain [MODEL]`, `--gr [MODEL]`

Adds grain, **last**, after every other enhancement, the lens blur included: a real photo's blurred background still has its sensor's noise. A denoise, a restore or a sharpen can leave a photo smoother than any camera records ("plastic" skin and skies); by default the grain puts back the photo's **own** sensor noise, measured per brightness on the original, only where the enhancements smoothed it away, so a clean photo gets none and a grainy one gets its own back. If the command also denoises (`--dn`), the noise the denoise **left** is matched instead, so the grain never undoes the denoise. Grain runs no model and needs no download. On an upscaled photo it's drawn at the new pixels, finer than an enlarged copy of the original's grain.

| Model      | Use it for                                                       |
|------------|------------------------------------------------------------------|
| `sensor` (default) | A digital sensor's noise: mostly brightness, a little colour, slightly coarser than a pixel. |

Options:

- `--gr-amount <AMOUNT>`, a whole number from 0 to 100 (default 100): how much of the missing noise is put back. `50` puts back half of it.
- `--gr-add`: add grain everywhere, whatever noise the photo has, instead of matching the photo's own. `--gr-amount` is then a share of a high-ISO sensor's grain (about 4 levels of fine noise at mid-grey at `100`), as Lightroom's Grain Amount is.

```bash
# Restore a soft photo and give it its own noise back
opai soft.jpg --rs --gr -o soft-restored.png
# Add a little grain on purpose
opai portrait.jpg --gr --gr-add --gr-amount 30 -o portrait-grain.png
```

## Masks

`--mask <FILE>` applies the enhancements only where a mask image is white, and leaves the photograph as it was where the mask is black:

```bash
# Sharpen the subject only, with a mask that is white over the subject and black elsewhere
opai photo.jpg --sh --mask subject.png -o out.jpg

# Denoise a whole directory through one mask, e.g. a fixed camera's sky
opai photos --dn --mask sky.png -o enhanced
```

- **White applies, black keeps, grey mixes.** The mask's brightness (its luminance) is the weight: a pixel 25% grey gets a quarter of the enhancement. Soft edges in the mask give soft transitions in the result. A 16-bit mask keeps its 16 bits.
- **Transparency is ignored.** Only the mask's colour is read; a PNG with an alpha channel is read as if it were opaque. Flatten it first if the transparency is what you meant.
- **Any format `opai` reads**, oriented by its EXIF like a photograph. It's resized to each photograph's size, so it needn't be the same resolution, only the same shape.
- **The shapes must match.** A photograph whose aspect ratio differs from the mask's by more than 1% is refused, naming both ratios, and the rest of the batch still runs. With several inputs, one mask applies to each.
- **It needs an enhancement**, or `--ap`, which it then limits to the mask.
- **The models still run over the whole photograph**; the mask decides afterwards how much of their result each pixel shows. So a mask adds a quick blend to a run (and, under an upscale, an enlargement of the photograph), but changes neither how long the models take nor what the [run cache](#run-cache) stores, and running the same enhancements again with a different mask runs no model.
- **With an upscale**, the black part shows a plain (Lanczos) enlargement of the photograph, which is softer than the upscaled part. A strongly changed result, such as an upscale or a colorization, can show a visible seam at the mask's edge: a softer edge in the mask hides it.
- **With a lens blur, the white part is the subject, kept in focus.** The lens blur holds it sharp, in proportion to the mask (a grey pixel's blur is scaled down by its grey), and blurs the rest as it would without a mask. The enhancements are blended with the photograph *before* the blur, so the black part is the photograph blurred, not the photograph as it was, and an Ultra HDR gain map is blurred with the photograph and kept sharp in the same place. Changing only the mask estimates no depth and runs no model again: the blend and the blur are redone. A hard-edged mask can show a thin halo of background at the subject's edge; a softer edge hides it. With a lens blur and no other enhancement, a mask no longer limits where the blur applies: it is only what stays sharp, so to blur only part of the photograph, mask the part to keep sharp.
- **The grain is not limited by the mask.** It runs over the blended photograph, after any lens blur, and puts back the photo's own noise only where it is missing: inside the mask where an enhancement smoothed it, and wherever the lens blur blurred, while the part the mask kept as the photograph keeps its own noise and gets none. `--gr-add` adds its grain everywhere.
- **`--mask-use <USE>`** says which of the two the mask is for: `both` (the default), `enhancements` (limit the enhancements; the lens blur ignores the mask) or `lens-blur` (keep the mask in focus and apply every other enhancement to the whole photograph). `lens-blur` without `--lens-blur` is refused, since the mask would have no effect, and so is `--mask-use` without `--mask`.

```bash
# Hold the side hair in focus under an f/1.4 blur, and denoise the whole photograph
opai portrait.jpg --dn --lb --lb-f 1.4 --lb-focus 0.60,0.30 --mask hair.png --mask-use lens-blur -o out.jpg
```

## Remove

`--remove <FILE>` (alias `--rm`) takes out what a mask image marks in white and fills it from the photograph around it, with Lima (Big-LaMa, a generative inpainting model):

```bash
# Remove a tag hanging off a plant
opai bonsai.jpg --rm tag.png -o bonsai-clean.jpg

# Remove a passer-by, then restore the faces that are left and sharpen
opai street.jpg --rm passer-by.png --fr --sh -o street-clean.jpg
```

- **White is removed, black is kept.** The mask's brightness is read, and a pixel is removed where it is at least 50% grey; there are no partial removals. Transparency is ignored. Paint a little past the object's edge, and over its shadow if it casts one.
- **The mask is grown, and the fill feathered.** Every pixel within 8 px of the white part is replaced; from 8 to 16 px the fill fades into the photograph; nothing further than 16 px changes at all. These are pixels of the photograph, whatever its size.
- **Each separate region is filled on its own**, from a crop around it with generous context (0.6 times the region's size, plus 64 px, on every side), resized so its longer side is 512 px for the model and read back at full size. Regions close enough for their crops to overlap are filled together. Small objects come out sharp; a **large region comes out soft**, since a crop of thousands of pixels is filled at 512 and enlarged.
- **It runs first**, on the photograph as it was taken, before every other enhancement, so the fill is denoised, sharpened and upscaled along with the rest of the photograph. A face in the removed part is not restored by `--fr`.
- **The shapes must match**, as for `--mask`: a photograph whose aspect ratio differs from the mask's by more than 1% is refused, naming both ratios, and the rest of the batch still runs. With several inputs, one mask applies to each.
- **It goes with everything**: other enhancement flags, `--mask` (which limits the other enhancements, but never brings back what was removed) and `--ap` (whose analysis sees the photograph before the removal).
- **It runs on the CPU** under `-p auto`, which measured faster than Core ML for this model; `-p coreml` uses Core ML anyway. A first run loads the 210 MB model (a few seconds); each region then takes about 1 to 2 s. The removal is kept in the [run cache](#run-cache), so changing only the later enhancements does not run it again.
- **Fork only.** Lima is exported by this fork and installed by hand (see the fork's notes); it is not downloaded. Its weights were trained on Places365, whose terms allow non-commercial research use.

## Autopilot

`--autopilot` (alias `--ap`) runs the same analysis as the GUI's Autopilot on each image, once it's loaded, and applies the enhancements that image calls for: denoise for visible grain, face recovery when there are faces to restore, colorization for a black & white photo, light adjustment for a badly exposed one, color balance for a color cast, sharpen for a blurred one, and an upscale for a small one. Different images in one batch can get different enhancements. They still run in the [fixed order](#processing-order).

```bash
opai photos --ap -o enhanced
```

- **It replaces the enhancement flags.** Combining `--ap` with any enhancement flag (`--dn`, `--fr`, `--cl`, `--la`, `--cb`, `--sh`, `--up`, `--rs`, `--lb`, `--gr`) or their options (`--dn-str`, `--sh-str`, `--la-bias`, `--cb-bias`, `--scale`, `--rs-scale`, `--rs-str`, `--lb-f`, `--lb-amount`, `--lb-focus`, `--lb-edges`, `--lb-refine`, `--gr-amount`, `--gr-add`) fails with exit code `2`, for example `error: the argument '--autopilot' cannot be used with '--upscale [<MODEL>]'`. Every other option (`-q`, `-p`, `-r`, `--format`, `--prefix`, `--suffix`, `--overwrite`, `--mask`, `--mask-use`, `--remove`, `-o`) works as usual, and `-q` picks the build of the models autopilot runs.
- **Each enhancement uses its default strength or bias**: strength `100` for denoise and sharpen, bias `50` for light adjustment and color balance. An upscale runs at the factor the image's size calls for (see [Upscale](#upscale---upscale-model---up-model)); an image above 4 MP is not upscaled.
- **An image that needs nothing is still written**, unchanged, in the format asked for. Its row ends with `Enhancements applied to` as any other.
- **If face detection can't run** (for example, its model can't be downloaded), the image still gets the other enhancements, without face recovery, and the [log file](#log-file) records a warning.
- **What was chosen for each image** is recorded in the [log file](#log-file). The terminal shows each enhancement in the image's row while it runs, and with [`--verbose`](#--verbose) the image's line names them once it is written, with their models and parameters:

  ```bash
  opai photos --ap -o enhanced --verbose
  # 🖼️ [100%] Enhancements applied to       photos/a.jpg: Denoise Stockholm strength 100, Upscale Tokyo 4x
  # 🖼️ [100%] Enhancements applied to       photos/b.jpg: Color Balance Rio bias 50
  ```

### Choosing the models

Each enhancement runs with its default model unless an environment variable names another one, or, where the variable isn't set, the [GUI's settings](#gui-settings) choose a model or switch the enhancement off for Autopilot:

| Enhancement      | Environment variable    | Default model | Accepted values                       |
|------------------|-------------------------|---------------|---------------------------------------|
| Denoise          | `OPAI_AP_DENOISE`       | Stockholm     | `stockholm`, `gothenburg`, `malmo`, `off` |
| Face Recovery    | `OPAI_AP_FACE`          | Athens        | `athens`, `santorini`, `off`          |
| Colorization     | `OPAI_AP_COLORIZATION`  | Delhi         | `delhi`, `mumbai`, `jaipur`, `off`    |
| Light Adjustment | `OPAI_AP_LIGHT`         | Paris         | `paris`, `lyon`, `off`                |
| Color Balance    | `OPAI_AP_COLOR`         | Rio           | `rio`, `saopaulo`, `off`              |
| Sharpen          | `OPAI_AP_SHARPEN`       | Moscow        | `moscow`, `petersburg`, `novgorod`, `off` |
| Upscale          | `OPAI_AP_UPSCALE`       | Tokyo         | `kyoto`, `tokyo`, `saitama`, `off` |

```bash
# Upscale with Kyoto whenever autopilot upscales
OPAI_AP_UPSCALE=kyoto opai photos --ap -o out

# Never sharpen or restore faces, and use Malmo for denoise
OPAI_AP_SHARPEN=off OPAI_AP_FACE=off OPAI_AP_DENOISE=malmo opai photos --ap -o out
```

- Model names are spelled as the enhancement flags take them, in any case (`Kyoto` = `kyoto`).
- **`off`** (any case) leaves that enhancement out entirely: it's never applied, and the analysis doesn't even look for it, so `OPAI_AP_FACE=off` skips face recovery.
- An **empty** value is the same as not setting the variable.
- An **invalid** value fails with exit code `2` before anything is downloaded, naming the variable and listing what it accepts:

  ```
  error: invalid value 'paris' for the environment variable OPAI_AP_UPSCALE
    [possible values: kyoto, tokyo, saitama, off]
  ```
- The variables are read **only with `--ap`**. Without it they're ignored and not checked: `OPAI_AP_UPSCALE=paris opai photo.jpg --up -o out.png` upscales with Tokyo. The GUI never reads them; its Autopilot uses the models chosen in its settings.

## GUI settings

`opai` follows the settings you saved in the GUI, for the settings it has a flag or variable for: the processor, the quality of each enhancement, and, with [`--ap`](#autopilot), each enhancement's model and whether Autopilot may suggest it. You don't need shell aliases or `OPAI_AP_*` variables to make the command line match the GUI.

Every time the GUI starts, and every time you **Save** a change to one of these settings, it writes them to `gui-settings.json` in the application's configuration directory, beside its [log file](#log-file):

| OS      | File                                                                 |
|---------|----------------------------------------------------------------------|
| macOS   | `~/Library/Application Support/io.vinicius.opai/gui-settings.json`   |
| Windows | `%APPDATA%\io.vinicius.opai\gui-settings.json`                       |
| Linux   | `~/.config/io.vinicius.opai/gui-settings.json`                       |

`opai` reads it at startup. Each setting comes from the first of these that gives one:

| Setting                         | 1. Flag            | 2. Variable  | 3. GUI settings                                               | 4. Default            |
|---------------------------------|--------------------|--------------|---------------------------------------------------------------|-----------------------|
| Processor                       | `-p`               |              | The GUI's processor                                           | `auto`                |
| Quality of each enhancement     | `-q` (all of them) |              | The quality (HD or SD) of the model chosen for it             | `hd`                  |
| Autopilot's model (with `--ap`) |                    | `OPAI_AP_*`  | The model chosen for it, or `off` where Autopilot may not suggest it | The [default model](#choosing-the-models) |

```bash
# The GUI has every model at SD and Color Balance and Colorization off for Autopilot:
opai photos --ap -o out                        # SD, no colour balance, no colorization
opai photos --ap -q hd -o out                  # HD instead
OPAI_AP_COLOR=rio opai photos --ap -o out      # colour balance back on, with Rio
opai photos --ap --no-gui-settings -o out      # the built-in defaults, as if the GUI had never run
```

- The bare enhancement flags keep their own default models: `--up` runs Tokyo whatever the GUI chose. Only the quality comes from the GUI for them.
- With [`--verbose`](#--verbose), the processor line says where each value came from, and with `--ap` a second line lists each enhancement's model or `off` and its source:

  ```
  🧠 Running inference using the processor Auto (from GUI settings); quality SD (from GUI settings)
  🤖 Autopilot models: Denoise Stockholm (from GUI settings), Face Recovery Athens (from GUI settings), Colorize off (from GUI settings), ...
  ```
- The file never stops a run. If it's missing, `opai` uses its defaults. If it can't be read, isn't a settings file, or comes from a newer version, it's ignored and the [log file](#log-file) records why. A single entry naming a model or processor that doesn't exist is ignored on its own.
- `opai` never writes the file, and the GUI never reads it back: the GUI's own settings stay the source of truth. Export settings, language and background aren't shared.

## Processing order

The enhancements always run in this order, no matter what order you type the flags in:

```
Remove → Denoise → Face Recovery → Colorization → Light Adjustment → Color Balance → Sharpen → Upscale → Restore → Lens Blur → Grain
```

So these two commands do exactly the same thing:

```bash
opai photo.jpg --up --dn -o out.png
opai photo.jpg --dn --up -o out.png
```

The order is designed so that every step works on the cleanest image possible. For example, noise is removed before faces are restored, and the image is upscaled after the faithful corrections so they work at the original resolution. Restore comes after all of them: it redraws detail, so it is handed the best image the faithful models can make, and no faithful model after it corrects what it drew. Lens Blur comes after them: it is a creative choice rather than a correction, so every other step works on the sharp photo, and nothing after it sharpens, denoises or restores the blur. Grain comes last of all, so it can put back the noise any step before it smoothed away, the blurred background's included.

## Supported formats

### Input formats

The input format is detected from the **file extension** (any case), so the file needs a correct one. A JPEG named `photo.dat` is rejected when named, and ignored inside a directory.

| Format       | Extensions        |
|--------------|-------------------|
| BMP          | `.bmp`            |
| GIF          | `.gif`            |
| JPEG         | `.jpg`, `.jpeg`   |
| PNG          | `.png`            |
| TIFF         | `.tiff`, `.tif`   |
| AVIF         | `.avif`           |
| HEIF / HEIC  | `.heif`, `.heic`  |
| WebP         | `.webp`           |
| Camera RAW   | `.3fr`, `.ari`, `.arw`, `.cr2`, `.cr3`, `.crm`, `.crw`, `.dcr`, `.dcs`, `.dng`, `.erf`, `.fff`, `.iiq`, `.kdc`, `.mef`, `.mos`, `.mrw`, `.nef`, `.nrw`, `.orf`, `.ori`, `.pef`, `.qtk`, `.raf`, `.raw`, `.rw2`, `.rwl`, `.srw`, `.x3f` |

Camera RAW files are developed to 16 bits per channel before processing. Some other RAW formats are recognised but can't be decoded yet (for example GoPro `.gpr`, older Sony `.srf`/`.sr2` and Kodak `.k25`); those are rejected with an error.

### Output formats

| Format      | Extensions        | Compression | Default quality | 16-bit output |
|-------------|-------------------|-------------|-----------------|---------------|
| PNG         | `.png`            | Lossless    | n/a             | Yes           |
| TIFF        | `.tiff`, `.tif`   | Lossless    | n/a             | Yes           |
| BMP         | `.bmp`            | Lossless    | n/a             | No            |
| GIF         | `.gif`            | Lossless (256 colors) | n/a   | No            |
| JPEG        | `.jpg`, `.jpeg`   | Lossy       | 90              | No            |
| WebP        | `.webp`           | Lossy       | 75              | No            |
| AVIF        | `.avif`           | Lossy       | 60              | Yes           |
| HEIF / HEIC | `.heif`, `.heic`  | Lossy       | 60              | Yes           |

With an output **file**, the format is the one its extension names. With an output **directory**, it's chosen by [`--format`](#--format-format-default-preserve). The quality and compression settings are the same ones the GUI uses when exporting. They can't currently be changed from the CLI.

**Bit depth:** the output keeps the bit depth of the input. A 16-bit source (a 16-bit PNG/TIFF or any camera RAW) produces a 16-bit result, saved as 16-bit if the output format supports it. For RAW files, use PNG or TIFF to keep the extra depth: AVIF and HEIF are written at 8 bits for now.

**Transparency is not kept.** Transparent areas are filled with black during processing.

**Colour profile:** if the input embeds an ICC colour profile (a Display P3 photograph from a phone or camera, an Adobe RGB scan), the result embeds the same profile when it is written as JPEG, PNG, TIFF or WebP, so it shows in the same colours as the input. BMP, GIF, AVIF and HEIF results are written without it for now, and the [log file](#log-file) records a warning naming the format. Colours are not converted: the profile is carried over unchanged, and an input without one produces a result without one. Camera RAW inputs have none.

**HDR (Ultra HDR):** an Ultra HDR JPEG — a JPEG with an HDR gain map, as Lightroom, Camera Raw and Android phones write them (ISO 21496-1 or Adobe `hdrgm`) — stays one when the result is written as JPEG: its gain map is carried through every enhancement and written back after the result, so an HDR display still shows it in HDR. `--format jpeg` is the exception: it writes an SDR JPEG on purpose, leaving out the gain map and every trace of it (the `hdrgm:` XMP, the ISO 21496-1 marker and the MPF index), and `--format uhdr` or `preserve` keeps it. The gain map is turned upright with the photograph, keeps its own resolution and is never enlarged: after an upscale it is smaller than the result, which the format allows and every viewer resamples. When the result is not cropped and not smaller than the gain map, the gain map is written back exactly as the input had it, so enhancing a result again loses nothing of it; otherwise it is re-encoded at the result's JPEG quality, but never below 90, with its own gain-map metadata. A gain map without gain-map metadata of its own is not one any viewer can apply, and is left out; so is one whose result would have more XMP metadata than a JPEG's main XMP segment holds, since viewers would not find the declaration that the gain map is there. Results in any other format are SDR, and say so (below). A gain map that cannot be read or written is left out, and the result is an ordinary SDR JPEG that says so. **One caveat:** the gain map records how much brighter each part of the original was in HDR, so enhancements that change the photograph's brightness or colour — light adjustment, color balance and colorization — leave its HDR rendition slightly off; denoise, sharpen, face recovery and upscale barely change it.

**Metadata:** the result keeps the input's EXIF — capture date and time zone, camera, lens and exposure, GPS location, copyright and the rest — and its XMP — rating, label, keywords and their hierarchy, title, caption, creator, rights and edit history — whenever it is written as JPEG, PNG or WebP, and its IPTC (keywords, caption, creator, copyright) when it is written as JPEG; a PNG or WebP result carries the IPTC values in its XMP instead. There is no option to strip it, location included. A few fields are rewritten so that they stay true of the result: the software becomes `Open Photo AI <version> (<date>)`, the modification time and its offset become the time of the export, the pixel dimensions become the result's, the orientation says "upright" because the result is saved the right way up, the embedded thumbnail is removed (it would show the unenhanced, uncropped input), the result gets a new XMP document ID that names the input as its source, and the edit history gains an Open Photo AI entry. Lightroom's develop settings (`crs:`) are dropped, since Lightroom would apply them again to the already developed result. The camera's private MakerNote is kept byte for byte; in the rare case that the rewritten EXIF would not fit a JPEG, it is left out and the [log file](#log-file) says so. Nothing is written for BMP, GIF, TIFF, AVIF or HEIF results yet; the log file records a warning naming the format. Camera RAW inputs carry no metadata over yet.

**What a result loses is shown.** When a result holds less than its input, the command always says so on the terminal, with one line per affected input:

```text
⚠️ hawk.jpg: HDR gain map dropped: PNG output is SDR only for now
⚠️ scan.tif: 16-bit → 8-bit in JPEG
⚠️ phone.jpg: Display P3 profile not embedded in AVIF
```

It names an **HDR** input written as SDR: an Ultra HDR / ISO 21496-1 gain-map JPEG written as anything but JPEG, or written as JPEG when its gain map could not be carried (`HDR gain map dropped: it could not be carried`), or written as plain JPEG with `--format jpeg`, which leaves it out as asked (`written as SDR (gain map left out)`), or a PQ or HLG PNG, AVIF or HEIF, which every format writes as SDR for now; **fewer bits per channel** than the input, or integer samples from a floating-point one; and a **colour profile** that is not in the result (the format cannot hold one, it is for another colour model, the encoder could not write it — a profile too large for the format — or the input carried a coded colour description such as Display P3 instead of a profile). A plain sRGB profile left out of an AVIF or HEIF result is not reported: those formats read as sRGB without one. Off a terminal the line follows that input's own line; on one it is printed once the progress view finishes, before the summary. With several inputs a count follows, such as `⚠️ 2 of 5 images lost something: 2 HDR, 1 bit depth`. Each loss is also recorded in the [log file](#log-file). **The exit status is unchanged:** a loss is a warning, and the result is still written. Not detected yet: AVIF and HEIF gain maps, and the private gain maps of older iPhones.

## What happens during a run

1. **The command is checked.** The arguments, the output, and every input are all checked before anything is downloaded or loaded. A typo fails right away with [exit code 2](#exit-codes), even if it's in the fortieth path. Specifically, the run is refused if:
   - an input you named (a file or a directory) doesn't exist or can't be read;
   - a file you named isn't an image `opai` can read;
   - a directory input's tree can't be listed (for example, a subfolder you don't have permission to read), naming that directory;
   - the output can't be used as asked (see [A file or a directory](#a-file-or-a-directory));
   - no image is left to enhance once the inputs are expanded, e.g. a directory with no images in it.

   A file **found inside a directory** that has an image extension but can't be read is not a refusal: it's skipped, recorded in the [log file](#log-file), and the rest of the batch goes on. Nothing is written or created when the run is refused.
2. **Dependencies are installed (first run only).** The first time `opai` runs, it downloads the components it needs: ONNX Runtime, plus CUDA and TensorRT on systems with a supported NVIDIA GPU. A `💾 Downloading dependency <name>... ⣾ 34% Complete` line shows the progress, with a spinner that keeps turning while the download works; it is rewritten when the next dependency starts, and shows a green `✓` once the dependency reaches `100%`. These are shared with the GUI, so if the GUI has already downloaded them, nothing is downloaded. This happens once per command, however many images it has. Then the [processor](#-p---processor-processor-default-auto) is checked against what this machine offers, and the run is refused with [exit code 2](#exit-codes) if it doesn't offer it. Once it's accepted, the line `🧠 Running inference using the processor <processor>; quality <HD|SD>` names the processor you asked for (`Auto` under `-p auto`) and the quality, followed by a blank line.
3. **Each image, one after another:**
   1. **The image is loaded.** Its row in the list reads `🖼️ [  0%] Preparing to enhance <path>`. With [`--autopilot`](#autopilot), the image is then analysed, and its row reads `🖼️ [  0%] Choosing enhancements for <path>` until the analysis has chosen its enhancements.
   2. **Models are downloaded (first use only).** Each model is downloaded the first time it's used. The image's row reads `Downloading <enhancement> for <path>` while this happens.
   3. **The enhancements run**, in the [fixed order](#processing-order). The image's row reads `Applying <enhancement> to <path>`, with one percentage for the whole image. When `--fr` is used, face detection is included in that percentage and reads as `Face Recovery`. The upscale factor, when `--scale` isn't given, is chosen from each image's own size.
   4. **The file is written**, and the image's row reads `🖼️ [100%] Enhancements applied to <path>`. Until then its percentage stops at `99%`. The [log file](#log-file) records the file written.
4. **A summary is printed** once every image has run: how many images were written and where, and the reason for each image that failed.

**A batch continues past a failure.** An image that fails to decode, to be enhanced or to be written has its row turn into a red `🧨 [ 42%] Failed to enhance <path>` row, keeping the percentage it had reached, and the next image runs. At the end, its reason is listed under the view, and the command exits with [code 1](#exit-codes).

### Hardware acceleration

By default (`-p auto`), the CLI uses the fastest hardware available, trying in this order: **TensorRT → CUDA → CoreML → WebGPU → CPU**. To choose the hardware yourself, use [`-p, --processor`](#-p---processor-processor-default-auto). The [log file](#log-file) records which hardware each image used.

- **A processor that can't run a model falls back to the CPU.** If the processor you chose (or `auto` picked) can't open one of the models, that model runs on the CPU and the run goes on. The terminal doesn't say so. The log file does, for example `ran on CPU (TensorRT could not open every model)`.
- **`-p` doesn't change what is installed.** The first run downloads what this machine needs, whichever processor you choose: `-p cpu` on a machine with a supported NVIDIA GPU still downloads CUDA and TensorRT. They're shared with the GUI and with later `auto` runs.
- **The [run cache](#run-cache) isn't kept per processor.** Running the same enhancements on the same image again within 24 hours with another `-p` reuses the cached result, and nothing runs on the new processor. The log file says `served from cache`. To compare processors, use another image or change the enhancements.

> [!NOTE]
> With TensorRT, the first run of each model can take several minutes while TensorRT optimises it. Later runs are much faster.

### Run cache

Every step's result is cached on disk for **24 hours**. If you run the same enhancements on the same image again within that time, the cached results are reused and the run finishes almost immediately. Changing anything (a model, strength, bias, scale or quality) creates a new cache entry from that step onwards. With [`--verbose`](#--verbose), a re-run's line ends `(cached)` when every step was reused, or `(2 of 3 cached)` when only some were.

### One instance at a time

Only one Open Photo AI process can run at a time, and that includes the GUI. If another one is running, `opai` exits with [code 1](#exit-codes) and the error `Open Photo AI is already running (…); close it and try again`, naming the other process when it can (for example `gui, pid 4821`). Close it, or wait for it to finish, and try again. To process several images, give them all to one command (see [Scripting](#scripting-and-batch-processing)). [`--gui`](#--gui) is the exception: it hands the images to the GUI that is already open.

## Terminal output

Everything `opai` prints goes to **stderr**. Nothing is written to stdout.

When stderr is a terminal, a `🧠` line names the processor and quality first, then you see one live view for the whole run, whatever the number of images: the batch's progress bar, a blank row, and a list of the five most recent images. A batch of eight, while its seventh image is being enhanced:

```
💾 Downloading dependency ONNX Runtime... ✓ 100% Complete    ← only when something was downloaded
🧠 Running inference using the processor Auto; quality HD

Enhancing [6/8]  ███████████████████████░░░░░   80.2%   ETA 1m12s

🖼️ [100%] Enhancements applied to       photos/c.jpg
🖼️ [100%] Enhancements applied to       photos/d.jpg
🧨 [ 42%] Failed to enhance             photos/e.jpg
🖼️ [100%] Enhancements applied to       photos/f.jpg
🖼️ [ 42%] Applying Upscale to           photos/g.jpg
```

And once the run ends, the view stays as it was last drawn, with the summary under it:

```
✨ 7 images enhanced successfully; files saved under out
🧨 could not enhance photos/e.jpg: …
🧨 1 of 8 images failed
```

- **The dependency line** appears only when a dependency is downloaded or expanded. Its spinner keeps turning while the line is drawn, and becomes a `✓` while the percentage reads `100%`. When several dependencies are installed, the same line is rewritten for each one.
- **The processor line** names the processor you asked for with [`-p`](#-p---processor-processor-default-auto), spelled `Auto`, `CPU`, `CoreML`, `CUDA`, `TensorRT` or `WebGPU`, and the [quality](#-q---quality-hdsd-default-hd), `HD` or `SD`. Under `-p auto` it reads `Auto`, not the hardware `auto` picks; the [log file](#log-file) records that.
- **The progress bar** covers the whole batch and is always labelled `Enhancing`. `[n/N]` counts the images that have ended, written or failed; `n` is padded to the width of `N`, as in `[02/15]`. It fills as `(n + p) / N`, where `p` is the current image's progress as its row shows it, so it moves forward within each image, never moves backwards when one image ends and the next starts, and reaches `100.0%` only once the last image has ended. It ends with `ETA <time>`, an estimate of the time the batch has left at the rate it has run so far; it reads `ETA --` until at least 1% has run. The first image's model loading (and, on first use, its download) makes the early estimate high, and it settles as the batch goes on.
- **The list** shows one row per image started so far, scrolling to the most recent five. Each row shows the image's own progress, right-aligned in three columns (`[  0%]`, `[ 42%]`, `[100%]`). It covers face detection and every enhancement, never decreases, and stops at `99%` until the file is written. A row reads as follows, its words padded with spaces so every path starts in the same column:
  - `🖼️ [  0%] Preparing to enhance <path>` before the image's run has reported anything, and for the whole time an image with nothing left to run is processed (an upscale skipped because the image is already large enough, or an image autopilot found nothing to do for);
  - `🖼️ [  0%] Choosing enhancements for <path>` with [`--autopilot`](#autopilot), while the image is analysed. On the first run, this includes downloading the face detection model, so it can stay at `0%` for a while;
  - `🖼️ [ 12%] Downloading <enhancement> for <path>` while a model is downloaded;
  - `🖼️ [ 42%] Applying <enhancement> to <path>` while an enhancement runs or is served from the [run cache](#run-cache);
  - `🖼️ [100%] Enhancements applied to <path>` once its result is written, followed under [`--verbose`](#--verbose) by `: <applied>` (see below);
  - `🧨 [ 42%] Failed to enhance <path>` once it has failed;
  - `🧨 [ 42%] Stopped enhancing <path>` when you [stopped the run](#stopping-a-run) while it was being worked on.

  `<enhancement>` is the enhancement's name as the GUI shows it: `Denoise`, `Face Recovery`, `Colorize`, `Light Adjust`, `Color Balance`, `Sharpen`, `Upscale`, `Restore` or `Lens Blur` (which is also what the depth estimate and the matte it starts with are called).
- **With `--verbose`**, a written image's row ends with `: <applied>`, what it received, in the order it ran: each enhancement's name, its model as the GUI names it (`Malmö`, `São Paulo`), and its parameter, `strength <n>` for denoise and sharpen, `bias <n>` for light adjustment and color balance, `<factor>x` for an upscale (`2x`, `1.5x`), `strength <n>` for a restore, after `<factor>x, ` where `--rs-scale` was given, `f/<n>, focus <x>,<y>` for a lens blur (then `, edges from the depth` under `--lb-edges depth`, and `, refined` under `--lb-refine`), none for face recovery and colorization. It ends with ` (cached)` when the [run cache](#run-cache) served every step, or ` (<k> of <n> cached)` when it served some. An image that needed nothing reads `no enhancement needed`. The text is drawn in the terminal's default color, and in the live view a row wider than the terminal is cut at its edge; the plain lines and the [log file](#log-file) carry it whole. Failed and stopped rows carry none. For example, `opai photo.jpg --dn --la --up --scale 2 -o out.png --verbose` ends with `🖼️ [100%] Enhancements applied to       photo.jpg: Denoise Stockholm strength 100, Light Adjust Paris bias 50, Upscale Tokyo 2x`.
- **The summary** starts with a success line when at least one image was written. For exactly one image it names the file written, so a numbered name is visible: `✨ Image enhanced successfully; file saved at out_1.png`. For more, it names the output directory: `✨ 7 images enhanced successfully; files saved under out`, or, without `-o`, ends `files saved beside their sources`. Then a `🧨` line gives the reason for each image that failed, in the order they ran, since a failure can have scrolled out of the list. Last comes the run's closing `🧨` line, if it didn't succeed (see [Exit codes](#exit-codes)).

In color, the dependency's name and its percentage are bold white, the spinner magenta and the `✓` green. In the processor line, the processor is green and the quality yellow. The bar is drawn in a color gradient. In a `🖼️` row, the percentage (with its `%`) is blue, the enhancement's name (or the word `Enhancements`) magenta and the path cyan and underlined; a `🧨` row is red throughout. The success line is bold yellow.

With a single image, the view is three rows tall: the bar, a blank row and the image's row.

When stderr is **not** a terminal (for example, when redirected to a file or run in CI), there's no dependency line, no view and no progress bar. The processor line comes first, followed by a blank line. Then each image prints one line, without colors, when it ends, with the words its row ends with, its percentage included; a failed image's line adds the reason, and under `--verbose` a written image's line adds what it received. Then come the summary's success line and the closing line:

```
🧠 Running inference using the processor Auto; quality HD

🖼️ [100%] Enhancements applied to       photos/a.jpg
🧨 [ 42%] Failed to enhance             photos/b.jpg: cannot write out/b.jpg: …
🖼️ [100%] Enhancements applied to       photos/c.jpg
✨ 2 images enhanced successfully; files saved under out
🧨 1 of 3 images failed
```

A terminal that can't draw the live view gets these same lines, in color.

**Colors** are turned off when the [`NO_COLOR`](https://no-color.org) environment variable is set to any non-empty value:

```bash
NO_COLOR=1 opai photo.jpg --up -o out.png
```

The terminal shows only the basic progress and the result. More detail, such as each file written, a skipped upscale, a skipped unreadable file, how many faces were found, the hardware used and full error messages, goes to the [log file](#log-file).

## Stopping a run

Press **Ctrl-C** at any time to stop.

- **While enhancing:** the run stops at the next safe point. Nothing is written for the image being enhanced, and its row reads `🧨 [ 42%] Stopped enhancing <path>`, keeping the percentage it had reached. No image after it is started. Files already written for earlier images are **kept**, and the summary says how many. The reasons of any images that failed before the stop are listed, then the line `🧨 stopped: nothing more was written` is printed. Exit code: `130`, even if images failed before the stop.
- **While downloading dependencies:** the download stops. Next time, it resumes from where it left off instead of starting over. Nothing is written. Exit code: `130`.

A file that the stopped image would have replaced with `--overwrite`, including its own source in place, is left exactly as it was, with no hidden file beside it.

## Exit codes

| Code  | Meaning                                                                                                     |
|-------|-------------------------------------------------------------------------------------------------------------|
| `0`   | Success. Every image was written, or with `--gui`, every image was handed to the GUI.                       |
| `1`   | The run failed: a setup error (including another instance running), the GUI couldn't be started for `--gui`, or **any** image that failed to decode, enhance or be written. The other images in the batch were still processed. |
| `2`   | Usage error: bad arguments, an unreadable or unsupported named input, an output that can't be used as asked, no image to enhance, or a [processor](#-p---processor-processor-default-auto) this machine doesn't offer. Nothing was written. |
| `130` | Stopped with Ctrl-C. Results already written for earlier images are kept.                                  |

`--help` and `--version` exit with `0`.

A run that fails ends with one `🧨` line. When images failed in a run of several, it says how many: `🧨 1 of 8 images failed`. When the run has a single image, that image's own reason is the last line, for example `🧨 could not enhance photo.jpg: …`, and no count follows it.

## Log file

Each run writes detailed records to a log file shared by the CLI and the GUI:

| OS      | Path                                                              |
|---------|-------------------------------------------------------------------|
| macOS   | `~/Library/Application Support/io.vinicius.opai/logs/opai.log`    |
| Linux   | `~/.config/io.vinicius.opai/logs/opai.log`                        |
| Windows | `%LOCALAPPDATA%\io.vinicius.opai\logs\opai.log`                   |

What's in it that the terminal doesn't show:

- An upscale that was skipped because the image was already large enough.
- With [`--autopilot`](#autopilot), the enhancements chosen for each image, and a warning when part of the analysis (such as face detection) couldn't run.
- A file found in a directory that was skipped because it couldn't be read, and why.
- Face detection results, e.g. `found 3 faces, restoring 2`, or `no faces were found; none will be restored`.
- Each file written, with its size and format, the hardware it ran on, e.g. `ran on CoreML, CPU` or `served from cache`, and the enhancements it received, e.g. `applied Denoise Stockholm strength 100 (cached)`, with or without `--verbose`.
- The full error behind any `🧨` line.

**Please attach this file when [reporting a bug](https://github.com/vegidio/open-photo-ai/issues).**

## Common errors

| What you see                                                                 | Cause and fix |
|-------------------------------------------------------------------------------|---------------|
| `error: at least one enhancement is required: --remove (--rm), --denoise (--dn), …` | No enhancement flag was given. Add at least one, or `--ap`. |
| `error: the argument '--autopilot' cannot be used with '--upscale [<MODEL>]'`   | `--ap` chooses the enhancements itself. Drop `--ap`, or drop the enhancement flags and their options. |
| `error: invalid value 'paris' for the environment variable OPAI_AP_UPSCALE` followed by `[possible values: …]` | An `OPAI_AP_*` variable names a model of another enhancement, or a misspelt one. Pick one from the list shown, `off`, or unset it. |
| `error: invalid value 'foo' for '--upscale [<MODEL>]'` followed by `[possible values: …]` | Unknown model name. Pick one from the list shown. |
| `error: invalid value 'photo.jpg' for '--upscale [<MODEL>]'`                   | An input was placed after a flag. Put the `INPUT`s first. |
| `error: invalid value '9' for '--scale <FACTOR>': the scale must be from 1.0 to 8.0` | `--scale` out of range or not a number. |
| `error: --upscale (--up) cannot be used with --rs-scale 2: …`                  | An enlarging restore replaces the upscale. Leave out `--up`, or use `--rs-scale 1` to restore after the upscale. |
| `error: invalid value '3' for '--rs-scale <FACTOR>': the restore size must be one of 1, 2, 4` | `--rs-scale` takes only 1, 2 or 4. |
| `error: Osaka is a restore model now: use --restore osaka (--rs osaka) …`       | Osaka moved from Upscale to Restore. Use `--rs osaka`, with `--rs-scale 2` or `4` to enlarge. |
| `error: invalid value '0.5' for '--lb-f <F>': the f-number must be from 1 to 32` | `--lb-f` out of range or not a number. |
| `error: invalid value '101' for '--lb-amount <AMOUNT>': the blur amount must be from 0 to 100` | `--lb-amount` out of range or not a number. |
| `error: the argument '--lb-amount <AMOUNT>' cannot be used with '--lb-f <F>'` | An aperture is asked for twice: choose one of `--lb-f` and `--lb-amount`. |
| `error: invalid value '1.2,0.5' for '--lb-focus <X,Y>': the focus point must be two fractions from 0 to 1; focus_x must be between 0 and 1 inclusive, but 1.2 was supplied` | Each of the two numbers must be from 0 to 1, separated by a comma, with no space after it unless quoted; the message names `focus_x` (across) or `focus_y` (down), whichever was refused, and a negative one is refused the same way. |
| `error: invalid value '0.5' for '--la-bias <BIAS>': the bias must be a whole number from -100 to 100` | Strength/bias must be a whole number within range. |
| `error: the following required arguments were not provided: --denoise [<MODEL>]` | A strength/bias/scale option was used without its enhancement flag (e.g. `--dn-str` without `--dn`, `--scale` without `--up`, or `--lb-f`, `--lb-amount`, `--lb-edges` or `--lb-refine` without `--lb`). |
| `error: invalid value 'raw' for '--format <FORMAT>': "raw" is not a format; use preserve, …` | Unknown `--format`. Pick one from the list shown. |
| `error: invalid value 'cuda' for '--processor <PROCESSOR>'` followed by `[possible values: …]` | That processor doesn't exist for your operating system (`coreml` exists only on macOS; `cuda` and `tensorrt` never on macOS). Pick one from the list shown. |
| `🧨 cannot read missing.jpg: No such file or directory …`                      | A named input doesn't exist or can't be read. If it's a wildcard, it matched nothing. |
| `🧨 the codec refused notes.txt: could not determine an image format …`        | A named file isn't an image `opai` can read, or its extension isn't a supported format. |
| `🧨 cannot list photos: …`                                                     | Part of a directory input's tree can't be read. Fix its permissions, or leave it out. |
| `🧨 no image to enhance in photos`                                             | The inputs hold no image `opai` can read. Add `-r` if the images are in subfolders. |
| `⚠️ --upscale is ignored with --gui; set it in Open Photo AI instead`          | `--gui` opens the images in the GUI, which uses its own settings. Not an error: the images are still opened. |
| `🧨 could not open Open Photo AI; is it installed? …`                          | `--gui` on macOS couldn't find or start the GUI. Install it in Applications and open it once. |
| `🧨 could not find Open Photo AI: OpenPhotoAI is neither beside opai nor on PATH` | `--gui` on Windows or Linux couldn't find the GUI's program. Put its folder on your `PATH`. |
| `🧨 an output file takes exactly one input file; …`                            | `-o` names a file, but there are several inputs or a directory input. Give a directory instead. |
| `🧨 --suffix needs an output directory; …`                                     | `--format`, `--prefix` and `--suffix` don't work when `-o` is a file. Name a directory, or leave `-o` out to write beside the source. |
| `🧨 cannot write into out: it is a file, not a directory, …`                   | `-o` would be a directory, but a file already exists there. |
| `🧨 cannot write out/a.png: …`                                                 | The result couldn't be written there: the directory is missing or not writable, or every numbered name up to `_999` is taken. Without `-o`, this is the source's own directory; give `-o` to write elsewhere. |
| `🧨 Open Photo AI is already running …; close it and try again`                | Another `opai` process or the GUI is running. Close it and try again. |
| `🧨 the processor … is not available on this machine; use …`                   | This machine doesn't offer the processor given with `-p` (for example, no supported NVIDIA GPU for `cuda`). Use one of the processors listed, or leave `-p` out. Exit code `2`. |
| `🧨 the mask could not be read: …`                                             | The file given with `--mask` doesn't exist or isn't an image `opai` can read. Exit code `2`. |
| `🧨 could not enhance photos/b.jpg: the mask's aspect ratio is 1.500 (3000x2000) and the photograph's is 1.333 (4000x3000); …` | The mask isn't the photograph's shape. Make a mask for it, or leave that photograph out; the rest of the batch went on. |
| `🧨 the removal mask could not be read: …`                                     | The file given with `--remove` doesn't exist or isn't an image `opai` can read. Exit code `2`. |
| `🧨 could not enhance photos/b.jpg: the removal mask's aspect ratio is …`       | As for `--mask`: the removal mask isn't the photograph's shape. |
| `🧨 [ 42%] Failed to enhance photos/b.jpg`                                      | That image failed; the rest of the batch went on. Its reason is listed under the view when the run ends. |
| `🧨 could not enhance photos/b.jpg: …`                                         | That image failed; the rest of the batch went on. The log file has the full error. |
| `🧨 failed to decode photos/b.jpg: …`                                          | That image's header was readable, but the image itself wasn't; the rest of the batch went on. |
| `🧨 2 of 5 images failed`                                                      | The summary after a batch with failures. Exit code `1`. |
| `🧨 stopped: nothing more was written`                                         | You pressed Ctrl-C. |
