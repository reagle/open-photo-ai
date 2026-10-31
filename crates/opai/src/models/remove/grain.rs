//! The photograph's grain, matched in a fill: measured in the photograph around the hole and in the fill inside it, and
//! the difference added back to the fill as noise.
//!
//! A fill read back from the model's square is smoother than the photograph wherever the crop was reduced to reach it:
//! the reduction averages the grain away and the read-back cannot invent it again, so a large region comes back as a
//! clean patch in a grainy photograph, which the eye finds at once. What is matched is the high-frequency residual —
//! each pixel less the mean of its 3 x 3 neighbourhood — per channel, and how much the three channels' residuals move
//! together (a camera's grain is mostly in luminance, a JPEG's chroma is coarser still).

// A robust estimate, the median absolute residual, rather than a standard deviation: around a removed object there are
// edges, and an edge's residual is large and rare, which a median ignores and a mean squares. The same estimator on both
// sides, so the difference is grain and not the estimator.
//
// The noise is white, at the level that gives the fill the photograph's residual: a camera's grain after demosaicing and
// compression is a little coarser than one pixel, which white noise of the matched residual slightly understates in
// amplitude and overstates in fineness; at the one-to-four-pixel scale a viewer compares, the two read alike. It is
// drawn from a hash of the photograph's own coordinates, so a removal is the same image every time it is run, region by
// region, whatever else the mask holds.

use super::region::{GROW, in_hole};

/// The median absolute deviation of a normal distribution over its standard deviation's reciprocal: what turns the
/// median absolute residual into the standard deviation of a normal residual.
const MAD_TO_SIGMA: f32 = 1.4826;

/// The standard deviation of the residual of unit white noise: the pixel less the mean of nine, `sqrt(8 / 9)`.
const WHITE_RESIDUAL: f32 = 0.942_809;

/// The reduction a crop must have been brought to the model's square by before its fill is given the photograph's
/// grain: under it the model saw the photograph at about its own resolution and drew the grain itself, and what the
/// ring's measure would add is its texture again, as noise (on the bench, small holes came out a quarter grainier than
/// the photograph with it).
pub(crate) const FROM_REDUCTION: f32 = 1.25;

/// The fewest samples either side is measured from before anything is added: a hole in a crop too small to have a
/// ring of known pixels around it is left as the model filled it.
const FEWEST: usize = 64;

/// The most samples either side is measured from: a ring of millions of pixels is read on a stride.
const MOST: usize = 1 << 18;

/// The residual below which a photograph has no grain, only the rounding of a smooth gradient to 8-bit levels (whose
/// own is about a third of a level): three quarters of one.
const QUANTISATION: f32 = 0.75 / 255.0;

/// How far around the hole's box the photograph's grain is measured, in pixels, beyond [`RING_FRACTION`] of its
/// longer side.
const RING_PIXELS: usize = 32;

/// How far around the hole's box the photograph's grain is measured, as a fraction of its longer side.
const RING_FRACTION: f32 = 0.25;

/// The grain of one side, in the unit range: each channel's residual, and how much the channels' residuals move
/// together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Grain {
    /// Each channel's residual, as the standard deviation of a normal one.
    pub(crate) sigma: [f32; 3],
    /// The mean correlation between two channels' residuals, in `[0, 1]`.
    pub(crate) correlation: f32,
}

/// A crop of the photograph with its fill in place, in the unit range, one pixel per element in rows.
pub(crate) struct Composite<'a> {
    pub(crate) pixels: &'a mut [[f32; 3]],
    pub(crate) width: usize,
    pub(crate) height: usize,
}

