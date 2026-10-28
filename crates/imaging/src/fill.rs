//! Values filled in where a weight says they are unknown, from the known values around them, by a push-pull pyramid.
//!
//! The first caller is the lens blur's matte (`models::lens_blur`): the background behind a matted subject, its colour
//! and its depth, as if the subject were not there, so that the render blurs the background without the subject in
//! it and the subject is composited over the result.

// Push-pull (Gortler, Grzeszczuk, Szeliski and Cohen, "The Lumigraph", 1996): down a pyramid of weighted means, each
// level the 2x2 box reduction of the weighted values and of the weights, then back up, each level's pixel taking its
// own weighted mean for as much as its weight says is known and the coarser level's fill for the rest. Exact where the
// weight is 1, a smooth continuation where it is 0, linear in the pixel count, and with no tiles and no iterations, so
// no seams and no diffusion to converge.
//
// The reductions are `boxes`', whose blocks along the right and bottom edges are as wide as what is left, so a hole at
// the frame's edge fills like any other; the enlargement is the align-centres bilinear read at exactly two, as the
// defocus render enlarges its reduced layers. The pyramid goes down to one pixel: a hole is filled from the nearest
// level that has a weight above 0 under it, and a block's weight is the mean of its pixels' weights, so no level has
// every weight at 1 while a hole is left in the finest.
//
// The weights are means, not coverage, so the fill leaks: a block that is part hole has a weight under 1 and takes the
// coarser level's fill for the rest, so the fill is drawn a little towards the means of wider surroundings. Across the
// 100 px hole in the gradient test it is off by 1.4% of the gradient's range at worst. Close enough for a background
// that is then blurred; not an inpainting.

use crate::bilinear::Axis;
use crate::boxes::{box_mean, reduced, reduced_with};
use crate::plane::Plane;
use crate::rows::for_each_row;

/// `values`, `channels` interleaved a pixel over `width` x `height`, filled in where `weight` is under 1 from where it
/// is not.
///
/// `weight` is at the values' size and in `[0, 1]` (anything else is clamped into it, and a weight that is not a
/// number is 0). A value whose weight is 1 comes back unchanged and one whose weight is 0 is replaced entirely by the
/// fill; between, the two are mixed by the weight. Where no weight is above 0 there is nothing to fill from, and every
/// pixel becomes the plain mean of the values.
///
/// # Panics
///
/// Panics where `channels` is not 1 to 4, `values` is not `channels` values for every pixel, or `weight` is not
/// `width` x `height`.
pub fn filled(values: &[f32], channels: usize, width: u32, height: u32, weight: &Plane) -> Vec<f32> {
    let (columns, rows) = (width as usize, height as usize);

    assert_eq!(
        values.len(),
        channels * columns * rows,
        "the values are not {channels} for each of {width}x{height}"
    );
    assert_eq!((weight.width(), weight.height()), (width, height), "the weight plane is not the values' size");

    // The pyramid's levels carry the weighted values and the weight together, `channels + 1` of them a pixel, as a
    // fixed-size array so that the reductions are `boxes`'.
    match channels {
        1 => pushed_pulled::<1, 2>(values, columns, rows, weight.as_slice()),
        2 => pushed_pulled::<2, 3>(values, columns, rows, weight.as_slice()),
        3 => pushed_pulled::<3, 4>(values, columns, rows, weight.as_slice()),
        4 => pushed_pulled::<4, 5>(values, columns, rows, weight.as_slice()),
        _ => panic!("{channels} channels: the fill takes 1 to 4"),
    }
}

