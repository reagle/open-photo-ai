> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

After a run, the terminal says `Enhancements applied to photo.jpg` and nothing about which ones. Under `--autopilot`
that is the whole question: a user who sees a colour shift in the result cannot tell whether colour balance or light
adjustment ran, with which model, or at what bias, without finding and reading the log file. Even without `--ap`, the
upscale factor is chosen per image, and a re-run served from the run cache in under a second looks identical to a real
run. The live row names each family while it runs, but only for as long as it runs: a fast or cached step is never
seen, the row does not name the model or its parameters, and off a terminal (`2>log`, CI) the in-progress row is never
printed at all.

The `add-cli-autopilot` change deliberately left this out ("Printing the chosen families on the image's end line" was
a Non-Goal, because the log records them and the live row shows each family while it runs). This change revisits that
decision, but only on request: the report is shown under a new `--verbose` flag, and a run without it prints exactly
what it prints today. design.md - D1 answers the rationale point by point, and D6 explains the flag.

## What Changes

Task estimate: **9**. One change, not a series. It is independent of `cli-output-default` (no shared requirement,
either may land first).

**In `crates/cli`, observable on the command line:**

- **A new `--verbose` flag turns the report on.** It takes no value, works with every other option (`--ap` included),
  and has no short form, because `-v` already prints the version. **Without it, nothing the terminal shows changes.**
- **Under `--verbose`, each written image's end line names what it received.** The ended row, in the live view and in
  the plain line off a terminal, gains `: <applied>` after the path:
  ```
  🖼️ [100%] Enhancements applied to       photo.jpg: Denoise Stockholm strength 100, Light Adjust Paris bias 50, Upscale Tokyo 2x
  ```
  Each enhancement is named in the order it ran, with the GUI's family words (as the live row already uses), the
  model's display name from the catalogue (`Malmö`, `São Paulo`), and its parameter: `strength <n>` for denoise and
  sharpen, `bias <n>` for light adjustment and colour balance, `<factor>x` for an upscale. Face recovery and
  colorization take no parameter.
- **The run cache is visible.** When every step was served from the run cache, the list ends with ` (cached)`; when
  some were, with ` (<k> of <n> cached)`.
- **An image that received nothing says so:** `: no enhancement needed`, for an upscale skipped as unnecessary or an
  image autopilot found nothing to do for.
- **Same rule with and without `--autopilot`.** With flags, it confirms the defaults and the per-image upscale factor;
  with `--ap`, it is the only place on the terminal the choice is visible.
- **Colour:** the appended text is drawn in the terminal's default colour, so the path stays the row's most visible
  element; `NO_COLOR` and plain output carry it uncoloured, as they do the rest of the line.
- **Failed and stopped rows are unchanged**, as are the summary lines, the processor line and the view's height. The
  face count, the providers used, and the image's size and format stay in the log only.
- **The log's per-image `wrote …` record gains the same `<applied>` text, with or without `--verbose`**, so a batch
  run without the flag, or whose rows scrolled out of the live view, can still be reviewed there.
- **Docs:** `docs/CLI_REFERENCE.md`: the `--verbose` option, and the "Terminal output", "Autopilot" and run-cache
  sections.

**Unchanged:** every run without `--verbose`, on the terminal. The GUI, the library, every exit status, standard
output (still empty).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cli-enhance`: a new requirement, "Under --verbose, each image's end names the enhancements it received" (the
  `--verbose` flag itself, the `<applied>` text, its wording, the cache marker, the empty case, colour, the log
  record); "Progress is shown while the command works" appends `: <applied>` to the ended row and the plain line under
  `--verbose` only, its scenarios unchanged; "Autopilot chooses each image's enhancements" allows printing the choice
  under `--verbose`.

## Impact

**Crates touched:** `cli` only (`args.rs`, `chain.rs`, `look.rs`, `progress.rs`, `run.rs`) and
`docs/CLI_REFERENCE.md`.

**Why the logic is in the front end:** everything the line reports is already shared and stays in `opai`: the
operations (`Plan.operations`, built from the catalogue), the model display names (`VariantEntry::label`), the
per-step cache signal (`Stage::Cached` in the progress reports) and the run's provider verdict. What the CLI adds is
the sentence that puts them into words for a terminal row. The GUI shows the same facts in its own sidebar and
progress UI, in TypeScript and translated through i18next, and would not use an English sentence built in Rust.

**`opai`:** unchanged. (An alternative that adds a per-step "served from cache" list to `Enhanced` is weighed in
design.md - D4 and not taken.)

**`rust-sak`:** nothing needed. The row is drawn with `tui`'s `Live`/`LiveLine` as today; no new infrastructure.

**Third-party crates introduced:** none.

**Breaking:** none. Without `--verbose` every line is as today; a script that adds `--verbose` and matches the ended
line exactly (`Enhancements applied to <path>$`) would need to allow the new tail.
