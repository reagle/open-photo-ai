## Context

The series' decisions are in `add-lens-blur-1-depth/design.md` (D1 to D17), in particular D9 (guided upsampling and edge
grow), D10 (the layered render) and D11 (the `keep_sharp` seam). The render as built (`imaging/src/defocus.rs` on
`add-lens-blur-2-render`): signed radius per pixel, about 14 evenly spaced layers with one at 0, tent weights between
neighbours, far to near, each layer's premultiplied colour and coverage convolved with an anti-aliased disc (reduced
resolution beyond 12 px), composited "over", normalised at the end; `keep_sharp` scales the radius.

The prototype that settled this design is `opai-notes/lens-blur/matting/` (`render_matte.py --matte`,
`matte_b_vitmatte.py`, `photos.py`), with results on three photographs (a fur hood against a street, dark hair against a
dark case, leaves against a lattice) in its README. Its findings that this slice relies on:

- Spreading the subject over the depth layers by its own per-pixel depth, premultiplied by alpha, fails: at long focal
  lengths the subject's depth noise spreads it over many layers whose blurred coverage leaves gaps, through which the
  filled background shows as grey contour bands over the face. **One layer for the subject** fixes it.
- The background must be filled behind the subject (colour and disparity), and the disparity also over a ring around it
  (0.7 percent of the long side) where the depth map bled the subject's depth outward; without the ring those pixels
  render as a crisp background halo.
- The subject's colour must have the background removed (blur fusion); otherwise its edge pixels carry the background's
  colour onto the composite.
- With 14 evenly spaced layers over -401..+154 px (a 181 mm-equivalent lens at f/1.4), nothing within 40 px of focus
  renders at its own radius. Radius-proportional layers (x1.4 from 1 px) fix that.

## Goals / Non-Goals

**Goals:** the primitives and the render path for a matted subject, exact enough that slice 8 only has to supply an
alpha.

**Non-Goals:** the matting model, the trimap, choosing when to matte (slice 8); changing what today's lens blur renders
(the default spacing and the absent subject keep slice 2's output bit for bit); blurring the subject by its own depth
variation (it is one layer, sharp or one radius: a "focus range").

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-lens-blur-1-depth` | Depth family, Lisbon, `DepthMap`, cached data run | - | `main@upstream` | built |
| 2 | `add-lens-blur-2-render` | `imaging` primitives and the layered defocus render | - | `main@upstream` | built |
| 3 | `add-lens-blur-3-operation` | Lens EXIF, Lens Blur operation, injection, catalogue, CLI | 1, 2 | 1 + 2 | built |
| 4 | `add-lens-blur-4-gui` | Panel, f-number slider, click to focus, `lens_info`, i18n | 3 | 3 | built |
| 5 | `add-lens-blur-5-hdr-gainmap` | Gain map blurred and rewritten | 3, colour stack | `add-color-4-ultra-hdr-gainmap` + 1..4 | built |
| 6 | `add-lens-blur-6-live-preview` | Display-size render while dragging | 4 | 4 | pending |
| 7 | `add-lens-blur-7-matte-render` | Subject composite in `defocus`, push-pull fill, blur fusion, morphology, geometric layers | 2 (4 for the stack) | 4 | **this change** |
| 8 | `add-lens-blur-8-matting` | Matting family (Porto = ViTMatte-S, fork-only graph), depth trimap, matte injected into the lens blur, `--lb-edges` | 7 | 7 | proposed |
| 9 | `add-lens-blur-9-matte-gainmap` | The matte composite applied to an Ultra HDR gain map | 5, 8 | 5 + 7..8 | pending, not proposed |

## How the work divides

`imaging` only: four new modules and one changed one, none of which knows what a lens, a depth or a model is. `opai`
passes `None` and `Even` until slice 8.

## Decisions

### M1. The subject is one premultiplied layer, composited at its radius

```rust
pub struct Subject<'a> {
    /// The subject's own colour, linear, three values a pixel, at the photograph's size (background removed: M3).
    pub colour: &'a [f32],
    /// Its coverage in [0, 1], at any size (read bilinearly, like `keep_sharp`).
    pub alpha: &'a Plane,
    /// The one signed radius it is blurred by; under 0.5 px it is sharp.
    pub radius: f32,
}