impl Composite<'_> {
    /// The residual at `(x, y)`, an inner pixel: the pixel less the mean of its 3 x 3 neighbourhood.
    fn residual(&self, x: usize, y: usize) -> [f32; 3] {
        let mut sum = [0.0_f32; 3];
        for row in y - 1..=y + 1 {
            for pixel in &self.pixels[row * self.width + x - 1..][..3] {
                sum.iter_mut().zip(pixel).for_each(|(sum, value)| *sum += value);
            }
        }

        let centre = self.pixels[y * self.width + x];
        std::array::from_fn(|channel| centre[channel] - sum[channel] / 9.0)
    }

    /// The grain measured at the inner pixels whose whole 3 x 3 neighbourhood `inside` admits, among `x0..x1` by
    /// `y0..y1`, on a stride that keeps the count under [`MOST`]; `None` from fewer than [`FEWEST`].
    fn grain(
        &self,
        (x0, x1): (usize, usize),
        (y0, y1): (usize, usize),
        inside: impl Fn(usize) -> bool,
    ) -> Option<Grain> {
        let (x0, y0, x1, y1) = (x0.max(1), y0.max(1), x1.min(self.width - 1), y1.min(self.height - 1));
        if x0 >= x1 || y0 >= y1 {
            return None;
        }

        let stride = ((x1 - x0) * (y1 - y0)).div_ceil(MOST).max(1);
        let mut samples: Vec<[f32; 3]> = Vec::new();

        for (index, (x, y)) in (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).enumerate() {
            if index % stride != 0 {
                continue;
            }

            let whole = (y - 1..=y + 1).all(|row| (x - 1..=x + 1).all(|column| inside(row * self.width + column)));
            if whole {
                samples.push(self.residual(x, y));
            }
        }

        measured(&samples)
    }
}

/// The grain of `samples`, residuals: each channel's from its median absolute value, and the channels' correlation over
/// the samples within three of those of zero, so an edge's large residuals do not stand for the grain's.
fn measured(samples: &[[f32; 3]]) -> Option<Grain> {
    if samples.len() < FEWEST {
        return None;
    }

    let sigma: [f32; 3] = std::array::from_fn(|channel| {
        let mut magnitudes: Vec<f32> = samples.iter().map(|sample| sample[channel].abs()).collect();
        let middle = magnitudes.len() / 2;
        let (_, median, _) = magnitudes.select_nth_unstable_by(middle, f32::total_cmp);

        *median * MAD_TO_SIGMA
    });

    // The products and squares of the residuals within the bound, per pair of channels.
    let bounded = samples
        .iter()
        .filter(|sample| sample.iter().zip(&sigma).all(|(value, sigma)| value.abs() <= 3.0 * sigma));
    let (mut products, mut squares) = ([0.0_f64; 3], [0.0_f64; 3]);
    for sample in bounded {
        for (pair, (a, b)) in [(0, 1), (1, 2), (0, 2)].into_iter().enumerate() {
            products[pair] += f64::from(sample[a] * sample[b]);
        }
        squares.iter_mut().zip(sample).for_each(|(square, value)| *square += f64::from(value * value));
    }

    let correlations = [(0, 1), (1, 2), (0, 2)].into_iter().enumerate().map(|(pair, (a, b))| {
        let scale = (squares[a] * squares[b]).sqrt();
        if scale > 0.0 { products[pair] / scale } else { 0.0 }
    });
    let correlation = (correlations.sum::<f64>() / 3.0).clamp(0.0, 1.0) as f32;

    Some(Grain { sigma, correlation })
}

/// The grain of the photograph around the hole and of the fill inside it, of `composite`, whose distances from the
/// marks are `squared`: the photograph's from the known pixels within a ring of the hole's box, the fill's from the
/// pixels the fill replaces outright. `None` where either has too few pixels to say.
pub(crate) fn grains(composite: &Composite<'_>, squared: &[u16]) -> Option<(Grain, Grain)> {
    let (width, height) = (composite.width, composite.height);

    // The hole's box, from the distances.
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    for (index, squared) in squared.iter().enumerate() {
        if in_hole(*squared) {
            let (x, y) = (index % width, index / width);
            (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x + 1), bottom.max(y + 1));
        }
    }
    if left >= right {
        return None;
    }

    let reach = RING_PIXELS + (RING_FRACTION * (right - left).max(bottom - top) as f32).round() as usize;
    let ring = (left.saturating_sub(reach), (right + reach).min(width));
    let rows = (top.saturating_sub(reach), (bottom + reach).min(height));

    let photograph = composite.grain(ring, rows, |index| !in_hole(squared[index]))?;
    let fill = composite.grain((left, right), (top, bottom), |index| u32::from(squared[index]) <= GROW * GROW)?;

    Some((photograph, fill))
}

