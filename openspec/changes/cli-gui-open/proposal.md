> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

A user at the terminal who wants to look at photographs in the window, crop them or compare enhancements before
exporting, has no way to get them there from the command line: they start the application and add the photographs
again with the picker or a drop. `opai` cannot help either, because a run is refused while the GUI holds the
single-instance claim. Every other photo tool's command line can hand files to its application (`code file`,
`open -a`); `opai` already knows how to find the photographs in a mix of files, wildcards and directories, so it
should be able to hand them over too.

## What Changes

Task estimate: **8 tasks**, one change, the minimal version. It **depends on `gui-macos-open-with`**, which makes the
GUI open the files the platform hands it (macOS Apple Events, launch arguments, and a second launch's arguments through
the single-instance plugin); the code is one commit stacked on that bookmark (`cli-gui-open`).

- A new flag, `opai --gui <INPUT>... [-r]`: the inputs are expanded and checked exactly as for a run (directories, `-r`,
  the supported-extension filter, the header probe), and every image found is handed to Open Photo AI, which opens it
  whether or not it was already running.
- Neither an enhancement nor `--output` is required under `--gui`.
- Every other option given on the command line is ignored, and one warning on stderr names them and says to set them in
  the GUI. `-r` is used, so it is not named.
- `--gui` ends before telemetry starts and before the library is initialized, so it never takes the single-instance
  claim and works while the GUI is open.
- A short confirmation, `🖼️ Opening N images in Open Photo AI`, exit status 0; a failure to start the GUI exits 1 with
  a message saying why.

**Future work, not in this change:** passing the run's settings to the GUI (the enhancement flags, `-q`, `-p`, the
export naming), so `opai photos --up osaka --gui` would open the photographs with Osaka already chosen. That needs a
way for a launch to carry settings to a running instance, and a decision on whether they replace the window's own;
this change warns instead.

## Capabilities

### New Capabilities

### Modified Capabilities

- `cli-enhance`: adds a requirement that `--gui` hands the inputs to the GUI instead of running, without an output or an
  enhancement, warning about the other options, and before the single-instance claim and telemetry.

## Impact

- Crates: `cli` only (`src/args.rs`: the flag, `--output` now `Option<PathBuf>` with `required_unless_present = "gui"`,
  the ignored options collected from clap's `ArgMatches`; a new `src/gui.rs`; `src/main.rs`; a warning line in
  `src/look.rs`; a `Launch` error in `src/error.rs`). `docs/CLI_REFERENCE.md`.
- `opai`, `gui`: nothing. The GUI side is `gui-macos-open-with`'s.
- No new crates; the CLI's messages are not localised, so no strings files change.
- Base: `gui-macos-open-with` (on `main@upstream`). Upstream-able after it.
