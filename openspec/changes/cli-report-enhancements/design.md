## Context

See proposal.md - Why. These facts were checked in the code and shape the design:

- **The chain is known before it runs.** `run::enhance_one` holds a `chain::Plan { operations, notes }` for every
  image: built from the flags before loading (`Planner::Fixed`) or from `Opai::suggest` after loading
  (`Planner::Autopilot`, via `choose` and `plan_suggested`). An empty `operations` is the "nothing to run" path (a
  skipped upscale, or autopilot suggesting nothing).
- **`chain::build` already holds everything a description needs.** It looks up the model's `VariantEntry` (whose
  `label` is the catalogue's display name: `Stockholm`, `Malmö`, `São Paulo`, `Petersburg`), sets the published
  parameter defaults, applies the strength/bias override, and for an upscale the scale (explicit, suggested by
  autopilot, or `opai::suggested_scale` for the image's size). So the values that ran are known in `plan()` without
  reading them back out of an `Operation`.
- **The GUI's family words are already in the CLI**: `look.rs` names `Denoise`, `Face Recovery`, `Colorize`,
  `Light Adjust`, `Color Balance`, `Sharpen`, `Upscale` for the `Applying … to` row.
- **The run cache reports per step.** `opai` emits `Stage::Cached` for an operation served from the store
  (`inference/progress.rs`: "a variant of its own rather than `Running` reported at `1.0`, so that a front end can say
  *Upscale - from an earlier run*"). `progress::ImageProgress::report` currently folds `Running | Cached` into one
  `Applying` state, discarding the difference. The run's `ProviderVerdict::NothingExecuted` separately says "every
  step was served" (`run::provenance` logs `served from cache`).
- **The run cache is real and persistent, which explains the observed sub-second re-runs.** `openspec/specs/run-cache`
  - "The result of each step of a chain is kept and reused": each step's result is stored, keyed by the input image's
  identity plus the operations applied so far, kept for a day on disk (memory-only when the disk store cannot be
  opened), and served without running the model. A re-run of the same input and settings therefore runs no model at
  all. Under `--ap`, the analysis's face detection is a cached `execute` too, and the remaining signals are cheap, so
  the whole run is fast. A report must therefore say "cached", or a user comparing two runs is misled into thinking
  the second one did the work again.
- **The ended row is drawn by `look::row` (live) and `look::plain_row` (off a terminal)** from `RowState::Applied`,
  with the words padded to `WORDS_WIDTH` so paths line up. A failed plain line already appends `: <reason>` after the
  path. `rust-sak`'s `tui` progress line clips each piece to its area, so a row too long for the terminal is cut at
  the right edge, never wrapped.
- **`look::ImageEnd::Enhanced`** carries nothing today; `progress::View::end_image` / `end_line` turn it into the
  ended row.
- **`-v` is taken.** `args.rs` maps `-v` to `--version` (`ArgAction::Version`), pinned by
  `args::tests::the_version_is_the_librarys` and documented in `docs/CLI_REFERENCE.md` ("`-v` also works").

## Goals / Non-Goals

**Goals:**

- With `--verbose`, on a terminal or not, the user can read for each written image which enhancements ran, with which
  model, at which parameter or factor, and whether they came from the cache, without opening the log.
- Without `--verbose`, the terminal shows exactly what it shows today.
- No new line in the view, no change to its height or to the path column; the summary lines are unchanged.
- One source of truth: the description is built from the same values that built the operations, in `plan()`, so the
  two cannot drift.

**Non-Goals:**

- Machine-readable output (`--json`, NDJSON on standard output). Standard output stays empty, as specified. Sketched
  under "Difficulty and likely paths" as a follow-up change, `cli-report-json`.
- A `-v` short form for `--verbose` (D6), or a `--quiet` switch.
- `--verbose` printing anything beyond `<applied>`: log records, providers, timings. One flag, one addition.
- Printing the face count, the providers used, the image's size or format. They stay in the log, as "Nothing printed
  outside the layout" requires.
- Keeping rows that scrolled out of the live list visible after the run. The plain mode (`2>file`) and the log record
  every image.
- Precision (`FP32`/`FP16`/`INT8`) per step. The processor line already states the run's quality.
- Any change to `opai` or the GUI.

## How the work divides

**All in `cli`.** Every fact reported comes from `opai` through existing seams (the catalogue's `VariantEntry`, the
published parameter defaults, `suggested_scale`, `Stage::Cached`). The CLI only assembles an English sentence for a
terminal row, which the GUI would not use: it shows the same facts in its own components, translated through i18next.

## `rust-sak` coverage

| Need | Covered by | Feature |
|---|---|---|
| Drawing the ended row in the live view | `Live`, `LiveLine`, as today | `tui` |
| Clipping a row wider than the terminal | the progress line's own clipping | `tui` |

Nothing is missing, so `rust-sak` is not changed. No third-party crate is introduced.

## Decisions

### D1. Revisiting the `add-cli-autopilot` Non-Goal

That change listed "Printing the chosen families on the image's end line" as a Non-Goal with two reasons. Both hold
less well than they seemed:

- *"The log records them."* It does, but the log is diagnostics, out of sight, in a platform-specific directory,
  interleaved with session and provider records. Which enhancements touched a user's photograph is not a diagnostic
  about the run; it is the run's result, as much as the path written is, and the path is printed. The autopilot user
  who sees a colour shift and has to go find a log file to learn whether colour balance ran is exactly the case this
  change was raised for.
- *"The live row shows each family while it runs."* Only transiently, and not reliably: a step that takes a fraction
  of a second (light adjustment, colour balance, any cached step) is overwritten before it is seen; a run fully served
  from the cache shows nothing at all; off a terminal, the in-progress states are never printed, only the ended line;
  and the row names the family, never the model, the strength or bias, or the upscale factor.

What that Non-Goal protected is still protected: the view's layout (no new line, same height, same path column), the
rule that provider, face count, size and format stay in the log, and the summary lines. The text rides on the one line
that already exists per image and already appends `: <reason>` when it fails, so a written image appending
`: <applied>` is the symmetric case.

### D2. The description is built in `plan()`, from the values that built the operations

`Plan` gains `applied: Vec<Applied>`, one per operation, in the same order, where
`Applied { family: Family, model: &'static str, detail: Option<String> }`:

- `family`: the operation's family, worded by `look.rs`'s existing family names.
- `model`: `VariantEntry::label`, the catalogue's display name, already looked up in `build`.
- `detail`: `strength <n>` (denoise, sharpen), `bias <n>` (light adjustment, colour balance), `<factor>x` (upscale),
  none (face recovery, colorization). The value is the one `build` set: the override when one was given, otherwise the
  published default it read from the row. A factor is written without a trailing `.0` (`2x`, `1.5x`).

`build` returns the `Applied` beside the `Operation`, so a model or parameter added later cannot reach the chain
without reaching the description. Under `--ap`, `Selection::from_suggestions` feeds the same `plan()`, so the two
modes cannot word things differently.

*Alternative:* derive the text from the built `Operation` (`display_name()`, `params()` per arm). It would need a
seven-arm match on `opai`'s operation types in the CLI and reading values back out through accessors the CLI does not
otherwise use, and `display_name()` carries the precision and the upscale's native pass scale rather than the
requested factor. Rejected: `plan()` already has the values in hand.

### D3. Under `--verbose`, the ended row carries `: <applied>`, in both modes

`look::ImageEnd::Enhanced` gains the text: `Enhanced(Option<String>)`, `None` without `--verbose` (D6). `look::row`
and `look::plain_row`, for `RowState::Applied` with a text, append `: ` and the text after the path, in the terminal's
default colour (no style), so the cyan underlined path stays the row's focus. Under `NO_COLOR` and in plain mode the
whole line is uncoloured, as today. The padding before the path is unchanged, so paths still line up.

The text is the `Applied` items joined by `, `, each `<family> <model>[ <detail>]`, then the cache marker (D4). An
empty chain reads `no enhancement needed`.

In the live view, the row is clipped at the terminal's width by `tui` (no wrapping, no change in height). On a narrow
terminal the tail of a long chain may be cut; the plain mode and the log carry it whole. `progress::ImageProgress`
stores the text when the image ends so the live line can redraw it.

### D4. The cache marker comes from the progress reports

`ImageProgress::report` keeps, beside the row state, the set of families whose operation reported `Stage::Cached`
(only for an enhancement subject, not the face detection, which a cold run also serves from the store after
autopilot's analysis warmed it). When the image ends, with `k` cached of `n` operations:

- `k == n > 0`: the list ends ` (cached)`
- `0 < k < n`: ` (<k> of <n> cached)`
- `k == 0`: nothing

`run.rs` cross-checks against the verdict: `ProviderVerdict::NothingExecuted` with a non-empty chain forces
`(cached)`, so a run whose reports were throttled or skipped still says it was served.

*Alternative:* add a per-step "served from cache" list to `opai`'s `Enhanced` result. It is the more principled home
(the GUI could use it too), but it widens a library type every front end consumes for a fact the progress stream
already delivers. Not taken now; if the GUI later wants a "from an earlier run" badge, that change can move the tally
into `opai` and this one would read it from there.

### D5. The log records the same text, with or without `--verbose`

The existing `wrote <path> (<w>x<h>, <format>); <provenance>` record gains `; applied <text>`, so a batch whose rows
scrolled out of the live list, or that ran without `--verbose`, can be reviewed afterwards, and the log and the
terminal never disagree. The log is diagnostics out of sight, so recording it always changes nothing the user sees.

### D6. Shown only under `--verbose`

The project owner chose opt-in: a run without the flag prints exactly what it printed before, so nobody's terminal or
scripts change, and the user who wants the detail asks for it. `Args` gains `verbose: bool` (`--verbose`, long only);
`run.rs` passes `ImageEnd::Enhanced(args.verbose.then_some(text))`. Everything else (building `Applied` in `plan()`,
the cache tally, the log record) runs whatever the flag, so the flag gates one string and nothing else can drift.

No `-v`: upstream maps it to `--version`, specified and documented. Moving it would break a documented invocation for
a convenience, so `--verbose` has no short form. If the owner prefers `-v` for verbose, it is a one-line change in
`args.rs` plus the version test and the reference, and belongs in its own decision.

The flag is declared on `Args` and specified inside this change's ADDED requirement, not by modifying "The command
takes its inputs, its enhancements, and an output": `cli-output-default` modifies that requirement, and two changes
replacing the same requirement would make whichever archives second drop the other's text.

## Difficulty and likely paths

**Difficulty: easy to moderate.** About 1-2 days with tests and docs. The data is all in hand; the work is threading
one string from `plan()` to the ended row and tallying one progress stage, with exact-text tests in `look.rs` and
`progress.rs`.

| Path | Effort | Risk | Notes |
|---|---|---|---|
| End-row tail under `--verbose` (this change) | ~1-2 days, 9 tasks | Low | No layout change; `cli` only; default output unchanged. |
| A separate per-image line under the view, or below the summary | ~1-2 days | Medium | Changes the view's height or the summary's shape, both specified in detail; long batches print hundreds of lines. |
| `--json` NDJSON on stdout (follow-up `cli-report-json`) | ~2-3 days | Medium | One object per image: `{input, output, status, enhancements: [{family, model, precision, parameter}], cached, providers, faces}`, and a final summary object. Needs a "standard output stays empty" exception, a serde shape to keep stable, and `serde_json` in `cli` (already a dependency of `opai`, `gui` and `perf`, so no new crate for the workspace). Best for scripts; does not help the interactive user, so it complements this change rather than replacing it. |
| `--verbose` printing the existing log records to stderr | ~1 day | Medium | Cheap to wire through `tracing`, but the records are developer-worded and noisy, and would break the view's layout. |

**Risks:**

- *Long rows on narrow terminals.* A five-step chain is about 110 columns after the path. The live row clips; plain
  mode and the log do not. Acceptable, and documented; a later change could abbreviate (`Dn Stockholm 100`) below a
  width threshold.
- *Scripts matching the ended line exactly.* Unaffected unless they add `--verbose`.
- *Discoverability.* The autopilot user who does not know to ask will not see the choice. `--help` and the reference's
  "Autopilot" section point at `--verbose`.
- *Cache tally and partial runs.* A stopped or failed image prints no `<applied>`, so the tally only matters for
  written images, where every operation has reported. The verdict cross-check (D4) covers a missing final report.
- *Parameter wording vs the GUI.* The GUI's sidebar labels strength and bias in its own words and translations; the
  CLI uses the flag vocabulary (`strength`, `bias`), which matches `--dn-str`/`--la-bias` and the reference.

**Upstream acceptability:** good. It softens a Non-Goal written by the same project, so the rationale in D1 matters;
but it is opt-in, changes nothing without the flag, keeps every library type unchanged, and fixes a real usability gap
that `--ap` created.

## Platforms

Terminal text only. To be validated by hand on macOS (iTerm2/Terminal, CPU and CoreML) in live mode, plain mode
(`2>file`) and `NO_COLOR=1`. Unit tests run on all three CI platforms. Windows Terminal and Linux terminals stay
untested by hand; the `ö` and `ã` in `Malmö` and `São Paulo` are already printed in paths by the same code path, and
widths are measured as the row's other text is. No execution provider is involved beyond reading the existing verdict.

## Migration Plan

None. No stored state changes.

## Open Questions

- ~~Default on for every run, or only under `--ap`, or behind a flag?~~ Decided: behind `--verbose` (D6).
- `-v` for `--verbose`? It prints the version today (D6); left as is unless the owner decides otherwise.
- Show the face count on a face recovery (`Face Recovery Athens, 2 faces`)? It is useful, but "Nothing printed outside
  the layout" currently keeps it in the log.
- Follow with `cli-report-json` now, or wait for a scripting user to ask?
