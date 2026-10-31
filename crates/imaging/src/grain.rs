//! A photograph's grain, measured per brightness, and noise synthesised to put back what a result lacks of it.
//!
//! A generative restorer draws clean detail: the photograph's own sensor noise or film grain does not survive it, and a
//! restored sky or cheek reads as smoother than any capture of the same scene. What is measured here is the fine
//! residual — each pixel less the mean of its 3 x 3 neighbourhood — per channel, in eight bands of luminance, and how
//! much the three channels' residuals move together. Measured on the image a run was given and on what it made, the
//! difference per band is the grain the run took away; [`added`] draws noise of that deviation, interpolated by each
//! pixel's own luminance, and adds it to the result.
//!
//! **Per brightness**, because a sensor's noise is signal-dependent: its variance grows with the light it counts and
//! then shrinks again near white, where the tone curve compresses the highlights. sRGB is not raw, so the curve is
//! measured rather than modelled — eight bands, the deviation interpolated linearly between their centres, which is the
//! piecewise-linear scaling function AV1 signals its film grain with, fitted here rather than signalled.
//!
//! **Deterministic**: the noise is drawn from a hash of the result's own pixel coordinates and a salt, so a run gives
//! the same pixels every time and a cached result is the same as a recomputed one. The hash is the one Remove's fill
//! matching draws from, and the salt keeps two callers from drawing the same field.

// The estimator is a median absolute residual rather than a standard deviation: a photograph has edges, and an edge's
// residual is large and rare, which a median ignores and a mean squares. The same estimator on both sides, so the
// difference is grain and not the estimator. The correlation is taken over the samples within three of their band's
// deviations of zero, for the same reason.
//
// The noise is not white. A camera's grain after demosaicing and compression is a little coarser than one pixel, so the
// field is smoothed by a separable [1 2 1] / 4 kernel (a correlation of about one and a half pixels) and renormalised to
// unit variance; its residual is then a known fraction of its deviation, [`SMOOTHED_RESIDUAL`], which is what turns
// the residual to put back into the deviation to draw.
//
// Not film grain: film's is clumped, larger than a pixel and depends on the density and the stock. Not done here.

use std::sync::LazyLock;

use image::DynamicImage;

use crate::tensor::{Channel, Sampler};

/// How many bands of luminance the grain is measured in, equal over `[0, 1]`.
pub const BANDS: usize = 8;

/// The version of the synthesis, for a cache key: a result drawn by a later version is a different image.
pub const VERSION: u32 = 1;

/// The median absolute deviation of a normal distribution over its standard deviation's reciprocal: what turns the
/// median absolute residual into the standard deviation of a normal residual.
const MAD_TO_SIGMA: f32 = 1.4826;

/// The fewest samples a band is measured from; a band with fewer takes the nearest measured band's figures.
const FEWEST: usize = 64;

/// The most samples a picture is measured from: a large one is read on a stride.
const MOST: usize = 1 << 18;

/// The residual below which a band has no grain, only the rounding of a smooth gradient to 8-bit levels (whose own is
/// about a third of a level): three quarters of one.
const QUANTISATION: f32 = 0.75 / 255.0;

/// The deviation below which there is nothing to add: a twentieth of an 8-bit level, which no viewer could see.
const INVISIBLE: f32 = 0.05 / 255.0;

/// What the separable [1 2 1] / 4 kernel multiplies a unit white field's deviation by: `(6 / 16)` per axis, `3 / 8`.
/// The smoothed field is divided by it, so the noise drawn has unit variance.
const SMOOTHED_DEVIATION: f32 = 0.375;

/// The residual of the smoothed unit field: `sqrt(19) / 9`, the 3 x 3 residual operator applied to the kernel, over the
/// kernel's own norm. A white field's would be `sqrt(8 / 9)`.
pub const SMOOTHED_RESIDUAL: f32 = 0.484_322_1;