pub fn defocus(linear: &[f32], width: u32, height: u32, radius: &Plane, keep_sharp: Option<&Plane>,
               subject: Option<Subject<'_>>, settings: Defocus, cancelled: &dyn Fn() -> bool) -> Option<Vec<f32>>
```

With a subject, the caller passes `linear` and `radius` already filled behind it (M2): the scene as if the subject were
not there. The far-to-near loop is unchanged but for one step: before the first layer nearer than `subject.radius` (in
the loop's own order), `acc = (F alpha, alpha) + acc (1 - alpha)`, with the subject's planes convolved by the disc of
its radius first when that is 0.5 px or more. Nearer layers then cover it as they cover anything. Memory: one more
full-frame four-plane layer while the subject is composited.

`keep_sharp` keeps its meaning and applies to the background's radius (a user's mask in masking slice 3 still holds
areas sharp).

### M2. Push-pull fill

`fill::filled(values: &[f32], channels: usize, width, height, weight: &Plane) -> Vec<f32>`: a pyramid of weighted means
(each level the 2x2 box reduction of `w v` and `w`, until 1 px or all weights positive), then back up, each level's
pixels taking `(w v + (1 - w) up) / ...` from the coarser level where their own weight is low: exact where `weight` is
1, a smooth fill where it is 0, a blend between. O(N) and seamless (no tile boundaries, no diffusion iterations). The
lens blur (slice 8) fills linear colour where `alpha > 0.004` and disparity there and in the ring of M5.

### M3. Foreground colour by blur fusion

`fusion::foreground(image: &[f32], alpha: &Plane, width, height, radii: &[u32]) -> Vec<f32>`: Forte and Pitié,
"Approximate fast foreground colour estimation" (ICIP 2021), the two-pass form: per pass, box-filtered `F` and `B`
estimates weighted by `alpha` and `1 - alpha`, then `F = F_blur + alpha (I - alpha F_blur - (1 - alpha) B_blur)`,
clamped to [0, 1] in linear light. Passes at 90 then 6 px on a 12 to 24 MP photograph in the prototype; the radii are
the caller's (slice 8 scales them with the diagonal). Box filters by running sums (`boxes`), rows in parallel.

### M4. Grey morphology

`morph::eroded(plane, radius)` and `morph::dilated(plane, radius)`: separable square min and max filters by the van
Herk/Gil-Werman running method (three comparisons a pixel whatever the radius). A square rather than a disc: the
trimap's band edge does not need to be round, and the prototype worked at quarter size where the difference is a pixel.
`guided::dilated_nearer` stays as it is (its disc and "nearer wins" semantics are its own).

### M5. What slice 8 will feed it (recorded here so the primitives fit)

Trimap from the depth band around focus (distance from focus / 1.1 to focus x 1.5; sure foreground the band eroded by
0.7 percent of the long side, sure background its outside dilated by the same, unknown between, at quarter size);
ViTMatte-S alpha on the unknown band; background fill of colour over `alpha > 0.004` and of disparity over that plus a
0.7 percent ring; subject radius from the median disparity of the confident interior (`alpha > 0.9`, eroded), not the
edge. Slice 8's design owns these numbers.

### M6. Geometric layer spacing

`Defocus::spacing: Spacing` with `Even` (today's: about `layers` radii spread evenly over the range, one at 0) and
`Geometric { first: f32, ratio: f32 }` (radii 0, then `first`, `first ratio`, `first ratio^2`, ... on each side up to
the range's ends; `layers` is then ignored). `Defocus::DEFAULT` keeps `Even`, so slice 2's renders and tests are
unchanged; slice 8 chooses `Geometric { first: 1.0, ratio: 1.4 }` for the lens blur after a visual check. At -401..+154
px that is about 18 + 15 layers instead of 14: render time grows roughly with the count of large-radius layers, which
are reduced-resolution and cheap; measured in task 2.4.

### M7. Shared box filters

`guided.rs` has private `box_mean` and `reduced`; fusion needs the first and fill the second. Per the project's tier
rule they move to `imaging::boxes` (`box_mean`, `reduced`) in this slice's first commit, before the new callers are
written, with their tests; `guided` calls them from there.

## Risks / Trade-offs

- **Ordering the subject in the loop**: a subject whose radius sits between two layers is composited between them; with
  tent weights a background pixel at the same radius is split across both. Tested with a subject at radius 0 and at a
  nonzero radius between layers.
- **Fill at the frame edge**: the pyramid's coarsest level is one pixel, so a subject touching the frame edge still gets
  a fill; tested.
- **Blur fusion's limit**: a bright background behind dense dark strands leaves light specks (prototype, Tokyo); a known
  limit of the method.
- **Performance**: fusion and fill add well under a second at 24 MP in Rust (box filters by running sums); measured in
  the timing test.

## Platforms

CPU only, platform-independent. Validated on macOS (Apple silicon); Windows and Linux untested.

## Open Questions

1. **Square or disc morphology** (default: square, M4).
2. **Geometric ratio** (default: 1.4 from 1 px; slice 8 decides after its visual check).
