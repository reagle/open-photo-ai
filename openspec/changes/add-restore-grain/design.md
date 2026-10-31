## Context

Restore (`crates/opai/src/models/restore/`, bookmark `add-restore-family`) runs Kazan (PiSA-SR, one 512 tile per graph
run through the shared `filter`, wrapped in `resample::resampled`) or Osaka (SeedVR2, its own driver), then blends the
result towards the image the model was handed by `strength`. The result is clean: the model draws no sensor noise, so a
restored photograph is smoother than any capture of the same scene. Kazan costs 9 to 13 s per 1.3 MP at 1x on the fork's
M4 Pro; a grain pass is CPU work an order of magnitude cheaper.

The fork's Remove line already matches grain in a fill (`models/remove/grain.rs`, commit `xsqvqlrs` "give the fill the
photograph's grain", on the `remove-hd` workspace's line, not yet on a bookmark): a 3 x 3-mean residual per channel, a
median-absolute estimate, the inter-channel correlation, and white noise drawn from a SplitMix64 hash of the
photograph's coordinates. This change uses the same estimator and hash, made signal-dependent and shared.

Research: `~/dev/opai-notes/research/restore-grain-2026-10-31.md` (what other apps do, the noise models).

## Goals / Non-Goals

**Goals:** a restored photograph that has the noise its source had, in kind and in amount, per brightness; off by
default; the same pixels on every run; nothing else changed.

**Non-Goals:** film-grain emulation (clumped, density-dependent, stock-specific grain as DxO FilmPack and Capture One
offer); grain for other families or as a finishing enhancement of its own (D9, later); a manual absolute amount (open
question 1); grain in the gain map.

## Decisions

### D1. A parameter of Restore, not a family

`Restore { variant, scale, strength, grain }`. The grain answers a property of what Restore makes, it is measured
against Restore's own input, and it must sit between Restore and the lens blur that may follow it (`Family::APPLY_ORDER`
ends Restore, Lens Blur): a later "Grain" family placed after Lens Blur would grain the blurred background with the
photograph's grain, which a lens does not do. Topaz offers both patterns (Gigapixel: under the model's settings; Photo
AI: a finishing enhancement); the per-model one is what fits a chain with a lens blur after it.

`grain` is a bounded amount (`opai::Grain`, 0 to 1, quantised as `Strength` is, `#[serde(default)]` so a stored or
sent Restore without it is grain off). `Restore::new` keeps its signature and sets 0; `Restore::with_grain(Grain)` sets
it. The catalogue row publishes `grain` (`Range { min: 0, max: 1, default: 0 }`), and `ParameterValues::with_grain` and
the by-name setter carry it.

### D2. The amount is a share of what the restoration removed ("match source")

`grain = 1` puts back all of the noise the restoration took away; 0.5 puts back half of its standard deviation, so
the slider reads linearly to the eye. A clean source gets nothing at any amount, a grainy one gets its own
grain back: the realistic result, and the one no fixed amount gives.

Why not a manual absolute amount (Lightroom, Luminar, Capture One): it is the right control for a look, not for
realism, and it would be wrong by default on every photograph whose noise differs from the default. Open question 1
keeps the door open for an "Add" mode beside "Match".

### D3. Measured per luminance band, on the input and the output

For the photograph Restore was given (`input`, before any resample) and the restoration's result (`output`, after the
strength blend), each at its own size:

1. The residual of each inner pixel, per channel: the pixel less the mean of its 3 x 3 neighbourhood.
2. The pixel's band: its Rec. 709 luma (of the 3 x 3 mean) in eight equal bands over [0, 1].
3. Per band and channel, sigma = 1.4826 x median |residual| over at most 2^18 samples on a stride (edges are rare and
   large; a median ignores them), and the mean inter-channel correlation over samples within 3 sigma.
4. A band with fewer than 64 samples takes the nearest measured band's figure; a picture with none measured adds
   nothing.

The missing deviation per band and channel is `sqrt(max(0, sigma_in^2 - sigma_out^2))`, under a floor: an input band
whose sigma is under 0.75 / 255 (the rounding of a smooth 8-bit gradient) has no grain to put back. Each pixel's
deviation is interpolated linearly between band centres by its own luma: a piecewise-linear scaling function, the shape
AV1 signals its film grain with, fitted here rather than signalled. This captures what a Poisson-Gaussian sensor does
(variance growing with signal, then shrinking near white as the tone curve compresses highlights) without assuming the
curve: sRGB output is not raw, so the model is measured, not parametrised.

