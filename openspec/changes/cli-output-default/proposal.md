> **AI disclosure:** this change is being planned and implemented by Joseph Reagle working with Claude (Anthropic's AI assistant, via Claude Code). Joseph reviews and is responsible for it.

## Why

`-o/--output` is required today, so the most natural one-liner fails before doing anything:
`opai --autopilot --quality sd --suffix _opai photo.jpg` ends with
`error: the following required arguments were not provided: --output`. A user who just wants an enhanced copy of a
photograph next to it has to spell out `-o .`, and even then a run over `../other/photo.jpg` or over a directory tree
lands somewhere they did not expect. The GUI already has an answer to "where does a result go when nobody chose a
folder": its export defaults to the source's own directory, and the library's naming rule (`export::destination_for`
with no folder) already implements it. The CLI should take the same default.

## What Changes

Task estimate: **8**. One change, not a series. It is independent of `cli-report-enhancements` (no shared requirement,
either may land first).

**In `crates/cli`, observable on the command line:**

- **`-o/--output` becomes optional.** The usage line reads `opai <INPUT>... [OPTIONS] [--output <OUTPUT>]`. Every run
  that gives `-o` behaves exactly as today.
- **Without `-o`, each result is written beside its source**, in the directory that holds the source file, named by
  the same export rule as under an output directory (prefix, source name, suffix, extension of `--format`). For a
  photograph in the current directory this is the current directory: `opai photo.jpg --ap` writes `./photo_opai.jpg`.
  A named file elsewhere (`opai ~/Pictures/a.jpg --up`) is written beside it in `~/Pictures`, and an image found in a
  directory input, `-r` or not, is written beside itself in that tree, so nothing is mirrored and nothing is scattered
  into the current directory.
- **Without `-o` and without `--prefix` or `--suffix`, the suffix defaults to `_opai`.** The result's folder is the
  source's own, so its name is the only thing that tells the two apart. `photo.jpg` gives `photo_opai.jpg`, not
  `photo_1.jpg` (which reads like a camera duplicate), and `--overwrite` then replaces an earlier `_opai` result,
  never the source. Giving `--prefix` or `--suffix` (an empty `--suffix ""` included) turns the default off, so true
  in-place work is still possible but has to be asked for: `--suffix "" --overwrite`, or `-o photos --overwrite` as
  today.
- **`--format`, `--prefix` and `--suffix` are accepted without `-o`**, as they are with an output directory.
- **Numbering is unchanged:** without `--overwrite`, an existing `photo_opai.jpg` is kept and the result is
  `photo_opai_1.jpg`.
- **`--overwrite` never replaces another input.** Where a result's name is another file given or found as input
  (`opai a.jpg a_opai.jpg --overwrite`, or a re-run over a tree holding earlier results), that file is kept and the
  result numbered, with a warning in the log.
- **The success line for several images** reads
  `✨ <count> images enhanced successfully; files saved beside their sources` when no `-o` was given. The single-image
  line already names the file written and is unchanged.
- **Exit codes are unchanged.** A missing `-o` is no longer a usage error; a source directory that cannot be written
  to fails that image (exit 1, or the batch status), exactly as an unwritable output directory does today.
- **Docs:** `docs/CLI_REFERENCE.md` (synopsis, the rules in short, `-o` no longer "(required)", a "No `-o`: beside
  each source" subsection under "Where the results go", the `_opai` default, quick-start examples without `-o`), and
  `--help`.

**Unchanged:** every run that gives `-o`. The GUI. The library.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cli-enhance`: "The command takes its inputs, its enhancements, and an output" makes `--output` optional and lets
  `--format`, `--prefix` and `--suffix` apply whenever the output is not a file; a new requirement, "Without an
  output, each result is written beside its source", covers the placement, the `_opai` default suffix, `--overwrite`,
  directory inputs and the batch success line.

## Impact

**Crates touched:** `cli` only (`args.rs`, `output.rs`, `run.rs`, `look.rs`, `main.rs` module docs) and
`docs/CLI_REFERENCE.md`.

**Why the logic is in the front end:** the placement rule itself is already shared and stays in `opai`:
`export::destination_for(source, naming, None)` puts a result beside its source, which is what the GUI's "Save to:
original directory" uses. What the CLI adds is argument handling (an optional flag, a default suffix tied to that
flag's absence) and the wording of one terminal line. Neither is something the GUI needs: the GUI's export dialog has
its own persisted defaults and no notion of "the flag was omitted".

**`opai`:** unchanged.

**`rust-sak`:** nothing needed. No infrastructure is involved: no IO beyond the existing export write, no new paths,
no config. Checked `rust-sak`'s `fs` module (config dirs, temp handles, extraction); none of it applies.

**Third-party crates introduced:** none.

**Breaking:** none for existing invocations. A script that relied on a missing `-o` failing with exit 2 would now
write files; that is the point of the change.