/// `values` as [`filled`] takes them, filled in where `weight` is under 1 with the values of the nearest pixel whose
/// weight is a half or more, smoothed over the known pixels within `smoothing` of it.
///
/// Push-pull draws a wide hole's fill towards the means of wider surroundings; behind a thick branch over a dark
/// street with pale tiles below, its fill came out grey, and a blur spread that grey out past the branch's edge as a
/// light halo. The nearest known value continues what lies just outside each edge, so near an edge the fill is the
/// background beside it. Where nothing is known, the fill is [`filled`]'s.
///
/// # Panics
///
/// As [`filled`].
pub fn nearest_filled(
    values: &[f32],
    channels: usize,
    width: u32,
    height: u32,
    weight: &Plane,
    smoothing: u32,
) -> Vec<f32> {
    let (columns, rows) = (width as usize, height as usize);
    assert_eq!(
        values.len(),
        channels * columns * rows,
        "the values are not {channels} for each of {width}x{height}"
    );
    assert_eq!((weight.width(), weight.height()), (width, height), "the weight plane is not the values' size");

    let weight = weight.as_slice();
    let seed = |index: usize| known(weight[index]) >= 0.5;
    if !(0..weight.len()).any(seed) {
        return filled(
            values,
            channels,
            width,
            height,
            &Plane::from_vec(width, height, weight.to_vec()).expect("a plane"),
        );
    }

    let nearest = nearest_seeds(columns, rows, seed);

    // The known values smoothed over the known alone, so a seed's noise is not drawn out in streaks across a hole.
    let mask: Vec<f32> = (0..weight.len()).map(|index| f32::from(u8::from(seed(index)))).collect();
    let weighted: Vec<f32> = values
        .iter()
        .enumerate()
        .map(|(i, v)| if mask[i / channels] > 0.0 { *v } else { 0.0 })
        .collect();
    let (sums, counts) = (
        box_mean(&weighted, channels, columns, rows, smoothing as usize),
        box_mean(&mask, 1, columns, rows, smoothing as usize),
    );

    let mut out = vec![0.0_f32; values.len()];
    for_each_row(&mut out, channels * columns, rows, |y, out| {
        for (x, value) in out.chunks_exact_mut(channels).enumerate() {
            let pixel = y * columns + x;
            let own = known(weight[pixel]);
            let from = nearest[pixel] as usize;
            for (channel, value) in value.iter_mut().enumerate() {
                let fill = sums[channels * from + channel] / counts[from].max(f32::MIN_POSITIVE);
                *value = if own >= 1.0 {
                    values[channels * pixel + channel]
                } else {
                    own * values[channels * pixel + channel] + (1.0 - own) * fill
                };
            }
        }
    });
    out
}

/// For every pixel of a `columns` x `rows` grid, the index of the nearest pixel `seed` says is one, by two passes of
/// the eight neighbours' nearest (an approximate Euclidean distance transform, exact but for rare ties far from any
/// seed). At least one pixel must be a seed.
fn nearest_seeds(columns: usize, rows: usize, seed: impl Fn(usize) -> bool) -> Vec<u32> {
    const NONE: u32 = u32::MAX;
    let mut nearest: Vec<u32> =
        (0..columns * rows).map(|index| if seed(index) { index as u32 } else { NONE }).collect();
    let distance = |from: u32, x: usize, y: usize| {
        let (fx, fy) = ((from as usize % columns) as i64, (from as usize / columns) as i64);
        (fx - x as i64).pow(2) + (fy - y as i64).pow(2)
    };
    let visit = |nearest: &mut Vec<u32>, x: usize, y: usize, neighbours: &[(isize, isize)]| {
        let index = y * columns + x;
        let mut best = nearest[index];
        let mut best_distance = if best == NONE { i64::MAX } else { distance(best, x, y) };
        for &(dx, dy) in neighbours {
            let (nx, ny) = (x as isize + dx, y as isize + dy);
            if nx < 0 || ny < 0 || nx >= columns as isize || ny >= rows as isize {
                continue;
            }
            let candidate = nearest[ny as usize * columns + nx as usize];
            if candidate != NONE {
                let d = distance(candidate, x, y);
                if d < best_distance {
                    (best, best_distance) = (candidate, d);
                }
            }
        }
        nearest[index] = best;
    };
    for y in 0..rows {
        for x in 0..columns {
            visit(&mut nearest, x, y, &[(-1, 0), (-1, -1), (0, -1), (1, -1)]);
        }
    }
    for y in (0..rows).rev() {
        for x in (0..columns).rev() {
            visit(&mut nearest, x, y, &[(1, 0), (1, 1), (0, 1), (-1, 1)]);
        }
    }
    nearest
}

