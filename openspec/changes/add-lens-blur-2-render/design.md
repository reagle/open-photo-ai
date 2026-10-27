## Context

The series' decisions are in `add-lens-blur-1-depth/design.md`; this slice implements D9 (guided upsampling, edge grow),
D10 (the render) and D11 (the masking seam) as `imaging` primitives. `imaging` today: tiling, tensors, padding, tile
blending, `mix::blended`, square bilinear sampling (`bilinear.rs`, asserts a square plane), `present`, and
`rows::for_each_row` (band-per-worker `std::thread::scope`, at least 64 rows a worker, bit-identical to serial).

## Slices

| # | Change | Delivers | Depends on | Base | State |
|---|---|---|---|---|---|
| 1 | `add-lens-blur-1-depth` | Depth family, Lisbon, `DepthMap`, cached data run | - | `main@upstream` | built |
| 2 | `add-lens-blur-2-render` | `imaging` primitives and the layered defocus render | - (parallel to 1) | `main@upstream` | **this change** |
| 3 | `add-lens-blur-3-operation` | Lens EXIF, Lens Blur operation, injection, catalogue, CLI | 1, 2 | 1 + 2 | built |
| 4 | `add-lens-blur-4-gui` | Panel, f-number slider, click to focus, `lens_info`, i18n | 3 | 3 | built |
| 5 | `add-lens-blur-5-hdr-gainmap` | Gain map blurred and rewritten | 3, colour stack | `add-color-4-ultra-hdr-gainmap` + 1..4 | built |
| 6 | `add-lens-blur-6-live-preview` | Display-size render while dragging | 4 | 4 | pending |

## How the work divides

`imaging` only. Nothing here knows about lenses, depth models or EXIF: the render takes radii in pixels; slice 3
computes them.

## Decisions

### R1. API

```rust
// plane.rs
pub struct Plane { width: u32, height: u32, data: Vec<f32> }
impl Plane {
    pub fn new(width: u32, height: u32, fill: f32) -> Self;
    pub fn from_vec(width: u32, height: u32, data: Vec<f32>) -> Result<Self, PlaneSize>;
    pub fn width(&self) -> u32; pub fn height(&self) -> u32;
    pub fn get(&self, x: u32, y: u32) -> f32;
    pub fn as_slice(&self) -> &[f32];
    pub fn as_mut_slice(&mut self) -> &mut [f32];
    pub fn resized(&self, width: u32, height: u32) -> Plane; // bilinear, pixel centres aligned
}
// linear.rs
pub fn srgb_to_linear(v: f32) -> f32; pub fn linear_to_srgb(v: f32) -> f32;
pub fn decoded_linear(image: &DynamicImage) -> (Vec<f32> /* RGB interleaved */, ChannelDepth);
pub fn encoded(linear: &[f32], width: u32, height: u32, depth: ChannelDepth) -> DynamicImage; // clamps to 0..1
// guided.rs
// guide: the photograph's sRGB values in 0..1, RGB interleaved
pub fn upsampled(low: &Plane, guide: &[f32], width: u32, height: u32, radius: u32, eps: f32) -> Plane;
pub fn dilated_nearer(plane: &Plane, radius: f32) -> Plane;
// defocus.rs
pub struct Defocus { pub layers: usize, pub max_direct_radius: f32 } // 14, 12.0
pub fn defocus(linear: &[f32], width: u32, height: u32, radius: &Plane, keep_sharp: Option<&Plane>,
    settings: Defocus, cancelled: &dyn Fn() -> bool) -> Option<Vec<f32>>; // None when cancelled
```

The radius plane must match the image's size (slice 3 makes it so; a mismatch is a panic); `keep_sharp` may be any size and is resampled. Non-finite radii and samples are read as 0 (see As built). Errors are `thiserror` types in the module that raises them, as `imaging` does elsewhere (`PlaneSize`).

### R2. Guided upsampling