/// The Rec. 709 luma weights, which put a pixel in its band.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// A picture's grain, in the unit range: each band's and channel's residual, and how much the channels' residuals move
/// together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// Per band of luminance, from dark to light, each channel's residual as the standard deviation of a normal one.
    pub sigma: [[f32; 3]; BANDS],
    /// Per band, the mean luminance of the pixels it was measured from: where in the band its figure belongs, which a
    /// picture that only reaches into a band (a sky's highlights, a shadow's last few levels) sets well away from the
    /// band's middle. A band with too few samples to say has its middle.
    pub centre: [f32; BANDS],
    /// The mean correlation between two channels' residuals, in `[0, 1]`.
    pub correlation: f32,
}

impl Profile {
    /// This profile with every deviation multiplied by `share`: what a grain amount puts back of what was missing.
    #[must_use]
    pub fn scaled(self, share: f32) -> Self {
        Self { sigma: self.sigma.map(|band| band.map(|sigma| sigma * share)), ..self }
    }

    /// Whether a viewer could see any of it.
    pub fn is_invisible(&self) -> bool {
        self.sigma.iter().flatten().all(|sigma| *sigma < INVISIBLE)
    }

    /// The deviation for a pixel of `luma`: linear between the bands' centres, and the end band's beyond them.
    fn at(&self, luma: f32) -> [f32; 3] {
        // The centres rise from band to band, each inside its own, so the pair around `luma` is the first centre above
        // it and the one before.
        let above = self.centre.iter().position(|centre| *centre > luma);
        let (below, above) = match above {
            Some(0) => return self.sigma[0],
            None => return self.sigma[BANDS - 1],
            Some(above) => (above - 1, above),
        };
        let fraction = (luma - self.centre[below]) / (self.centre[above] - self.centre[below]);

        std::array::from_fn(|channel| {
            let (low, high) = (self.sigma[below][channel], self.sigma[above][channel]);
            fraction.mul_add(high - low, low)
        })
    }
}

/// The middle of each band, where a band measured from no pixels of its own puts its figure.
fn middles() -> [f32; BANDS] {
    std::array::from_fn(|index| (index as f32 + 0.5) / BANDS as f32)
}

/// The band a luminance falls in.
fn band(luma: f32) -> usize {
    ((luma * BANDS as f32) as usize).min(BANDS - 1)
}

/// The grain of `image`, from the residuals of its inner pixels on a stride that keeps the count under [`MOST`];
/// `None` where no band has [`FEWEST`] samples.
pub fn measured(image: &DynamicImage) -> Option<Profile> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    if width < 3 || height < 3 {
        return None;
    }

    // A stride on both axes rather than on the pixel index, so it cannot fall in step with the width and read one
    // column of the picture.
    let inner = (width - 2) * (height - 2);
    let stride = ((inner as f64 / MOST as f64).sqrt().ceil() as usize).max(1);
    let sampler = Sampler::new(image);
    let unit = |x: usize, y: usize| sampler.rgb(x as u32, y as u32).map(|value| f32::from(value) / 65535.0);

    let mut bands: [Vec<[f32; 3]>; BANDS] = Default::default();
    let mut lumas = [0.0_f64; BANDS];
    let mut row = [[[0.0_f32; 3]; 3]; 3];
    for y in (1..height - 1).step_by(stride) {
        for x in (1..width - 1).step_by(stride) {
            for (dy, line) in row.iter_mut().enumerate() {
                for (dx, pixel) in line.iter_mut().enumerate() {
                    *pixel = unit(x + dx - 1, y + dy - 1);
                }
            }

            let mean: [f32; 3] = std::array::from_fn(|c| row.iter().flatten().map(|pixel| pixel[c]).sum::<f32>() / 9.0);
            let luma = LUMA.iter().zip(mean).map(|(weight, value)| weight * value).sum::<f32>();
            bands[band(luma)].push(std::array::from_fn(|c| row[1][1][c] - mean[c]));
            lumas[band(luma)] += f64::from(luma);
        }
    }

    let mut profile = profile(&bands)?;
    for ((centre, sum), samples) in profile.centre.iter_mut().zip(lumas).zip(&bands) {
        if samples.len() >= FEWEST {
            *centre = (sum / samples.len() as f64) as f32;
        }
    }

    Some(profile)
}