The input is measured before the resample, not after: a Lanczos enlargement softens the noise it enlarges, and would
understate the source's per-pixel noise at 2x and 4x (D5).

### D4. Sensor-like noise, at the output's pixels

Per output pixel: four standard normals from a SplitMix64 hash of `(x, y)` and a salt of its own (the Remove fill's
hash, salted, so the two never draw the same field). One is shared by the three channels with weight `sqrt(c)` and one
per channel with `sqrt(1 - c)`, `c` the source's measured correlation: luminance-dominated noise with a little chroma,
as a demosaiced, compressed capture has. The field is smoothed with a separable [1 2 1] / 4 kernel and renormalised to
unit variance (a fixed factor, measured once in a test): a correlation of about one and a half pixels, what demosaicing
gives, where white noise reads as finer than any camera's. The residual of the smoothed field is measured once (a test
constant) so the amount added gives exactly the measured residual back.

Not film grain: film's is clumped, larger than a pixel, and depends on density and stock; the research note lists the
tools that emulate it. A later "type" control could add it.

### D5. Scale: the grain is at the output's pixel size

An enlarging restore resamples first, so its output pixels are smaller than the source's. The grain is drawn per output
pixel at the deviation the **source** had per pixel (measured on the source before the resample, D3 run on the
unresampled input), less what the output has: what a sensor with the output's resolution and the source's per-pixel
noise would record. At 2x it therefore looks finer at print size than the source's, never coarser, and never like an
enlarged noise field (which would show 2 x 2 blocks). The wrapper sits outside `resample::resampled`, so it sees both the
unresampled input and the final output.

### D6. Strength

Applied after the strength blend: at strength 0.5 the output already holds part of the input's noise, so less is
missing and less is added; at strength 0 nothing is missing and nothing is added.

### D7. Determinism and the cache

The noise depends only on the output's coordinates and the measured figures, which depend only on the pixels: the same
run gives the same bytes, a cached step is the same as a recomputed one, and preview and export agree when they run at
the same size. The cache tag gains `-g<grain>` when grain is above 0 (`rs-kazan-fp16-s1-t1-g0.5`), and a constant
`GRAIN_VERSION` (`-g0.5v1`) so a later change of the synthesis never serves an older result. Grain off keeps today's tag,
so every stored restore stays valid.

### D8. The gain map is untouched

Restore does not rewrite the gain map (`rewrites_gain_map` stays false), and the grain is added to the base pixels only.
An HDR display then raises the grain with its highlight as it raises the highlight: what a capture's own noise does
under the same map.

### D9. A shared module, used by Restore now and by Remove when the lines meet

`imaging::grain`: `Profile` (eight bands x three channels of sigma, the correlation), `measured(&DynamicImage) ->
Option<Profile>` (D3), `missing(source, result) -> Profile`, `added(&mut DynamicImage, &Profile, salt)` (D4, generic over
8 and 16 bits). `opai::models::restore::grain` is the `Grained` wrapper only. Remove's `models/remove/grain.rs` measures
over a crop and a hole rather than a whole picture; when the Remove and Restore lines are merged, it can be rewritten on
`imaging::grain` (its masked sampling is one more argument) - noted for that merge, not done here, since the Remove line
is in flight in another workspace.

A grain for other families (Denoise at full strength leaves the same plastic look, and Sharpen and Upscale at times)
reuses `Grained` around their pipelines; not in this change.

### D10. CLI and GUI

- `--rs-grain <0-100>`, a whole percent (as `--rs-str`), `requires = "restore"`, 0 accepted (off). Documented with what
  100 means.
- GUI: `IntensityControl` "Grain" under Strength in `RestoreOptions.tsx`, 0 to 100 percent from the published range,
  default 0, with a one-line hint: "Puts back the photograph's own noise that the restoration smoothed away." Keys
  `enhancements.restore.grain`, `enhancements.restore.grainHint` in the 14 locales.
- Autopilot never sets it (Restore is outside Autopilot).

## Risks / Trade-offs

- **Noise in the source that is not grain** (JPEG blocking, banding) is measured as residual too. The 3 x 3 residual
  sees mostly the finest scale, and blocking is coarser; a heavily compressed source gets a little more grain than it
  "had". Acceptable for an opt-in control.