Colour guided filter, coefficients solved at `sub = 4`: **the guide is box-reduced to `ceil(w/4) x ceil(h/4)` by exact block means** (the blocks on the right and bottom edges as wide as what is left), and the low plane is **box-reduced by the integer factor that brings it near that size** and then bilinearly resized to it (a bilinear read alone would alias a plane much larger than the target's quarter scale), window radius `max(1, round(radius / 4))`, box means by running sums (O(1) per pixel) whose **window is clipped at the plane's edges and divided by the number of pixels it holds**, He's convention, rather than reflected (a reflected window counts the pixels beside an edge twice), so "a flat guide returns the bilinear upsample" is tested away from the edges. The 3x3 covariance plus `eps * I` is inverted per pixel in closed form; the second pass of the filter box-means the coefficients; `a` (3 planes) and `b` are bilinearly upsampled, read on an axis of four times the small extent so that a photograph whose side is not a multiple of four does not slide the coefficients up to three pixels off the edges they were solved against, and applied as `q = a . I + b` at full resolution, clamped to the low plane's range. The same algorithm as the prototype (`depth_da2.py::guided_upsample`); the tests check properties rather than the prototype's output, so nothing generated from the private test photographs enters the repository: a step edge in the guide carries a step in the output within 2 px; a flat guide returns the bilinear upsample; a constant plane stays constant.

### R3. The disc convolution

For a radius `R` (`0.5 <= R <= max_direct_radius`), row offsets `dy` in `-ceil(R)..=ceil(R)`, the kernel is the brute-force anti-aliased disc **exactly**, `clamp(R + 0.5 - hypot(dx, dy), 0, 1)`: each row is a run of whole pixels (weight 1) plus **each fractional rim pixel as its own tap, at its own per-pixel weight**, rather than one fractional end per row, which would square a small disc's top and bottom off by up to half a pixel (several percent at `R <= 12`) at about 40% fewer reads. The whole-pixel runs are summed by **a running `f64` window along each source row** (edges clamped, coverage handled by convolving the coverage plane identically), not from a stored prefix-sum plane, so no 32 byte per pixel prefix plane is held and a long row's sums do not lose precision in `f32`. The kernel's total weight is computed once and divided out. Larger radii: box-reduce the layer's box by `k = ceil(R / max_direct_radius)`, convolve at `R / k`, bilinearly enlarge back. Matches a brute-force disc convolution with the same kernel within 2 percent of the peak on random images up to `R = 12`.

### R4. Layering, compositing, the seam

As `add-lens-blur-1-depth/design.md` - D10 and D11. Layer radii: `n` (default 14) evenly spaced over `[min(s), max(s)]` with one at exactly 0, **at least 1 px apart and the range widened to include 0** (the prototype makes 10,000 layers for a constant radius); tent weights. **A radius under 0.5 px is a dead zone: the whole weight is on the layer at 0, so a pixel that close to focus is returned exactly as it went in** (the spec's "returned unchanged"); the interval next to 0 starts at 0.5, so the weight still grows continuously with the radius. A layer whose weights are all below 1e-4 is skipped; each layer is processed only over its bounding box padded by its radius. **Edges are replicated (clamped)**, so a blur near the frame's edge neither darkens nor lets farther layers show through. `keep_sharp`: radius multiplied by `1 - clamp(w, 0, 1)` before layering, the weight resampled to the radius plane's size. `cancelled` is checked before each layer. Parallel rows via `for_each_row` in the convolution and the compositing; the result is bit-identical between one and many workers (the `rows` contract).

### R5. Performance target and measurement

An ignored test (`--release`, `-- --ignored defocus_timing --nocapture`) renders a synthetic 6000 x 4000 image with radii from -60 to +20 px along a diagonal (pessimistic: every layer's bounding box is wide and tall) and a 1600 x 1200 one, and prints the times, with the decode to linear light and the encode. Targets: 24 MP under 3 s, 2 MP under 0.2 s on an M-series CPU. If 24 MP is over 5 s, lower `max_direct_radius` to 8 before anything else, and record the trade (softer disc edges near focus) here.

**Measured** (M4 Pro, 12 cores, release; the machine was loaded, load average 11 to 15 from other agents and Spotlight, so the spread is wide): 6000 x 4000 in **0.61 to 1.01 s** (target 3 s; decode 0.012 s, encode 0.04 s), 1600 x 1200 in **0.07 to 0.22 s** (target 0.2 s; decode 0.001 s, encode 0.004 to 0.016 s). The 2 MP case uses the same pixel radii as the 24 MP one, a blur 3.5 times larger relative to the frame than a real 2 MP output would get, and is over 0.2 s only on the loaded runs. On the photographs: the bonsai at 4815 x 3852 with MoGe ViT-B depth at f/1.4 equivalent takes 0.09 s to guided-upsample, 0.05 s to grow the edges and 0.5 to 1.1 s to render; the ramp renders in 0.4 to 0.55 s. A layer costs its bounding box; the heaviest is the last direct one (12 px), about 0.26 s for a 19 MP box under load. The fallback was not needed.

## Risks / Trade-offs

- Memory: about 4 x 4 bytes x pixels for the accumulator, as much for a layer, plus the linear image: about 1.2 GB at 24
  MP. Bands are the fallback, not built now.
- Reduced-resolution layers soften a disc's rim by about `k` pixels, at most 1/12 of the radius: a slightly soft-edged
  bokeh, as many real lenses give.
- A highlight whose radius falls between two layers is the tent mix of two discs: on over-range points of light (600 times the range, on black) it shows as a faint second rim up to one layer spacing (3.6 px at the radii tested) outside the main disc. It is not visible on the photographs, whose highlights are clipped at 1.0 and so are dim discs; slice 5's gain map could make it visible on HDR highlights. 20 layers instead of 14 narrow it to about 2.5 px at roughly 40% more render time, and the default stays 14.
- Between focus and the first layer (0 to 3.6 px at the radii tested) a pixel is a cross-fade of sharp and blurred rather than a true 2 px blur: inherent to the tent layering, and not visible on the photographs.

## As built

What the implementation settled differently from the text above, and what the checks showed. Timings are R5's; the visual check is task 3.2's.

- **Input guards.** `defocus` panics where the image is not three values a pixel, the radius plane is not the image's size, or `max_direct_radius` is under 1 or not a number: these are the caller's bugs. A radius that is not finite is read as 0 (no blur; NaN is not an error), one beyond the photograph's diagonal as the diagonal (a disc that size already covers the photograph from any pixel), a linear value that is not finite as 0 (so one bad sample cannot poison the running sums along its row), and a keep-sharp plane with no area as no plane. `guided::upsampled` panics on a guide of the wrong length, returns an empty plane for an empty photograph, and panics where the low plane has no area and the photograph has some.
- **`decoded_linear` composites alpha against black**, as `tensor::Sampler` does for every pipeline, which is how "alpha dropped" is read. A private `Workers` switch (serial against `for_each_row`) exists only for the one-worker and many-workers bit-identity test. `bilinear::Tap`'s fields are `pub(crate)` so `Plane::resized` can use the existing axis arithmetic.
- **Visual check** (a throwaway harness outside the repository, nothing derived committed): a horizontal radius ramp from -40 to +10 px on the three test photographs shows no layer seams, the handover from direct to reduced layers at 12 px is not visible in 100% crops, and image edges neither darken nor smear. The bonsai end to end with MoGe-2's real depth (guided upsample, edge grow 2.6 px, the thin lens at 36 mm f/1.4 against f/6 equivalent, focus at 1.89 m, radii -30.8 to +8.0 px) keeps leaf edges sharp against the blurred lattice with no coloured fringe or halo and the background blooms into soft discs, comparable to the prototype's. A synthetic chart of noise and points of light shows a single layer's disc as the anti-aliased disc with no rings.
## Platforms

Pure Rust, no platform code; tested on macOS. Windows and Linux run the same unit tests in CI.