/// The profile of residuals sorted into bands: each measured band's deviations from its median, the sparse ones from
/// their nearest measured neighbour, and one correlation over all of them.
fn profile(bands: &[Vec<[f32; 3]>; BANDS]) -> Option<Profile> {
    let measured: [Option<[f32; 3]>; BANDS] = std::array::from_fn(|index| {
        let samples = &bands[index];
        (samples.len() >= FEWEST).then(|| {
            std::array::from_fn(|channel| {
                let mut magnitudes: Vec<f32> = samples.iter().map(|sample| sample[channel].abs()).collect();
                let middle = magnitudes.len() / 2;
                let (_, median, _) = magnitudes.select_nth_unstable_by(middle, f32::total_cmp);

                *median * MAD_TO_SIGMA
            })
        })
    });

    if measured.iter().all(Option::is_none) {
        return None;
    }

    // The products and squares of the residuals within three deviations of zero, per pair of channels.
    let (mut products, mut squares) = ([0.0_f64; 3], [0.0_f64; 3]);
    for (samples, sigma) in bands.iter().zip(&measured) {
        let Some(sigma) = sigma else { continue };
        let bounded = samples
            .iter()
            .filter(|sample| sample.iter().zip(sigma).all(|(value, sigma)| value.abs() <= 3.0 * sigma));

        for sample in bounded {
            for (pair, (a, b)) in [(0, 1), (1, 2), (0, 2)].into_iter().enumerate() {
                products[pair] += f64::from(sample[a] * sample[b]);
            }
            squares.iter_mut().zip(sample).for_each(|(square, value)| *square += f64::from(value * value));
        }
    }

    let correlations = [(0, 1), (1, 2), (0, 2)].into_iter().enumerate().map(|(pair, (a, b))| {
        let scale = (squares[a] * squares[b]).sqrt();
        if scale > 0.0 { products[pair] / scale } else { 0.0 }
    });
    let correlation = (correlations.sum::<f64>() / 3.0).clamp(0.0, 1.0) as f32;

    // A sparse band takes the nearest measured one's figures, the darker of two at the same distance.
    let sigma = std::array::from_fn(|index| {
        (0..BANDS)
            .filter_map(|other| measured[other].map(|sigma| (index.abs_diff(other), sigma)))
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, sigma)| sigma)
            .expect("at least one band was measured")
    });

    Some(Profile { sigma, centre: middles(), correlation })
}

/// What `result` lacks of `source`'s grain, per band and channel: `sqrt(max(0, source^2 - result^2))`, and nothing in
/// a band whose source residual is under the rounding of a smooth 8-bit gradient. The correlation is the source's.
pub fn missing(source: &Profile, result: &Profile) -> Profile {
    let sigma = std::array::from_fn(|index| {
        std::array::from_fn(|channel| {
            let (had, has) = (source.sigma[index][channel], result.sigma[index][channel]);
            if had < QUANTISATION { 0.0 } else { (had * had - has * has).max(0.0).sqrt() }
        })
    });

    // The source's centres: the deviation belongs where the source's pixels of that band are.
    Profile { sigma, centre: source.centre, correlation: source.correlation }
}

/// Adds to `image`, in place, noise whose residual is `profile`'s at each pixel's luminance, drawn from the hash of the
/// pixel's coordinates under `salt`.
///
/// Written at the image's own depth for the RGB and RGBA variants at 8 and 16 bits, which are what every pipeline hands
/// back; alpha is left as it is, and any other variant is left untouched. A profile no viewer could see adds nothing.
pub fn added(image: &mut DynamicImage, profile: &Profile, salt: u64) {
    if profile.is_invisible() {
        return;
    }

    let (width, height) = (image.width() as usize, image.height() as usize);
    match image {
        DynamicImage::ImageRgb8(buffer) => add_to::<u8>(buffer, width, height, 3, profile, salt),
        DynamicImage::ImageRgba8(buffer) => add_to::<u8>(buffer, width, height, 4, profile, salt),
        DynamicImage::ImageRgb16(buffer) => add_to::<u16>(buffer, width, height, 3, profile, salt),
        DynamicImage::ImageRgba16(buffer) => add_to::<u16>(buffer, width, height, 4, profile, salt),
        _ => {}
    }
}

