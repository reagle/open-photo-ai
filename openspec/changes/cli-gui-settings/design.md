## Context

See proposal.md - Why. These facts were checked in the code and shape the design:

- **The GUI already mirrors one preference to a file Rust reads.** `crates/gui/src/analytics.rs` keeps
  `analytics.json` in `opai`'s configuration directory (`rust_sak::fs::user_config_dir(opai::APP_NAME, FILE)`, which
  creates nothing), written through `fs::mk_temp_file_in` + `persist` and skipped when unchanged. The window's
  `lib/analytics.ts` sends the stored value once at boot and again from a `useSettingsStore.subscribe` when it
  changes. The file follows the store and is never read back by the window. This change repeats that pattern for three
  more preferences, with the format moved into `opai` because a second front end reads it.
- **Every write to the settings store is a Save.** `stores/settings.ts` is written only by `apply` (the dialog's Save)
  and `answerTensorRT`; drafts live in `features/settings/draft.tsx`. A store subscription therefore never sees a
  draft, and Cancel never reaches it.
- **The GUI has no global quality.** `models[family]` stores `<codename>_<precision>` (for example `tokyo_fp16`), and
  `lib/enhancements.ts` reads the tier positionally: `precisions[0]` is HD, `precisions[1]` SD. A family the user
  never chose for is absent and runs the catalogue's first variant at its first precision. The CLI's `-q` uses the
  same positional rule (`Quality::position`), so a per-family tier is the faithful translation.
- **The GUI's spellings are the library's.** `Family` serializes snake_case (`color_balance`), which is what
  `ipc/catalogue.ts` and `autopilotExcluded` use; the processor is lowercase (`coreml`), which
  `ExecutionProvider::from_str` accepts case-insensitively.
- **The CLI's processor check refuses** a processor this machine does not offer (exit 2). The GUI instead falls back
  to `auto` where a saved processor is not offered. A processor taken from the GUI must follow the GUI's rule, or a
  file written on one machine could make the CLI refuse to run on another.
- **`dirs::config_dir` honours `$HOME` on macOS and Linux**, so a run under a temporary `HOME` reads a temporary file;
  that is how the manual verification avoids the user's real configuration directory. No `OPAI_CONFIG_DIR`-style seam
  exists, and none is added.

## Goals / Non-Goals

**Goals:**

- One format, one writer and one reader, in `opai`; the CLI's precedence decided in `cli`.
- The GUI's behaviour is unchanged by construction: it writes the file and never reads it.
- A missing, damaged or newer file never fails a CLI run.
- `--verbose` answers "why did this run at SD?" without opening the log.

**Non-Goals:**

- The GUI's model for the bare enhancement flags (`--up` runs Tokyo, not the GUI's upscale model). clap fills a bare
  flag with its `default_missing_value`, so telling `--up` from `--up tokyo` needs a change to every model flag; a
  follow-up can do it if wanted.
- Export settings (format qualities), language, background or analytics: the CLI has no flag for them, or (analytics)
  already has its own file.
- Writing the file from the CLI, or the GUI reading the file.
- Per-family parameters (strength, bias): the GUI stores none as preferences.

## How the work divides

| Crate | What it gets |
|---|---|
| `opai` | `settings` module: `SharedSettings` (processor, Autopilot exclusions, per-family `ModelChoice` resolved against the catalogue), `SharedSettings::from_gui`, `path(name)`, `read_at()` → `Loaded`, `write_at()`; entries kept in apply order |
| `gui` | `set_gui_settings` Tauri command → `opai::settings::write_at`; `ipc/guiSettings.ts`; `lib/guiSettings.ts` `mirrorGuiSettings()` called from `main.tsx` |
| `cli` | `--no-gui-settings`; `-q`/`-p` become optional; `settings.rs` resolves processor and per-family quality with their sources; `AutopilotModels::read` takes the file as the layer below the variables; `plan` takes per-family qualities; `look` prints the sources under `--verbose` |

## `rust-sak` coverage

| Need | Covered by | Feature |
|---|---|---|
| The file's path, creating nothing | `fs::user_config_dir` | `fs` |
| Atomic write | `fs::mk_temp_file_in` + `NamedTempFile::persist` | `fs` |
| JSON | `serde_json`, already an `opai` dependency | - |

Nothing is missing, so `rust-sak` is not changed. No third-party crate is introduced.

## Decisions

### D1. The file is the GUI's choices, resolved by `opai`, not a CLI-shaped file