/// Adds to the fill of `composite` (the pixels in the hole of `squared`) the noise that gives it `photograph`'s grain
/// where its own is `fill`'s. `origin` is the crop's top-left corner in the photograph, which the noise is drawn from.
pub(crate) fn matched(
    composite: &mut Composite<'_>,
    squared: &[u16],
    origin: (u32, u32),
    photograph: Grain,
    fill: Grain,
) {
    // The noise to add, per channel, as a standard deviation: what the residual lacks, over the residual one unit of
    // white noise has.
    let added: [f32; 3] = std::array::from_fn(|channel| {
        let lacking = photograph.sigma[channel].powi(2) - fill.sigma[channel].powi(2);
        lacking.max(0.0).sqrt() / WHITE_RESIDUAL
    });

    // A photograph whose residual is under [`QUANTISATION`] has no grain to match, only the rounding to its levels; and
    // less than a twentieth of an 8-bit level to add is nothing a viewer could see.
    if photograph.sigma.iter().all(|sigma| *sigma < QUANTISATION) || added.iter().all(|sigma| *sigma < 0.05 / 255.0) {
        return;
    }

    // One shared draw and one per channel, mixed so two channels' noise correlates as the photograph's residuals do.
    let shared = photograph.correlation.sqrt();
    let own = (1.0 - photograph.correlation).sqrt();
    let width = composite.width;

    for (index, (pixel, squared)) in composite.pixels.iter_mut().zip(squared).enumerate() {
        if !in_hole(*squared) {
            continue;
        }

        let (x, y) = (origin.0 as usize + index % width, origin.1 as usize + index / width);
        let [common, red, green, blue] = normals(x as u64, y as u64);

        for ((value, sigma), draw) in pixel.iter_mut().zip(added).zip([red, green, blue]) {
            *value += sigma * shared.mul_add(common, own * draw);
        }
    }
}

/// The salt the fill's noise is drawn under, so it never draws the field another caller of the hash does.
const SALT: u64 = 0x5851_f42d_4c95_7f2d;