/// [`added`] over one picture's storage, `channels` values a pixel, in bands of rows across the available cores.
fn add_to<T: Channel + Send>(raw: &mut [T], width: usize, height: usize, channels: usize, profile: &Profile, salt: u64)
where
    image::Rgb<T>: image::Pixel<Subpixel = T>,
{
    // Below this there is not enough work to pay for a thread.
    const MIN_ROWS_PER_WORKER: usize = 64;
    static AVAILABLE: LazyLock<usize> =
        LazyLock::new(|| std::thread::available_parallelism().map_or(1, std::num::NonZero::get));

    let workers = (*AVAILABLE).min(height.div_ceil(MIN_ROWS_PER_WORKER)).max(1);
    let rows_per_worker = height.div_ceil(workers).max(1);
    let row_len = width * channels;

    // Each band computes the field from the coordinates alone, its rows' neighbours included, so the split cannot
    // change a value: the same pixels whatever the number of cores.
    std::thread::scope(|scope| {
        for (index, band) in raw.chunks_mut(rows_per_worker * row_len).enumerate() {
            let first = index * rows_per_worker;
            scope.spawn(move || {
                let mut field = Field::new(width, first, salt);
                for (offset, row) in band.chunks_mut(row_len).enumerate() {
                    let noise = field.row(first + offset);
                    let shared = profile.correlation.sqrt();
                    let own = (1.0 - profile.correlation).sqrt();

                    for (pixel, draws) in row.chunks_mut(channels).zip(noise) {
                        let values: [f32; 3] = std::array::from_fn(|c| pixel[c].to_unit());
                        let luma = LUMA.iter().zip(values).map(|(weight, value)| weight * value).sum::<f32>();
                        let deviation = profile.at(luma);

                        for channel in 0..3 {
                            let draw = shared.mul_add(draws[0], own * draws[channel + 1]);
                            let amount = deviation[channel] / SMOOTHED_RESIDUAL * draw;
                            pixel[channel] = T::from_unit(values[channel] + amount);
                        }
                    }
                }
            });
        }
    });
}

/// The smoothed unit noise field, row by row: four draws per pixel (one shared, one per channel), each through the
/// [1 2 1] / 4 kernel on both axes and renormalised.
struct Field {
    width: usize,
    salt: u64,
    /// The horizontally smoothed draws of the rows above, at and below the next one asked for, by `y + 1`.
    rows: [Vec<[f32; 4]>; 3],
    /// The row `rows[1]` holds.
    centre: usize,
    smoothed: Vec<[f32; 4]>,
}

impl Field {
    /// The field over rows from `first`, `width` pixels wide.
    fn new(width: usize, first: usize, salt: u64) -> Self {
        let rows = [first, first + 1, first + 2].map(|y| horizontal(width, y, salt));
        Self { width, salt, rows, centre: first, smoothed: vec![[0.0; 4]; width] }
    }

    /// The field's row `y`, which is the row after the last one asked for (or the first).
    fn row(&mut self, y: usize) -> &[[f32; 4]] {
        while self.centre < y {
            self.rows.rotate_left(1);
            self.rows[2] = horizontal(self.width, self.centre + 3, self.salt);
            self.centre += 1;
        }

        let [above, at, below] = &self.rows;
        for (((out, a), b), c) in self.smoothed.iter_mut().zip(above).zip(at).zip(below) {
            *out = std::array::from_fn(|k| (a[k] + 2.0 * b[k] + c[k]) * 0.25 / SMOOTHED_DEVIATION);
        }

        &self.smoothed
    }
}