/// A weight as the fill reads it: in `[0, 1]`, and 0 where it is not a number.
#[inline]
fn known(weight: f32) -> f32 {
    if weight > 0.0 { weight.min(1.0) } else { 0.0 }
}

/// [`filled`] for `C` channels, whose pyramid levels hold `N = C + 1` values a pixel: the weighted values, then the
/// weight.
fn pushed_pulled<const C: usize, const N: usize>(
    values: &[f32],
    width: usize,
    height: usize,
    weight: &[f32],
) -> Vec<f32> {
    debug_assert_eq!(N, C + 1);

    if weight.iter().all(|weight| known(*weight) >= 1.0) {
        return values.to_vec();
    }

    if weight.iter().all(|weight| known(*weight) == 0.0) {
        let mut sum = [0.0_f64; C];
        for pixel in values.as_chunks::<C>().0 {
            sum = std::array::from_fn(|channel| sum[channel] + f64::from(pixel[channel]));
        }

        let count = (width * height) as f64;
        return sum.map(|value| (value / count) as f32).repeat(width * height);
    }

    // Down: the first level straight from the values, so the weighted full-size plane is never allocated, then by two
    // until there is nothing left to fill.
    let mut levels = vec![Level {
        data: reduced_with::<N>(width, height, 2, |x, y| {
            let pixel = y * width + x;
            let weight = known(weight[pixel]);

            // A value with no weight is not read at all: it may be anything, a NaN among them, and NaN times 0 is NaN.
            std::array::from_fn(|channel| {
                if channel == C {
                    weight
                } else if weight > 0.0 {
                    values[C * pixel + channel] * weight
                } else {
                    0.0
                }
            })
        }),
        width: width.div_ceil(2),
        height: height.div_ceil(2),
    }];

    loop {
        let level = levels.last().expect("the first level is pushed above");

        if level.width * level.height == 1 {
            break;
        }

        let data = reduced::<N>(&level.data, level.width, level.height, 2);
        let (width, height) = (level.width.div_ceil(2), level.height.div_ceil(2));
        levels.push(Level { data, width, height });
    }

    // Up: the coarsest level is its own weighted means, which no pixel of it is without; each finer level its own
    // means mixed with the coarser one's fill by its weight.
    let coarsest = levels.pop().expect("there is at least one level");
    let mut fill: Vec<f32> = coarsest
        .data
        .as_chunks::<N>()
        .0
        .iter()
        .flat_map(|pixel| std::array::from_fn::<f32, C, _>(|channel| own_mean::<N>(pixel, channel)))
        .collect();
    let (mut fill_width, mut fill_height) = (coarsest.width, coarsest.height);

    for level in levels.iter().rev() {
        fill = pulled::<C, N>(level.width, level.height, &fill, fill_width, fill_height, |x, y| {
            let pixel = &level.data[N * (y * level.width + x)..][..N];
            (std::array::from_fn(|channel| own_mean::<N>(pixel, channel)), pixel[C].min(1.0))
        });
        (fill_width, fill_height) = (level.width, level.height);
    }

    pulled::<C, N>(width, height, &fill, fill_width, fill_height, |x, y| {
        let pixel = y * width + x;
        (std::array::from_fn(|channel| values[C * pixel + channel]), known(weight[pixel]))
    })
}

/// One level of the pyramid: `N` values a pixel, the weighted values then the weight.
struct Level {
    data: Vec<f32>,
    width: usize,
    height: usize,
}

/// A pyramid pixel's own weighted mean for `channel`, or 0 where it has no weight.
#[inline]
fn own_mean<const N: usize>(pixel: &[f32], channel: usize) -> f32 {
    let weight = pixel[N - 1];
    if weight > 0.0 { pixel[channel] / weight } else { 0.0 }
}