- **Texture mistaken for noise**: a finely textured band (foliage) has a large residual both before and after, so the
  difference stays small; a texture the model removed is put back as noise, which is closer to the source than a flat
  area.
- **Cost**: two measurements on a stride and one pass over the output: well under a second at 24 MP, measured in the
  acceptance run.

## Platforms

CPU work; validated on macOS (Apple silicon) only.

## Results

Implemented on bookmark `add-restore-grain` (four commits on `add-restore-family`: imaging, opai, CLI, GUI). Validated on macOS (M4 Pro, CoreML) only.

### Deviations from the decisions

- **D3, band centres.** Each band's figure is placed at the mean luma of the pixels it was measured from (`Profile::centre`), not at the band's middle; the per-pixel deviation is interpolated between those measured centres. With the middles, a photograph that only reaches into its end bands (highlights to 0.92, say) had its end figure placed 0.04 too far out, and the matched residual overshot by 10 to 20 percent there; with the measured centres the opai tests match within 10 percent at 1x and 15 percent at 2x. A sparse band (fewer than 64 samples) keeps the middle.
- **D3, edges.** A band made only of edge pixels (a clean step between two flat levels puts the step's 3 x 3 means in the bands between them) measures the edge as grain. Where a band also holds flat or textured pixels, as it does in a photograph, the median ignores the edges; the test is written that way (`an_edge_among_its_bands_flat_pixels_is_not_grain`).
- **D4.** The smoothed field's constants are exact rather than measured: the [1 2 1] / 4 kernel on both axes multiplies a unit field's deviation by 3/8, and the smoothed unit field's 3 x 3 residual is sqrt(19) / 9 = 0.4843 (white noise: sqrt(8 / 9)). A test recomputes both from the kernel.
- **D8.** `add-restore-family` has no gain-map plumbing in the pipeline contract (`rewrites_gain_map` is not on this base); `Grained` only ever returns the base image its restorer returned, so there is nothing for it to touch. The Ultra HDR scenario is to be checked on `integration`.
- **D9.** `imaging::grain` exposes `normals(x, y, salt)`, the Remove fill's hash with the salt as an argument (Remove's is `0x5851_f42d_4c95_7f2d`, Restore's `0x7265_7374_6f72_6521`); Remove is not moved onto it here.
- **Logging.** `Grained` writes one `debug` record per run (`opai::models::restore::grain`, "grain put back"): the source's, the restoration's and the added residual per band in 8-bit levels (green), the correlation and the pass's duration. The figures below are read from it.
- **Serialisation.** A `Restore` with grain off serialises without the field, so its JSON is the same as before grain existed.

### Visual check

Release CLI from the workspace, isolated home (`HOME=/Volumes/opai-bench/overnight/restore-grain/home`, models and runtime linked from the app's directory), Kazan FP16 (`-q sd`) on CoreML, PNG outputs in `/Volumes/opai-bench/overnight/restore-grain/`. Residuals are the 3 x 3 residual's median-absolute sigma per luma band (dark to light), green channel, in 8-bit levels, measured over the whole picture (the `noise.py` script, the same estimator as `imaging::grain` without the stride).

**The AIM-500 photographs themselves have no grain for Kazan to remove.** On `o_260f47c8` (portrait, 1080 x 1617) and `o_14a9d723` (portrait, 1620 x 1080), Kazan's output has *more* fine residual than the source in every measured band (it draws hair and skin texture): source `0.0 4.3 5.1 5.3 6.9 6.9 4.4 2.3`, Kazan 1x `0.0 12.2 17.0 15.3 20.1 17.6 10.9 4.0`. So nothing is added: grain 100 is byte-identical to grain 0 at 1x (`cmp`), and at 2x it adds 0.9 levels in the top band only. This is the "clean source gets nothing" scenario on real photographs, and also the first finding: on downsampled, compressed photographs the measured residual is texture, and a generative restorer adds texture.

To test the matching, sensor-like noise was added to both photographs (read noise 4 levels plus shot noise 0.06 x sqrt(luma), 90 percent shared between channels, [1 2 1]-smoothed; `*-noisy.png`):

| `o_14a9d723-noisy` | b0 | b1 | b2 | b3 | b4 | b5 | b6 | b7 |
|---|---|---|---|---|---|---|---|---|
| source | 6.10 | 7.58 | 7.74 | 7.08 | 6.92 | 7.25 | 7.08 | 7.41 |
| Kazan 1x, grain 0 | 2.97 | 7.41 | 8.90 | 4.28 | 2.80 | 2.14 | 1.48 | 0.99 |
| Kazan 1x, grain 50 | 4.45 | 7.41 | 8.73 | 5.93 | 5.11 | 4.78 | 3.95 | 3.79 |
| Kazan 1x, grain 100 | 6.75 | 7.41 | 8.40 | 8.57 | 8.24 | 8.24 | 7.25 | 7.41 |
| Kazan 2x, grain 100 | 7.91 | 8.57 | 9.06 | 8.90 | 8.40 | 8.24 | 7.25 | 7.25 |

| `o_260f47c8-noisy` | b0 | b1 | b2 | b3 | b4 | b5 | b6 | b7 |
|---|---|---|---|---|---|---|---|---|
| source | 1.32 | 6.59 | 7.91 | 8.90 | 10.21 | 10.54 | 9.23 | 8.24 |
| Kazan 1x, grain 0 | 0.00 | 11.37 | 14.66 | 15.81 | 18.12 | 17.46 | 9.23 | 4.37 |
| Kazan 1x, grain 50 | 0.82 | 11.37 | 14.66 | 15.81 | 18.12 | 17.46 | 9.23 | 6.10 |
| Kazan 1x, grain 100 | 1.32 | 11.37 | 14.66 | 15.81 | 18.12 | 17.30 | 9.55 | 8.57 |
| Kazan 2x, grain 100 | 1.32 | 8.07 | 10.54 | 11.04 | 12.36 | 10.54 | 9.72 | 8.40 |

Where Kazan smoothed the noise away (the light bands of both, the dark band of `o_260f47c8`), grain 100 brings the residual back to the source's: within 4 percent in bands 6 and 7 of both and band 0 of `o_260f47c8`, 11 percent over in band 0 of `o_14a9d723`; grain 50 lands between (about half the missing deviation, in quadrature). Where Kazan drew more texture than the noise it removed (the mid bands of `o_260f47c8`, bands 1 and 2 of `o_14a9d723`), nothing is added, as designed. The overshoot of up to 21 percent in the mid bands of `o_14a9d723` at 1x is where the restoration's own residual is texture rather than Gaussian: medians of a heavy-tailed residual and an added Gaussian do not add in quadrature, so the missing deviation computed from them is a little too large. A crop (`o_14a9d723-noisy-compare-2315.png`: noisy source, grain 0, 50, 100) shows the grain back on the hair, fine and without blocks.

The per-band figure is a whole-picture median, so it cannot see that one region of a band (a cheek) lost its noise while another (hair of the same brightness) gained texture: the band's residual can stay at or above the source's and nothing is put back on the cheek. A local (tiled) measurement would; noted as an open question.

Timings (wall clock of the whole CLI run; Kazan's first CoreML compile in this home was 200 s and is excluded): `o_14a9d723-noisy` 1x 12.0 s at grain 0, 11.8 s at 50, 11.7 s at 100, 2x at 100 35.6 s; `o_260f47c8-noisy` 11.8 / 11.9 / 11.9 s, 2x 36.5 s. The grain pass itself (both measurements and the synthesis, from the debug record): 17 ms at 1.75 MP (1x), 36 to 37 ms at 7 MP (2x).

### Checks

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` clean; `TZ=UTC cargo test -p imaging -p opai --release -j 6`: imaging 94 passed (1 ignored), opai lib 1740 passed (38 ignored), `public_api` 27 and the other integration suites passed; `cargo test -p cli` 194 passed; `cargo test -p gui` 338 passed (2 ignored); frontend `tsc --noEmit`, `biome check` and `vitest run` (1406 tests, 108 files) clean.

## Open Questions

1. An "Add" mode (an absolute amount on a clean photograph) beside "Match"?
2. Grain on by default at some amount (Topaz Gigapixel ships it off; Krea's default is a faint 0.02)? Default: off.
3. Offer it for Denoise and Upscale too (D9)?
4. Measure locally (per tile) rather than per whole-picture band, so a region that lost its noise gets it back even where another region of the same brightness gained texture? See Results.