/// The draws of field row `y - 1` (the field is offset by one on both axes, so the pixel before the first is drawn
/// too), smoothed along the row.
fn horizontal(width: usize, y: usize, salt: u64) -> Vec<[f32; 4]> {
    let raw: Vec<[f32; 4]> = (0..width + 2).map(|x| normals(x as u64, y as u64, salt)).collect();
    raw.windows(3)
        .map(|w| std::array::from_fn(|k| (w[0][k] + 2.0 * w[1][k] + w[2][k]) * 0.25))
        .collect()
}

/// Four standard normal draws for the pixel `(x, y)` under `salt`, from a hash of its coordinates: the same for the
/// same pixel and salt on every run.
///
/// SplitMix64 over the packed coordinates, two Box-Muller pairs from its four 32-bit halves: Remove's fill hash, whose
/// own salt is `0x5851_f42d_4c95_7f2d`.
pub fn normals(x: u64, y: u64, salt: u64) -> [f32; 4] {
    let mut state = ((x << 32) | (y & 0xffff_ffff)) ^ salt;
    let mut next = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };

    let (first, second) = (next(), next());
    let unit = |bits: u64| ((bits >> 40) as f32 + 0.5) / (1u64 << 24) as f32;
    let pair = |bits: u64| {
        let (u, v) = (unit(bits), unit(bits << 32));
        let radius = (-2.0 * u.ln()).sqrt();
        let angle = std::f32::consts::TAU * v;

        [radius * angle.cos(), radius * angle.sin()]
    };

    let ([a, b], [c, d]) = (pair(first), pair(second));
    [a, b, c, d]
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgb};

    use super::*;

    const SALT: u64 = 0x1234_5678_9abc_def0;

    /// A horizontal ramp from `low` to `high`, `width` x `height`, at 16 bits so no rounding adds a residual of its
    /// own.
    fn ramp_between(width: u32, height: u32, low: f64, high: f64) -> DynamicImage {
        DynamicImage::ImageRgb16(ImageBuffer::from_fn(width, height, |x, _| {
            let level = low + (high - low) * f64::from(x) / f64::from(width - 1);
            Rgb([(level * 65535.0).round() as u16; 3])
        }))
    }

    /// A horizontal ramp from black to white.
    fn ramp(width: u32, height: u32) -> DynamicImage {
        ramp_between(width, height, 0.0, 1.0)
    }

    /// The deviation a test profile gives a band: signal-dependent, rising to the middle and falling towards white.
    fn shaped(index: usize) -> f32 {
        let centre = (index as f32 + 0.5) / BANDS as f32;
        0.004 + 0.03 * centre * (1.0 - centre)
    }

    fn target() -> Profile {
        Profile { sigma: std::array::from_fn(|index| [shaped(index); 3]), centre: middles(), correlation: 0.8 }
    }

    #[test]
    fn the_smoothed_fields_constants_are_its_own() {
        // The deviation of the [1 2 1] / 4 kernel on both axes, and its residual: computed from the kernel, so the two
        // literals cannot drift from what the field does.
        let axis = [0.25_f64, 0.5, 0.25];
        let mut kernel = [[0.0_f64; 7]; 7];
        for (i, a) in axis.iter().enumerate() {
            for (j, b) in axis.iter().enumerate() {
                kernel[i + 2][j + 2] = a * b;
            }
        }
        let at = |i: isize, j: isize| {
            if (0..7).contains(&i) && (0..7).contains(&j) { kernel[i as usize][j as usize] } else { 0.0 }
        };
        let norm = kernel.iter().flatten().map(|k| k * k).sum::<f64>();
        let mut residual = 0.0;
        for i in 0..7_isize {
            for j in 0..7_isize {
                let mean = (-1..=1)
                    .flat_map(|di| (-1..=1).map(move |dj| (di, dj)))
                    .map(|(di, dj)| at(i + di, j + dj))
                    .sum::<f64>()
                    / 9.0;
                residual += (at(i, j) - mean).powi(2);
            }
        }

        assert!((norm.sqrt() - f64::from(SMOOTHED_DEVIATION)).abs() < 1e-7);
        assert!(((residual / norm).sqrt() - f64::from(SMOOTHED_RESIDUAL)).abs() < 1e-6);
    }

    #[test]
    fn a_hash_draws_normals_with_the_moments_of_a_normal_and_the_same_ones_every_time() {
        let draws: Vec<f32> = (0..200_u64).flat_map(|y| (0..200_u64).flat_map(move |x| normals(x, y, SALT))).collect();
        let mean = draws.iter().sum::<f32>() / draws.len() as f32;
        let variance = draws.iter().map(|draw| (draw - mean).powi(2)).sum::<f32>() / draws.len() as f32;

        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((variance - 1.0).abs() < 0.02, "variance {variance}");
        assert_eq!(normals(12, 34, SALT), normals(12, 34, SALT));
        assert_ne!(normals(12, 34, SALT), normals(34, 12, SALT));
        assert_ne!(normals(12, 34, SALT), normals(12, 34, SALT ^ 1), "a salt did not change the field");
    }

    #[test]
    fn a_clean_picture_measures_no_grain_and_has_none_put_back() {
        let clean = ramp(512, 256);
        let profile = measured(&clean).expect("a ramp has samples in every band");

        assert!(profile.sigma.iter().flatten().all(|sigma| *sigma < 1e-4), "{profile:?}");
        let lacking = missing(&profile, &profile);
        assert!(lacking.is_invisible());

        let mut after = clean.clone();
        added(&mut after, &lacking, SALT);
        assert_eq!(after, clean);
    }

    #[test]
    fn a_band_under_the_rounding_of_eight_bit_levels_has_no_grain_to_put_back() {
        let faint = Profile { sigma: [[0.5 / 255.0; 3]; BANDS], centre: middles(), correlation: 1.0 };
        let clean = Profile { sigma: [[0.0; 3]; BANDS], centre: middles(), correlation: 0.0 };

        assert!(missing(&faint, &clean).sigma.iter().flatten().all(|sigma| *sigma == 0.0));
    }

    #[test]
    fn a_picture_too_small_to_measure_says_nothing() {
        assert_eq!(measured(&ramp(2, 2)), None);
        assert_eq!(measured(&ramp(8, 8)), None, "a 6 x 6 interior is under the fewest samples");
    }

    #[test]
    fn added_noise_reproduces_a_signal_dependent_target_and_measures_back_to_it() {
        // `added` over a clean ramp, measured again: the profile it was asked for, band by band, within 5 percent. The
        // ramp is tall enough that every band holds tens of thousands of samples, and stops short of black and white so
        // no noise is clipped.
        for depth in ["Rgb16", "Rgb8"] {
            let mut picture = ramp_between(1024, 768, 0.05, 0.95);
            if depth == "Rgb8" {
                picture = DynamicImage::ImageRgb8(picture.to_rgb8());
            }

            added(&mut picture, &target(), SALT);
            let profile = measured(&picture).expect("measured");

            for (index, band) in profile.sigma.iter().enumerate() {
                for sigma in band {
                    // What was drawn over the band's width of the ramp: the target interpolated between the bands'
                    // centres, which a band's measurement averages, in variance, over its pixels.
                    let (low, high) =
                        ((index as f32 / BANDS as f32).max(0.05), ((index + 1) as f32 / BANDS as f32).min(0.95));
                    let drawn = (0..100)
                        .map(|step| target().at(low + (high - low) * (step as f32 + 0.5) / 100.0)[1].powi(2))
                        .sum::<f32>()
                        / 100.0;
                    // Eight bits adds its own rounding residual: about a third of a level.
                    let rounding = if depth == "Rgb8" { 0.29 / 255.0 } else { 0.0 };
                    let expected = (drawn + rounding * rounding).sqrt();
                    assert!(
                        (sigma - expected).abs() / expected < 0.05,
                        "{depth} band {index}: {sigma} against {expected}"
                    );
                }
            }
            assert!((profile.correlation - 0.8).abs() < 0.05, "{depth}: correlation {}", profile.correlation);
        }
    }

    #[test]
    fn the_same_picture_and_profile_draw_the_same_pixels() {
        let mut one = ramp(300, 200);
        let mut two = one.clone();
        added(&mut one, &target(), SALT);
        added(&mut two, &target(), SALT);

        assert_eq!(one, two);
    }

    #[test]
    fn a_tall_picture_split_across_workers_draws_what_one_worker_would() {
        // The field depends on the coordinates alone; compare a picture against a crop of a taller one.
        let mut tall = ramp(64, 400);
        added(&mut tall, &target(), SALT);
        let mut short = ramp(64, 50);
        added(&mut short, &target(), SALT);

        let tall = tall.to_rgb16();
        let short = short.to_rgb16();
        for (x, y, pixel) in short.enumerate_pixels() {
            assert_eq!(pixel, tall.get_pixel(x, y), "({x}, {y})");
        }
    }

    #[test]
    fn a_measured_profile_of_signal_dependent_noise_follows_the_noise() {
        // Noise whose deviation is set per pixel by the band it falls in, white, in a flat-per-column ramp: measured
        // back as the white residual of that deviation, per band.
        let (width, height) = (1024_u32, 512_u32);
        let picture = DynamicImage::ImageRgb16(ImageBuffer::from_fn(width, height, |x, y| {
            let level = f64::from(x) / f64::from(width - 1);
            let sigma = f64::from(shaped(band(level as f32)));
            let [a, ..] = normals(u64::from(x), u64::from(y), SALT);
            Rgb([((level + sigma * f64::from(a)).clamp(0.0, 1.0) * 65535.0).round() as u16; 3])
        }));

        let profile = measured(&picture).expect("measured");
        // The residual of white noise is sqrt(8 / 9) of it; the middle bands, away from the clamped ends.
        for index in 1..BANDS - 1 {
            let expected = shaped(index) * 0.942_809;
            let sigma = profile.sigma[index][1];
            assert!((sigma - expected).abs() / expected < 0.08, "band {index}: {sigma} against {expected}");
        }
        assert!(profile.correlation > 0.99, "one draw shared by the channels: {}", profile.correlation);
    }

    #[test]
    fn an_edge_among_its_bands_flat_pixels_is_not_grain() {
        // Clean lines a little darker than a flat grey every tenth column: the pixels beside them fall in the grey's
        // band with large residuals, and are fewer than half of it, which the median ignores.
        let lined = DynamicImage::ImageRgb16(ImageBuffer::from_fn(400, 300, |x, _| {
            Rgb([if x % 10 == 0 { 26000 } else { 29500 }; 3])
        }));
        let profile = measured(&lined).expect("measured");

        assert_eq!(band(29500.0 / 65535.0), band(26000.0 / 65535.0), "the lines left the grey's band");
        assert!(profile.sigma.iter().flatten().all(|sigma| *sigma < 1e-6), "{profile:?}");
    }

    #[test]
    fn a_sparse_band_takes_its_nearest_measured_neighbours_figures() {
        // A picture all at one grey, noisy: one band measured, the other seven take its figures.
        let picture = DynamicImage::ImageRgb16(ImageBuffer::from_fn(300, 300, |x, y| {
            let [a, ..] = normals(u64::from(x), u64::from(y), SALT);
            Rgb([((0.3 + 0.01 * f64::from(a)) * 65535.0).round() as u16; 3])
        }));
        let profile = measured(&picture).expect("measured");

        assert!(profile.sigma.iter().all(|other| *other == profile.sigma[band(0.3)]), "{profile:?}");
    }

    #[test]
    fn sixteen_bits_keep_alpha_and_eight_bits_round() {
        let mut rgba =
            DynamicImage::ImageRgba16(ImageBuffer::from_pixel(100, 100, image::Rgba([30000, 30000, 30000, 1234])));
        added(&mut rgba, &target(), SALT);
        let rgba = rgba.as_rgba16().expect("still 16-bit RGBA");

        assert!(rgba.pixels().all(|pixel| pixel.0[3] == 1234), "alpha moved");
        assert!(rgba.pixels().any(|pixel| pixel.0[0] != 30000), "nothing was added");
    }
}
