## Context

See proposal.md - Why. These facts were checked in the code and shape the design:

- **`-o` is a required `PathBuf`.** `args.rs` declares `pub output: PathBuf` with no default, and `override_usage`
  spells `--output <OUTPUT>`. `args::tests::the_output_is_required` pins the `MissingRequiredArgument` error.
- **`output::target(&Args)` turns `-o` into a `Target`**, `File { path, format }` or `Directory { path, naming }`,
  before anything is installed. `--format`, `--prefix` and `--suffix` are refused only for a `File`.
- **`output::destination(&Target, &Source)` already calls the library's
  `export::destination_for(&source.path, naming, Some(&folder))`.** Passing `None` instead of `Some(folder)` makes the
  library put the result in `source.path.parent()`, the GUI's "original directory" placement
  (`openspec/specs/export-naming` - "The file goes into the chosen folder, or beside its source"). Nothing new is
  needed in `opai`.
- **`run.rs` uses `args.output` in two places:** the `skip` directory handed to `inputs::expand` (only for
  `Target::Directory`) and the batch success line (`look::summary(written, output)`).
- **`inputs::expand` already copes with an output that is, or holds, the input**: it skips nothing then
  (`an_output_directory_that_is_the_input_or_holds_it_skips_nothing`). Without `-o` there is no output directory, so
  `skip` is `None`.
- **Numbering and `--overwrite` live in `export::write`**, which the CLI calls with `create_parent`; a source's own
  directory always exists, so `create_parent` is irrelevant there.
- **The GUI's export defaults** are "no prefix, no suffix, original directory" (`gui-export` - "The export settings
  decide each file's name, folder, format and quality"). The GUI user sees the queue and the dialog before anything is
  written; the CLI user does not, which is why this design adds a default suffix the GUI does not have (D2).

## Goals / Non-Goals

**Goals:**

- `opai photo.jpg --ap` works and writes `photo_opai.jpg` beside `photo.jpg`.
- No run that gives `-o` changes in any way: same files, same names, same lines, same exit status.
- A run without `-o` can never replace a source unless the user both turns the default suffix off and passes
  `--overwrite`.
- One placement rule, the library's; the CLI only decides whether a folder is passed.

**Non-Goals:**

- Treating the current directory as an output directory (Alternative A below).
- A configurable default suffix (an environment variable or a config file). `--suffix` already exists.
- Keeping a later run over a directory from picking up an earlier run's `_opai` results. A re-run over `photos`
  without `-o` sees `photos/a_opai.jpg` as an input, just as `-o photos` does today. Documented, not prevented: an
  exclusion by name pattern would silently drop a user's own file that happens to end in `_opai`.
- Any change to the GUI or to the library's naming rule.

## How the work divides

**All in `cli`.** `opai` already owns the placement rule (`export::destination_for`) and the write (`export::write`);
this change only calls the former with `None`. The GUI is untouched: its export dialog has its own persisted folder
setting and never sees CLI arguments.

## `rust-sak` coverage

| Need | Covered by | Feature |
|---|---|---|
| Destination beside the source | `opai::export::destination_for(.., None)`, already shared | - |
| Writing, numbering, atomic overwrite | `opai::export::write`, unchanged | - |

No infrastructure is involved, so nothing in `rust-sak` is used or missing. No third-party crate is introduced.

## Decisions

### D1. No `-o` means "beside each source", not "into the current directory"

`Args::output` becomes `Option<PathBuf>`. `Target` gains a third variant, `Beside { naming }`, which `target()`
returns when `output` is `None`. `destination()` maps it to `export::destination_for(&source.path, naming, None)`. For
a source given as `photo.jpg`, its parent is the empty path, so the destination is `photo_opai.jpg`, relative to the
current directory: the user's expected `./photo_opai.jpg`.

*Alternative A, the current directory as an implicit `-o .`:* a one-line `default_value = "."`. It gives the same
answer for a photograph in the current directory, and different, worse answers everywhere else:

- `opai ~/Pictures/trip -r --ap` from `$HOME` would mirror the whole `trip` tree (its subfolders included) into
  `$HOME`.
- `opai a/x.jpg b/x.jpg --up` would write `x.jpg` and `x_1.jpg` side by side in the current directory, the two results
  no longer telling which source they came from.
- `.` holds every directory input below it, so the existing "output directory not taken as input" rule does nothing,
  and a re-run would re-enhance the first run's results there too.

"Beside the source" has none of these: each result sits next to the file it came from, which is also the GUI's default
and what the library already computes. It is listed as an open question in case the user prefers the literal "current
folder".

*Alternative B, a usage error suggesting `-o .`:* keeps the status quo with a nicer message. Rejected: it does not do
what the user asked.

### D2. The suffix defaults to `_opai`, only when `-o` is omitted and no affix is given

In `target()`, for `Beside` only:
`suffix = args.suffix.clone().unwrap_or_else(|| if args.prefix.is_some() { String::new() } else { "_opai".into() })`.
So:

| Given | Name of `photo.jpg`'s result |
|---|---|
| nothing | `photo_opai.jpg` |
| `--suffix -big` | `photo-big.jpg` |
| `--prefix new-` | `new-photo.jpg` |
| `--suffix ""` | `photo.jpg` (numbered `photo_1.jpg`, or the source replaced with `--overwrite`) |