/// A `width` x `height` level filled: each pixel's own `C` values, as `own` answers them with their weight, mixed by
/// that weight with `coarse`, the `C`-channel fill of the level above, enlarged by two.
fn pulled<const C: usize, const N: usize>(
    width: usize,
    height: usize,
    coarse: &[f32],
    coarse_width: usize,
    coarse_height: usize,
    own: impl Fn(usize, usize) -> ([f32; C], f32) + Sync,
) -> Vec<f32> {
    // Over twice the coarse extent and only the first `width` entries read, as `guided::upsampled` does and for its
    // reason: an odd side's last coarse pixel stands for one pixel, not two.
    let across = Axis::new(coarse_width as u32, 2 * coarse_width as u32);
    let down = Axis::new(coarse_height as u32, 2 * coarse_height as u32);

    let mut out = vec![0.0_f32; C * width * height];

    for_each_row(&mut out, C * width, height, |y, out| {
        let row = down.tap(y);
        let (top, bottom) = (&coarse[C * coarse_width * row.lo..], &coarse[C * coarse_width * row.hi..]);

        for (x, value) in out.as_chunks_mut::<C>().0.iter_mut().enumerate() {
            let (own, weight) = own(x, y);

            // Exactly the pixel's own where it is wholly known, and exactly the fill where nothing of it is: no
            // rounding of a known value, and nothing read of an unknown one.
            if weight >= 1.0 {
                *value = own;
                continue;
            }

            let column = across.tap(x);
            let up: [f32; C] = std::array::from_fn(|channel| {
                let (lo, hi) = (C * column.lo + channel, C * column.hi + channel);
                let upper = top[lo] * (1.0 - column.frac) + top[hi] * column.frac;
                let lower = bottom[lo] * (1.0 - column.frac) + bottom[hi] * column.frac;

                upper * (1.0 - row.frac) + lower * row.frac
            });

            *value = if weight > 0.0 {
                std::array::from_fn(|channel| weight * own[channel] + (1.0 - weight) * up[channel])
            } else {
                up
            };
        }
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A weight of 1 everywhere but a `side` square of 0 at `(x0, y0)`.
    fn holed(width: u32, height: u32, x0: u32, y0: u32, side: u32) -> Plane {
        let inside = |x: u32, y: u32| (x0..x0 + side).contains(&x) && (y0..y0 + side).contains(&y);
        let values = (0..width * height).map(|index| f32::from(!inside(index % width, index / width))).collect();

        Plane::from_vec(width, height, values).unwrap()
    }

    #[test]
    fn a_hole_in_a_gradient_is_filled_close_to_the_gradient_and_nothing_known_moves() {
        let (width, height) = (400_u32, 300_u32);
        let gradient: Vec<f32> = (0..width * height).map(|index| (index % width) as f32 / (width - 1) as f32).collect();

        // Garbage in the hole, which the fill must not read.
        let weight = holed(width, height, 150, 100, 100);
        let mut values = gradient.clone();
        for (value, weight) in values.iter_mut().zip(weight.as_slice()) {
            if *weight == 0.0 {
                *value = f32::NAN;
            }
        }

        let out = filled(&values, 1, width, height, &weight);

        for (index, ((got, want), weight)) in out.iter().zip(&gradient).zip(weight.as_slice()).enumerate() {
            if *weight == 1.0 {
                assert_eq!(got.to_bits(), want.to_bits(), "a known value moved at {index}");
            } else {
                assert!((got - want).abs() < 0.05, "the fill at {index} is {got} where the gradient is {want}");
            }
        }
    }

    #[test]
    fn a_hole_at_the_frame_edge_is_filled() {
        // The bottom-right corner, on a frame whose sides are odd, so the pyramid's ragged last blocks hold the hole.
        let (width, height) = (151_u32, 97_u32);
        let values: Vec<f32> = (0..width * height).flat_map(|_| [0.25, 0.5, 0.75]).collect();
        let weight = holed(width, height, 120, 70, 40);

        let out = filled(&values, 3, width, height, &weight);

        for (index, pixel) in out.as_chunks::<3>().0.iter().enumerate() {
            for (channel, want) in [0.25_f32, 0.5, 0.75].into_iter().enumerate() {
                assert!(
                    (pixel[channel] - want).abs() < 1e-5,
                    "pixel {index}'s channel {channel} is {}",
                    pixel[channel]
                );
            }
        }
    }

    #[test]
    fn with_nothing_known_every_pixel_is_the_mean() {
        let values: Vec<f32> = (0..30 * 20).map(|index| (index % 7) as f32).collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;

        let out = filled(&values, 1, 30, 20, &Plane::new(30, 20, 0.0));

        assert!(out.iter().all(|value| (value - mean).abs() < 1e-5), "the fill of nothing is not one value");
    }

    #[test]
    fn channels_are_filled_independently_from_the_same_weights() {
        // Two channels running opposite ways; filled together they must each match their own fill alone.
        let (width, height) = (90_u32, 70_u32);
        let rising: Vec<f32> = (0..width * height).map(|index| (index % width) as f32 / 89.0).collect();
        let falling: Vec<f32> = rising.iter().map(|value| 1.0 - value).collect();
        let both: Vec<f32> = rising.iter().zip(&falling).flat_map(|(a, b)| [*a, *b]).collect();
        let weight = holed(width, height, 30, 20, 30);

        let together = filled(&both, 2, width, height, &weight);
        let (alone_rising, alone_falling) =
            (filled(&rising, 1, width, height, &weight), filled(&falling, 1, width, height, &weight));

        for (index, pixel) in together.as_chunks::<2>().0.iter().enumerate() {
            assert!((pixel[0] - alone_rising[index]).abs() < 1e-6, "channel 0 at {index}");
            assert!((pixel[1] - alone_falling[index]).abs() < 1e-6, "channel 1 at {index}");
            assert!((pixel[0] + pixel[1] - 1.0).abs() < 1e-5, "the two fills disagree at {index}");
        }
    }

    #[test]
    fn a_partial_weight_mixes_the_value_with_the_fill() {
        // A known field of 1 around one pixel of 0 at half weight: neither its own value nor the field's, and nothing
        // else moves.
        let mut values = vec![1.0_f32; 9 * 9];
        values[4 * 9 + 4] = 0.0;
        let mut weight = Plane::new(9, 9, 1.0);
        weight.as_mut_slice()[4 * 9 + 4] = 0.5;

        let out = filled(&values, 1, 9, 9, &weight);
        let mixed = out[4 * 9 + 4];

        assert!(mixed > 0.25 && mixed < 0.75, "the half-known pixel is {mixed}");
        assert!(out.iter().enumerate().all(|(index, value)| index == 4 * 9 + 4 || *value == 1.0));
    }

    #[test]
    fn nothing_to_fill_returns_the_values() {
        let values: Vec<f32> = (0..12 * 5).map(|index| index as f32).collect();

        assert_eq!(filled(&values, 1, 12, 5, &Plane::new(12, 5, 1.0)), values);
        assert_eq!(filled(&[], 3, 0, 0, &Plane::new(0, 0, 0.0)), Vec::<f32>::new());
    }

    #[test]
    fn the_nearest_fill_continues_the_background_beside_each_edge_and_not_a_brighter_one_further_off() {
        // A 100 px hole (a branch) over a dark street, the column right of it at 0.1, with pale tiles at 0.9 below the
        // street; push-pull draws the hole's fill towards the tiles, the nearest fill keeps the street beside the edge.
        let (width, height) = (300, 300);
        let weight = holed(width, height, 100, 0, 100);
        let values: Vec<f32> = (0..width * height).map(|index| if index / width < 150 { 0.1 } else { 0.9 }).collect();

        let nearest = nearest_filled(&values, 1, width, height, &weight, 2);
        let pyramid = filled(&values, 1, width, height, &weight);

        let at = |x: u32, y: u32| (y * width + x) as usize;
        assert!(
            (nearest[at(195, 20)] - 0.1).abs() < 1e-5,
            "beside the edge, the street: {}",
            nearest[at(195, 20)]
        );
        assert!(
            pyramid[at(195, 20)] > 0.11,
            "push-pull is drawn towards the tiles there: {}",
            pyramid[at(195, 20)]
        );
        assert_eq!(nearest[at(20, 20)], 0.1, "a known value is its own");
    }
}
