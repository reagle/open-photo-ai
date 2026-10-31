## Context

The first version of this change (`add-restore-grain`, four commits on `add-restore-family`) put grain inside Restore: a
`grain` amount on `Restore`, a `Grained` wrapper outside the resample, `--rs-grain`, a slider in the Restore panel. Its
measurement and synthesis live in the shared `imaging::grain` (eight luma bands, a median-absolute 3 x 3 residual, a
[1 2 1]-smoothed field of shared and per-channel draws from a SplitMix64 hash of the output's coordinates); Remove's fill
grain draws from the same hash. Joseph asked for grain to be an enhancement of its own, applied last, after the lens
blur too. This change rebuilds it as a family on top of that work (`add-grain-family`, four commits on
`add-restore-grain`: imaging, opai, CLI, GUI) and takes Restore's grain out.

Research: `~/dev/opai-notes/research/restore-grain-2026-10-31.md` (what other apps do, the noise models).

## Goals / Non-Goals

**Goals:** an enhancement that puts back the photograph's own grain wherever the chain took it away, or adds a
sensor's grain on purpose; never by default; the same pixels on every run; nothing else changed.

**Non-Goals:** film-grain emulation (clumped, stock-specific; a later variant beside Sensor); Size and Roughness
controls (Lightroom's), not needed now; grain in the gain map.

## Decisions

### D1. A family, last of all, after the lens blur

`Family::Grain`, prefix `gr`, the last entry of `Family::ALL` and so of `APPLY_ORDER`. The first version argued the
reverse (a grain after the lens blur would grain the blurred background "with in-focus grain"); that argument is wrong
for a sensor's noise: a real photograph's blurred background still has the sensor's noise at the pixel level, since
noise is added at capture, after the optics. A synthetic blur without it is the tell. With D3's tile-by-tile
measurement, the background gets the photograph's noise back and the subject, which kept its own, gets next to none.
Last also means it follows Upscale, Sharpen and Face recovery, whose results are as plastic as a restore's.

One variant, **Sensor** (FP32: the precision of its arithmetic; one precision, so no tier label), no artifacts, no
session, `model_tag` `gr_sensor_fp32` for metrics. A film variant can join it later. Autopilot never suggests it
(no signal) and nothing adds it by default.

### D2. Two controls: Amount and Match source

Lightroom's Grain is an Amount (with Size and Roughness); this keeps the Amount and adds one switch:

- **Match source** (on, the default): the amount is the share, in standard deviation, of the photograph's grain the
  result lacks that is put back. 100% matches the photograph; a clean photograph gets nothing at any amount.
- **Off** ("add"): the amount is a share of a sensor's high-ISO grain, `ADDED_AT_FULL` = 4 levels of 3 x 3 residual at
  mid-grey (about 8 levels of deviation), shaped by brightness (`imaging::grain::sensor`: a Poisson-Gaussian sensor,
  variance `Y + 0.002` in linear light, through the sRGB curve's slope, so most in the shadows), correlation 0.85,
  added everywhere whatever is there. Grain on purpose.

A switch rather than a mode selector, because there are two modes and one of them is the default. The amount defaults
to 100% because a user who adds Grain asks for the photograph's noise back; in add mode 100% is a strong grain, as
Lightroom's is. The catalogue publishes `amount` (0-1, default 1) and the switch as `match`, a range from 0 (add) to 1
(match), default 1, read from a half up as matching (`GrainMode::from_switch`): the catalogue's only other kinds are a
range and the faces, and a front end draws this one as a switch.

### D3. A profile per luminance band from the photograph, then both pictures tile by tile

The photograph's `Profile` is measured as before (eight bands, median-absolute 3 x 3 residual, sparse bands from their
neighbours, the correlation), then floored: a band under 0.75 / 255 has only a gradient's rounding, nothing to put back.

Both pictures are then measured **per 32 x 32 tile** against that profile (`imaging::grain::lacking`): each inner
pixel's residual per channel over the profile's deviation at that pixel's own luminance, pooled; a tile's share of the
band's grain is 1.4826 x the median of those ratios (so an edge in the tile is ignored). The photograph's share `had`
and the result's `has` are read at the same place (the same fraction of each picture, bilinearly between tile centres),
and the noise is drawn at `amount x sigma(luma) x sqrt(max(0, had^2 - has^2))`. A tile with fewer than 32 usable
residuals holds no share.

Why per tile, and on both: the first version's whole-picture band figures could not tell a blurred background from the
subject of the same brightness (one would get double grain or the other none), nor a smoothed cheek from hair of the
same brightness into which Kazan drew texture (its open question 4). Measuring only the result per tile against the
band's figure (this change's first build) fixed that but topped up every tile whose own noise sat below its band's
median: a real photograph's noise is uneven, so a result that was its photograph, or a denoise's result matched against
itself, was given 7 to 36 percent more (see Results). Comparing the same place in both gives such a result nothing. 32
px keeps a tile's median within a few percent and follows an outline to within a tile. Cost: one pass over each
picture on all cores.

### D4. Synthesis

Unchanged from the first version: four normals per output pixel from the salted SplitMix64 hash (salt `grain!!!`,
apart from Remove's), shared with weight `sqrt(c)` and per channel `sqrt(1 - c)`, smoothed [1 2 1] / 4 on both axes and
renormalised, the residual-to-deviation factor `sqrt(19) / 9` exact. Generic over 8 and 16 bits; alpha untouched.

### D5. Scale: the grain is at the output's pixel size

The photograph is measured at its own size and the result at its own; under an enlargement the result is asked for the
photograph's per-pixel grain at its own, smaller pixels: what a sensor of the output's resolution would record, finer at
print size than the source's and never an enlarged noise field. As the first version's D5, now for Upscale too.

### D6. The photograph matched: the chain's input, or what its last denoise made

A denoise is a request for less grain, so where the chain denoises, the grain matches the denoise's result, not its
input. The rule is `models::grain::is_grain_reference` (Denoise only); the driver asks a step whether it
`measures_against_photograph` and hands it the photograph through `ImagePipeline::run_against` (both defaulted, so no
other pipeline changed). The driver keeps the photograph only while a later step asks for one: a computed denoise's
result is shared; a denoise served from the run cache leaves its bytes, decoded only if the grain itself is computed
(and not at all when it is the grain's own input); bytes that will not decode rewind the chain with the store
distrusted up to and including the denoise. A cached grain needs nothing.

The grain's cache key needs no photograph in it: it is folded from the chain's input and every step before it.

### D7. The limiting mask does not limit grain

On `integration`, a limiting mask is blended after the removals and before the lens blurs. The chain is now split
before the trailing run of lens blurs **and** grain, and the grain runs over the blend: inside the mask it puts back
what an enhancement smoothed, outside it the picture is the photograph with its own noise and gets next to none (D3),
and under a lens blur the blurred background, inside the mask or out, gets the noise a real one has. A grain limited
by the mask would leave the blurred background outside it without the noise its subject keeps. The head hands the tail
its photograph (`Ran::photograph`, `Steps::measured_after`), so a masked denoise is not undone either. Add mode adds
everywhere, as asked.

### D8. Per-enhancement masks

Under `add-masking-6-per-enhancement` (its D1 list), Grain is maskable like any enhancement: a Grain mask limits where
grain is put back or added. Updated in that proposal on its bookmark.

### D9. Determinism, the cache tag, the gain map, the record

The noise depends only on the output's coordinates and the measured figures: the same chain gives the same bytes. Tag
`gr-<variant>-<precision>-<m|a><amount>v<imaging::grain::VERSION>`: `gr-sensor-fp32-m1v1`. The gain map is base-pixels
only and handed on shared (on `integration`, `run_against` is called in place of `run_with_gain_map`, which a grain
never needs). The export edit record lists it (`Grain Sensor (FP32): amount=1, mode=match`) as
`algorithmicallyEnhanced`: noise drawn by a formula, not a trained model.

### D10. CLI and GUI

- CLI: `--grain [MODEL]` (`--gr`), as every family flag takes an optional model; `--gr-amount <0-100>` (whole percent,
  0 accepted) and `--gr-add`, both `requires = "grain"`, named with the family prefix as `--rs-scale` and `--lb-f` are.
  `--grain [AMOUNT]` was considered and not taken: a family flag's value is a model everywhere else. Refused beside
  `--autopilot` (it is in the `Selection` group, as Restore is); `--gui` lists it among the ignored options, since the
  open request carries Sharpen and Lens Blur by name, not families generically.
- GUI: Grain in `ENHANCEMENTS` (manual, icon `Grip`), last, in the add menu's Effects group with Lens Blur; panel
  `GrainOptions.tsx`: the model tray, an Amount slider (marked at 100) and a Match source switch with a line saying what
  its state does; the row reads `Sensor, 100%, matched`.

### D11. Restore's grain goes; history

`Restore.grain`, `with_grain`, the `-g` tag, `models/restore/grain.rs`, `--rs-grain`, the Restore panel's slider and
its strings are removed; the amount type becomes `GrainAmount` under `models/grain/`. The family is added in four
commits on top of `add-restore-grain` rather than by rewriting its four, so the history shows the first design and why
it moved; the imaging commit keeps `missing` until the opai commit removes its last user, so each commit builds.

## Risks / Trade-offs

- **Noise that is not grain** (JPEG blocking, banding) in the photograph is measured as residual too; a compressed
  source gets a little more grain than it "had". Acceptable for an enhancement a user adds.
- **Texture mistaken for grain**: a tile whose texture the chain removed gets noise back, closer to the photograph than
  a flat tile; a tile where a model drew texture gets none.
- **A tile's median** is a few percent off its true grain either way; where the result has been resampled or redrawn,
  the two medians are of different pictures and only the low side adds, so a result that kept its grain is given a
  few percent more.
- **Cost**: one measurement of the photograph on a stride, one full pass over the result, one synthesis pass.

## Platforms

CPU work; validated on macOS (Apple silicon) only.

## Results

Implemented on bookmark `add-grain-family` (four commits on `add-restore-grain`: imaging, opai, CLI, GUI) and merged
into `integration`. Validated on macOS (M4 Pro, CoreML) only.

### Bench

Release CLI built from the `integration` merge, isolated home (`/Volumes/opai-bench/overnight/grain/home`, cloned from
the restore-grain run's, models linked from the app's directory, its run cache cleared), `-q sd`. Inputs: the two
AIM-500 photographs the restore-grain run gave synthetic sensor noise (read noise 4 levels plus shot noise 0.06 x
sqrt(luma), 90 percent shared, [1 2 1]-smoothed), `o_14a9d723-noisy` (1620 x 1080) and `o_260f47c8-noisy`
(1080 x 1617), and `o_14a9d723` as it is. Figures are the green 3 x 3 residual's median-absolute sigma per luma band,
dark to light, in 8-bit levels, over the whole picture (`measure.py` in the session scratchpad, the same estimator as
`imaging::grain`); outputs, `measured.json`, crop sheets `o_*-grain-crops-0839.png` in
`/Volumes/opai-bench/overnight/grain/`.

| `o_14a9d723-noisy` | b0 | b1 | b2 | b3 | b4 | b5 | b6 | b7 |
|---|---|---|---|---|---|---|---|---|
| source | 6.10 | 7.58 | 7.74 | 7.08 | 6.92 | 7.25 | 7.08 | 7.41 |
| Restore (Kazan 1x) | 2.97 | 7.41 | 8.90 | 4.28 | 2.80 | 2.14 | 1.48 | 0.99 |
| + Grain | 4.61 | 8.24 | 10.38 | 8.24 | 7.41 | 7.58 | 6.92 | 7.25 |
| + Grain 50% | 3.46 | 7.74 | 9.23 | 5.77 | 4.78 | 4.45 | 3.95 | 3.79 |
| Kyoto 2x | 2.31 | 3.62 | 3.62 | 1.81 | 1.32 | 0.99 | 0.66 | 0.66 |
| + Grain | 7.91 | 8.90 | 8.57 | 7.25 | 6.92 | 7.41 | 7.08 | 6.75 |
| Lens Blur (amount 100) | 6.10 | 7.66 | 7.74 | 6.92 | 5.11 | 5.77 | 4.45 | 7.25 |
| + Grain | 6.10 | 7.74 | 7.91 | 7.08 | 7.08 | 7.41 | 7.08 | 7.41 |
| Denoise (Stockholm) | 4.94 | 5.93 | 5.60 | 4.12 | 3.62 | 3.79 | 3.62 | 4.28 |
| + Grain | 4.94 | 5.93 | 5.60 | 4.12 | 3.62 | 3.79 | 3.62 | 4.28 |
| Denoise, Restore, Grain | 4.61 | 6.92 | 8.40 | 4.78 | 3.95 | 3.79 | 3.62 | 4.28 |

| `o_260f47c8-noisy` | b0 | b1 | b2 | b3 | b4 | b5 | b6 | b7 |
|---|---|---|---|---|---|---|---|---|
| source | 1.32 | 6.59 | 7.91 | 8.90 | 10.21 | 10.54 | 9.23 | 8.24 |
| Restore (Kazan 1x) | 0.00 | 11.37 | 14.66 | 15.81 | 18.12 | 17.46 | 9.23 | 4.37 |
| + Grain | 1.15 | 11.70 | 14.99 | 16.31 | 18.94 | 18.12 | 11.53 | 7.08 |
| Kyoto 2x | 0.00 | 1.81 | 2.31 | 3.13 | 4.12 | 5.11 | 3.46 | 1.32 |
| + Grain | 0.99 | 6.92 | 8.57 | 10.05 | 11.86 | 12.52 | 10.21 | 8.24 |
| Lens Blur (amount 100) | 0.49 | 5.93 | 7.25 | 8.24 | 9.55 | 10.05 | 8.73 | 8.07 |
| + Grain | 1.15 | 6.75 | 8.24 | 9.47 | 10.87 | 11.70 | 10.05 | 9.06 |
| Denoise, + Grain (identical) | 0.00 | 3.62 | 4.61 | 5.44 | 7.25 | 8.24 | 7.74 | 7.25 |

Lens blur, per 32 x 32 tile (median over the tiles of each class):

| | tiles | source | lens blur | + grain |
|---|---|---|---|---|
| `o_14a9d723` background (blur < 0.4 x source) | 221 | 6.42 | 0.33 | 6.42 |
| `o_14a9d723` in focus (blur > 0.85 x source) | 1406 | 7.08 | 7.08 | 7.08 |
| `o_260f47c8` background | 1011 | 1.15 | 0.33 | 0.99 |
| `o_260f47c8` in focus | 440 | 3.29 | 3.13 | 3.46 |

- **Lens blur**: the background gets the photograph's own noise back (6.42 of 6.42; 0.99 of 1.15) and the subject
  keeps its own (7.08, 7.08). Over the whole picture the light bands return to the source's within 5 percent.
- **Denoise**: a denoise followed by Grain is the denoise, byte for byte in the measures (both photographs): the
  denoised picture is the photograph matched, and it lacks nothing of itself. Denoise, Restore, Grain puts back only
  what Kazan smoothed below the denoise's level (bands 0 to 3), not the noise the denoise removed.
- **Restore**: where Kazan smoothed (`o_14a9d723` bands 3 to 7, `o_260f47c8` bands 6 and 7) the noise comes back to
  within 16 percent of the source (7 percent or less in five of them); where Kazan drew texture it gets next to nothing.
  Half the amount lands between. The whole-picture band medians sit a little above the source where texture and grain
  mix in one band (band 2 of `o_14a9d723`: 10.38 against 7.74, of which 8.90 is Kazan's own).
- **Kyoto 2x**: per output pixel back to the photograph's per source pixel within 5 percent from band 3 up; the dark
  bands overshoot (`o_14a9d723` b0 +30 percent, b1 +17; `o_260f47c8` b5 +19), where Kyoto's enlargement and the
  photograph are compared at a 2:1 tile scale. Finer at print size than the source's grain, as D5 intends (crop sheet).
- **A clean photograph** (`o_14a9d723` as published): Grain adds nothing (within 0.2 levels in one band); adding at 50%
  gives about 2 to 3.6 levels in the mid and light bands, more in the shadows, as the sensor shape says.
- **Cost**: the grain step's whole CLI run is 0.2 to 0.7 s with every step before it served from the cache (1.75 MP at
  1x, 7 MP at 2x).

### The first build: the result alone per tile

The first build measured only the result tile by tile, against the photograph's band figure. On the same bench (outputs
`out-one-sided/`, `measured-one-sided.json`): Denoise + Grain gave 7 to 36 percent more noise than the denoise
(`o_14a9d723` band 3: 4.12 → 5.60), Grain over the clean photograph raised band 3 from 1.32 to 2.80, and the restore's
mid bands overshot by up to 35 percent. A real photograph's noise is uneven, so every tile below its band's median was
topped up. Measuring the photograph per tile too (D3) removed all three.

### Tests

On the branch and on `integration`: `imaging` 98 and 178, `opai` lib 1754 and 2343, `public_api` 28 and 31, `cli` 195
and 297, `gui` 338 and 426, frontend vitest 1409 and 2259; `cargo fmt --check`, clippy `--workspace --all-targets -D warnings`, tsc and
biome clean on both. The grain-specific ones: the tile measurement (a clean result lacks all of it, the photograph
itself none, grain back on the half that lost it and not the half that kept it, the lack read between tile centres, the
sensor shape); the pipeline (matched within 10 percent at 1x and 15 at 2x, half the amount, the photograph itself given
nothing, a clean photograph, determinism, add mode, no photograph, cancellation); the chain (after a smoothing step, after
a denoise computed and stored, a stored denoise that will not decode run again, the photograph alone given nothing, no
session); on `integration` a limiting mask (grain where the mask let a smoothing through, nothing over the
photograph's half, a masked denoise not undone), the gain map left shared in both modes, and the edit record.

## Open Questions

1. Film grain as a second variant (clumped, stock-specific), and Size / Roughness, if Joseph wants a look rather than
   realism.
2. Dark-band overshoot after an AI enlargement (Kyoto 2x, above): compare at the photograph's tile scale rather than
   the result's?