Why a default at all, when the GUI has none: without `-o`, the destination folder is the source's, so with no affix
every result collides with its own source. Numbering protects the source, but `photo_1.jpg` reads like a camera
duplicate, and `--overwrite` (a flag a user may carry over from an `-o out` habit) would replace every original. The
default makes the dangerous case need two deliberate flags (`--suffix "" --overwrite`).

Why `_opai`: it is what the user typed, and it matches the `_1` numbering's underscore. The existing docs' examples
use `-opai`; either is fine and it is listed as an open question.

Why only without `-o`: with `-o` the user has chosen a place, and today's "no affix" default must not change (Goal 2).

*Alternative:* refuse `--overwrite` without `-o` unless an affix is given. Safer than nothing, but it still leaves the
default run writing `photo_1.jpg`, which is not what the user expected. Rejected in favour of the default suffix,
which handles both.

### D3. The batch success line names no directory

`look::summary(written, output)` takes `Option<&Path>`. With `None` and more than one file written, it reads
`✨ <count> images enhanced successfully; files saved beside their sources`. One file still names the file written,
numbered or not. `run.rs` passes `args.output.as_deref()`.

This is specified in the new requirement rather than by modifying "Progress is shown while the command works", because
the concurrent change `cli-report-enhancements` modifies that requirement; two changes replacing the same requirement
would make whichever archives second silently drop the other's text.

### D4. Everything else follows from `Beside` being "not a file"

- `--format`, `--prefix`, `--suffix`: the refusal stays tied to `Target::File`, so they are accepted for `Beside` as
  for `Directory`.
- `skip`: `None` for `Beside`, as for `File`.
- `format_name` (the analytics event): `Beside` reports its naming's format, as `Directory` does.
- `write`: `create_parent` is false for `Beside`; the source's directory exists by construction.
- `--recursive`: unchanged in what it finds; each found image is written beside itself, so the tree's shape is
  preserved by construction and no folder is created.

### D5. `--overwrite` never replaces another input

Without `-o`, a result's name can be another input's: `opai a.jpg a_opai.jpg --overwrite` (or a re-run over a tree
that finds its earlier `_opai` results) would replace the user's `a_opai.jpg` with `a.jpg`'s result, and, in that
order, before `a_opai.jpg` was even read. That breaks Goal 3. So `run.rs` records every expanded input, canonicalised
(`output::keep_inputs`, held on `Target::Beside`), and before each write `output::may_overwrite` withholds
`--overwrite` where the destination is one of them and not the image's own source. The write then numbers the result,
as it does without `--overwrite`, and the log records a warning.

Numbering rather than failing the image: the user asked for an enhanced copy and gets one, and nothing is lost; a
failure would turn a re-run of a whole tree into a batch of errors. The image's own source stays replaceable, since
that takes the two deliberate flags D2 requires. Skipping inputs that look like earlier results is still not done
(Non-Goals): this rule only stops the clobbering.

Under `-o`, the same clobber is possible today (`opai a.jpg a-x.jpg --suffix -x --overwrite -o .`), and is upstream's
existing behaviour; Goal 2 keeps `-o` runs unchanged, so the rule applies only beside the sources. Extending it to
`-o` would be one line (`keep_inputs` on every target) and is left to its own change.

## Difficulty and likely paths

**Difficulty: easy.** About a day including docs and tests. The placement rule already exists in `opai` and is already
called by the CLI; the change is an `Option`, one enum variant and one line of wording.

| Path | Effort | Risk | Notes |
|---|---|---|---|
| D1 + D2 (recommended) | ~0.5-1 day, 8 tasks | Low | New `Target` variant touches four `match`es, all in `cli`. |
| Alternative A (`default_value = "."`) | ~1 hour | Medium | Trivial code, surprising results for any input outside the current directory (D1). |
| Alternative B (better error only) | ~30 min | None | Does not deliver the feature. |

**Risks:**

- *Re-runs pick up earlier results.* A second `opai photos --ap` enhances `photos/a_opai.jpg` too. Same as `-o photos`
  today; documented. Mitigation if it bites: a later change could skip files whose name ends in the run's own suffix,
  with a log line.
- *Read-only source folders* (a mounted card, a network share) fail per image with a write error naming the
  destination. The message already names the path; the docs suggest `-o`.
- *Relative display paths.* The success line names `photo_opai.jpg`, without `./`. That is what the library returns
  for a relative source and matches how inputs are shown.
- *clap and an empty `--suffix ""`.* clap accepts an empty value for a `String` option by default; a test pins it,
  because D2's in-place escape hatch depends on it.

**Upstream acceptability:** high. The change is additive (no existing invocation changes), front-end only, reuses the
library's own rule, and matches the GUI's default placement. The one opinionated piece is the `_opai` default suffix;
if upstream prefers strict GUI parity (no affix, numbering only), D2 drops out without touching D1, D3 or D4, at the
cost of `photo_1.jpg` names.

## Platforms

Pure path handling through `std::path` and the library's existing export code, so nothing is platform-specific. To be
validated by hand on macOS. Unit tests run on all three CI platforms. Windows path separators in the success line
(`dir\photo_opai.jpg`) are whatever `Path::display` gives, as today. No execution provider is involved.

## Migration Plan

None. Existing invocations all pass `-o` and are unchanged.

## Open Questions

- ~~Beside the source (recommended) or the literal current directory (Alternative A)?~~ Decided by the project owner:
  beside the source.
- ~~Default suffix `_opai` (the user's spelling) or `-opai` (the docs' examples), or none (GUI parity)?~~ Decided by
  the project owner: `_opai`.