/// Four standard normal draws for the photograph's pixel `(x, y)`, from a hash of its coordinates: the same for the
/// same pixel on every run. The shared hash, `imaging::grain::normals`, under this fill's own salt.
fn normals(x: u64, y: u64) -> [f32; 4] {
    imaging::grain::normals(x, y, SALT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::remove::region::HOLE;

    /// A `width` x `height` composite: flat grey at `level`, plus `photograph` noise of the given standard deviation
    /// outside a disc of radius `radius` at the centre and none inside it, and the disc's distances from its rim.
    fn disc(width: usize, height: usize, radius: f32, photograph: f32) -> (Vec<[f32; 3]>, Vec<u16>) {
        let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
        let mut pixels = Vec::with_capacity(width * height);
        let mut squared = Vec::with_capacity(width * height);

        for y in 0..height {
            for x in 0..width {
                let distance = ((x as f32 - cx).hypot(y as f32 - cy) - radius).max(0.0);
                squared.push((distance * distance).min(((HOLE + 1) * (HOLE + 1)) as f32) as u16);

                let noise = if in_hole(*squared.last().unwrap()) {
                    [0.0; 4]
                } else {
                    normals(x as u64 + 7_000, y as u64).map(|draw| draw * photograph)
                };
                pixels.push([0.5 + noise[0], 0.5 + noise[0], 0.5 + noise[0]]);
            }
        }

        (pixels, squared)
    }

    #[test]
    fn a_hash_draws_normals_with_the_moments_of_a_normal_and_the_same_ones_every_time() {
        let draws: Vec<f32> = (0..200_u64).flat_map(|y| (0..200_u64).flat_map(move |x| normals(x, y))).collect();
        let mean = draws.iter().sum::<f32>() / draws.len() as f32;
        let variance = draws.iter().map(|draw| (draw - mean).powi(2)).sum::<f32>() / draws.len() as f32;

        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((variance - 1.0).abs() < 0.02, "variance {variance}");
        assert_eq!(normals(12, 34), normals(12, 34));
        assert_ne!(normals(12, 34), normals(34, 12));
    }

    #[test]
    fn a_smooth_fill_in_grainy_surroundings_is_given_the_surroundings_grain() {
        let (width, height, sigma) = (240, 200, 0.03);
        let (mut pixels, squared) = disc(width, height, 40.0, sigma);
        let mut composite = Composite { pixels: &mut pixels, width, height };

        let (photograph, fill) = grains(&composite, &squared).expect("both sides measured");
        // Monochrome noise: the three channels move together exactly, and the residual of white noise is 0.94 of it.
        assert!((photograph.sigma[1] - sigma * WHITE_RESIDUAL).abs() < 0.003, "{photograph:?}");
        assert!(photograph.correlation > 0.99, "{photograph:?}");
        assert!(fill.sigma[1] < 1e-6, "{fill:?}");

        matched(&mut composite, &squared, (0, 0), photograph, fill);
        let (again, _) = grains(&composite, &squared).expect("measured again");
        assert!((again.sigma[1] - photograph.sigma[1]).abs() < 0.003, "{again:?} against {photograph:?}");

        // And nothing outside the hole moved.
        let (untouched, _) = disc(width, height, 40.0, sigma);
        for ((after, before), squared) in pixels.iter().zip(&untouched).zip(&squared) {
            if !in_hole(*squared) {
                assert_eq!(after, before);
            }
        }
    }

    #[test]
    fn a_fill_already_as_grainy_as_its_surroundings_is_left_as_it_is() {
        let (width, height) = (200, 200);
        let (mut pixels, squared) = disc(width, height, 30.0, 0.02);
        // The fill's own grain, of the same level.
        for (index, pixel) in pixels.iter_mut().enumerate() {
            if in_hole(squared[index]) {
                let draw = normals(index as u64, 99)[0] * 0.02;
                *pixel = [0.5 + draw; 3];
            }
        }
        let before = pixels.clone();
        let mut composite = Composite { pixels: &mut pixels, width, height };

        let (photograph, fill) = grains(&composite, &squared).expect("both sides measured");
        matched(&mut composite, &squared, (0, 0), photograph, fill);

        // At most the little the two estimates differ by: a fraction of the grain, against the 0.02 it already has.
        let changes: Vec<f32> = pixels.iter().zip(&before).map(|(a, b)| a[0] - b[0]).collect();
        let moved = (changes.iter().map(|change| change * change).sum::<f32>() / changes.len() as f32).sqrt();
        assert!(moved < 0.005, "a fill as grainy as its surroundings was given noise of {moved}");
    }

    #[test]
    fn a_hole_with_no_ring_of_photograph_around_it_says_nothing() {
        // The hole covers the whole crop.
        let mut pixels = vec![[0.5; 3]; 30 * 30];
        let composite = Composite { pixels: &mut pixels, width: 30, height: 30 };

        assert_eq!(grains(&composite, &[0; 30 * 30]), None);
    }

    #[test]
    fn an_edge_in_the_ring_does_not_count_as_grain() {
        // A clean step from dark to light across the ring: no grain at all, however large the step's residual.
        let (width, height) = (200, 160);
        let (_, squared) = disc(width, height, 20.0, 0.0);
        let mut pixels: Vec<[f32; 3]> =
            (0..width * height).map(|i| [if i % width < 60 { 0.1 } else { 0.9 }; 3]).collect();
        let composite = Composite { pixels: &mut pixels, width, height };

        let (photograph, _) = grains(&composite, &squared).expect("measured");
        assert!(photograph.sigma.iter().all(|sigma| *sigma < 1e-6), "{photograph:?}");
    }
}