The file stores what the GUI stores (processor, exclusions, `model` + `precision` per family), not CLI vocabulary such
as `hd`. The tier is derived by the reader from the catalogue, so a file stays meaningful if a model gains a
precision. `SharedSettings::from_gui(processor, excluded, models)` takes the window's raw strings and resolves each
`<codename>_<precision>` by comparing it with every published `codename_precision`, never by splitting on `_`, which
is the rule `lib/enhancements.ts` keeps for the same reason.

Format, version 1:

```json
{ "version": 1, "processor": "CoreML", "autopilot_excluded": ["colorization", "color_balance"],
  "models": { "upscale": { "model": "osaka", "precision": "int8" } } }
```

*Alternative:* the window builds the file's JSON in TypeScript. Rejected: the format would then live in the front end
that does not read it, and the CLI's reader would have no shared type to agree with.

### D2. The reader is lenient by entry and strict by version

`read_at(path) -> Loaded` with `Loaded::{Absent, Ignored(reason), Read(SharedSettings)}`. Absent is silent. A file
that cannot be read, is not a JSON object, or has a `version` other than 1 is `Ignored`, logged at `warn`. Inside a
version-1 file, each field and each entry is parsed on its own from a `serde_json::Value`: an unknown key is skipped,
and a processor, family, model or precision that does not resolve is dropped with a `warn`, keeping the rest. A newer
GUI can therefore add keys without a version bump; a version bump is reserved for a change of meaning.

### D3. The window mirrors through a subscription, like the analytics choice

`mirrorGuiSettings()` sends `{ processor, autopilotExcluded, models }` once at boot and again from
`useSettingsStore.subscribe` when any of the three changed (compared by value, so Reset to defaults handing back equal
records sends nothing), and reports a rejection through `report` like the analytics mirror. It sits in `lib/`, not in
the store, so the store stays free of IPC and the shortcuts series' edits to `stores/settings.ts` do not conflict with
it. The command is traced (it has nothing private to protect), wrapped in `command_span!("set_gui_settings", …)`, and
does the file IO on `task::spawn_blocking`.

### D4. Precedence is decided once, in `args::parse`, with its source kept

`-q` and `-p` become `Option`s in clap (their help still says the default). `args::parse()` reads the file unless
`--no-gui-settings`, then resolves `Settings { processor: Sourced<Processor>, qualities: Qualities }`, where
`Qualities` is one `Sourced<Quality>` per family in apply order and `Source` is
`Flag | Variable(name) | Gui | Default`. `parse_from` resolves with no file, so every existing test sees today's
behaviour. Under `--ap`, `AutopilotModels::read(lookup, gui)` checks the variable first, then the file (exclusion →
off, model → that model), then the default, and records each family's source. `chain::plan` takes `&Qualities` and
asks it per family.

A processor from the file is resolved in two steps: `Processor::from_provider` (not nameable on this OS → `auto` with
a warning), then after initialization `run` checks it against the machine's providers and, only when its source is the
file, falls back to `auto` with a warning instead of refusing.

### D5. What `--verbose` shows

The processor line gains a source after each value: `Auto (from GUI settings); quality SD (from GUI settings)`. A
mixed quality reads `mixed (Upscale SD from GUI settings; the rest HD by default)`. Under `--ap` an autopilot line
follows, `🤖 Autopilot models: Denoise Stockholm (default), …, Color Balance off (from GUI settings), …`. The log
records which file was read, or why it was ignored, with or without `--verbose`. Without `--verbose` the only visible
difference is the values themselves and `quality mixed`.

## Risks / Trade-offs

- **[A surprise for scripts]** A script that relied on the built-in defaults changes behaviour once the GUI saves
  settings. → `--no-gui-settings`; the log and `--verbose` name the source; the docs say so.
- **[Stale file]** The file reflects the GUI's last launch or Save, so a `localStorage` edited by another build is
  only mirrored at the next launch. → It is the same staleness the analytics mirror accepts.
- **[Mixed quality reads oddly]** `quality mixed` is new wording. → It only appears when the GUI's own choices differ
  by family, which is the GUI's behaviour too; `--verbose` itemises it.
- **[Upstream may prefer a real settings store in `opai`]** → This file is small and versioned, and the module is
  named `opai::settings` so a later store can absorb it.

## Platforms

Validated by hand on macOS (CoreML, `-p auto`). The unit tests run on all three CI platforms; Windows and Linux paths
(`%APPDATA%`, `~/.config`) come from `rust-sak` and are untested by hand. The webview mirror is tested with Vitest;
the GUI was not launched by hand in this change.

## Migration Plan

None. The file appears the first time the GUI launches with this change; until then the CLI behaves as before.
