//! Grey erosion and dilation by a square: each pixel takes the smallest, or the largest, value within `radius` pixels
//! of it on both axes.
//!
//! The first caller is the lens blur's trimap (`models::lens_blur`): the in-focus band of the depth map eroded into
//! the sure subject and dilated into everything that might be, with the unknown band between them for the matting
//! model.

// A square rather than a disc: the trimap's band edge does not need to be round, and a square separates into a pass
// along the rows and one down the columns, each a running minimum by the van Herk/Gil-Werman method, three comparisons
// a pixel whatever the radius. A trimap's radius is tens of pixels, where a direct window would be a hundred
// comparisons a pixel. `guided::dilated_nearer` is a disc for the edge grow, whose radius is a few pixels.
//
// The window is clipped at the edges of the plane, as the box means are: beyond the edge is no value at all rather
// than a copy of the edge, which for an extremum is the same answer.

use crate::plane::Plane;
use crate::rows::for_each_row;

/// `plane` eroded by the `2 * radius + 1` square: each pixel takes the smallest value within `radius` pixels of it on
/// both axes. A radius of 0 returns the plane as it is.
pub fn eroded(plane: &Plane, radius: u32) -> Plane {
    filtered(plane, radius, f32::INFINITY, f32::min)
}

/// `plane` dilated by the `2 * radius + 1` square: each pixel takes the largest value within `radius` pixels of it on
/// both axes. A radius of 0 returns the plane as it is.
pub fn dilated(plane: &Plane, radius: u32) -> Plane {
    filtered(plane, radius, f32::NEG_INFINITY, f32::max)
}

/// The square filter for one extremum: `pick` of two values, and `none`, the value `pick` always passes over.
fn filtered(plane: &Plane, radius: u32, none: f32, pick: impl Fn(f32, f32) -> f32 + Copy + Sync) -> Plane {
    let (width, height) = (plane.width() as usize, plane.height() as usize);

    if radius == 0 || width * height == 0 {
        return plane.clone();
    }

    // Down the columns as along the rows of the transposed plane, so both passes are rows and split across cores.
    let across = along_rows(plane.as_slice(), width, height, radius as usize, none, pick);
    let down = along_rows(&transposed(&across, width, height), height, width, radius as usize, none, pick);

    Plane::from_vec(plane.width(), plane.height(), transposed(&down, height, width))
        .expect("one value for every pixel of the plane")
}

/// Each row of `data` filtered by the `2 * radius + 1` window along it.
fn along_rows(
    data: &[f32],
    width: usize,
    height: usize,
    radius: usize,
    none: f32,
    pick: impl Fn(f32, f32) -> f32 + Sync,
) -> Vec<f32> {
    // A window past the whole row is the whole row from every pixel, and a larger one would only pad more.
    let radius = radius.min(width);
    let window = 2 * radius + 1;

    // The row with `radius` of `none` before it and enough after it to fill the last block of `window`.
    let padded = (width + 2 * radius).div_ceil(window) * window;

    let mut out = vec![0.0_f32; data.len()];

    for_each_row(&mut out, width, height, |y, out| {
        let row = &data[width * y..][..width];
        let (mut forward, mut backward) = (vec![none; padded], vec![none; padded]);

        forward[radius..radius + width].copy_from_slice(row);
        backward[radius..radius + width].copy_from_slice(row);

        // Within each block of `window`, the extremum so far from its start and from its end. A window of that width
        // starting anywhere crosses at most one block boundary, so it is the backward value at its start and the
        // forward one at its end.
        for block in forward.chunks_exact_mut(window) {
            for index in 1..window {
                block[index] = pick(block[index], block[index - 1]);
            }
        }

        for block in backward.chunks_exact_mut(window) {
            for index in (0..window - 1).rev() {
                block[index] = pick(block[index], block[index + 1]);
            }
        }

        for (x, value) in out.iter_mut().enumerate() {
            *value = pick(backward[x], forward[x + window - 1]);
        }
    });

    out
}

/// `data`, `width` x `height`, with its rows and columns swapped.
fn transposed(data: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut out = vec![0.0_f32; data.len()];

    for_each_row(&mut out, height, width, |x, out| {
        for (y, value) in out.iter_mut().enumerate() {
            *value = data[width * y + x];
        }
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reproducible plane of values in `[0, 1)`.
    fn noise(width: u32, height: u32, seed: u32) -> Plane {
        let mut state = seed;
        let values = (0..width * height)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32
            })
            .collect();

        Plane::from_vec(width, height, values).unwrap()
    }

    /// The extremum over the clipped square, by looking at every pixel in it.
    fn brute_force(plane: &Plane, radius: u32, pick: fn(f32, f32) -> f32, x: u32, y: u32) -> f32 {
        let xs = x.saturating_sub(radius)..=(x + radius).min(plane.width() - 1);
        let ys = y.saturating_sub(radius)..=(y + radius).min(plane.height() - 1);

        ys.flat_map(|sy| xs.clone().map(move |sx| (sx, sy)))
            .map(|(sx, sy)| plane.get(sx, sy))
            .reduce(pick)
            .unwrap()
    }

    #[test]
    fn the_running_extrema_match_a_brute_force_square() {
        // 37 x 29, so neither side is a multiple of any window from 3 to 19 and the last block is always cut short.
        let plane = noise(37, 29, 5);

        for radius in 1..=9 {
            let (low, high) = (eroded(&plane, radius), dilated(&plane, radius));

            for y in 0..29 {
                for x in 0..37 {
                    assert_eq!(
                        low.get(x, y),
                        brute_force(&plane, radius, f32::min, x, y),
                        "erosion {radius} ({x}, {y})"
                    );
                    assert_eq!(
                        high.get(x, y),
                        brute_force(&plane, radius, f32::max, x, y),
                        "dilation {radius} ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn a_radius_past_the_plane_takes_the_whole_plane() {
        let plane = noise(13, 6, 9);
        let (lo, hi) = plane.range();

        for radius in [13, 40, u32::MAX] {
            assert!(eroded(&plane, radius).as_slice().iter().all(|value| *value == lo), "erosion by {radius}");
            assert!(dilated(&plane, radius).as_slice().iter().all(|value| *value == hi), "dilation by {radius}");
        }
    }

    #[test]
    fn a_radius_of_zero_changes_nothing() {
        let plane = noise(11, 7, 13);

        assert_eq!(eroded(&plane, 0), plane);
        assert_eq!(dilated(&plane, 0), plane);
    }

    #[test]
    fn a_band_eroded_and_dilated_moves_its_edge_by_the_radius() {
        // A vertical band over columns 10 to 19: the trimap's use, where the sure subject is the band eroded and the
        // possible subject the band dilated.
        let band =
            Plane::from_vec(30, 8, (0..30 * 8).map(|index| f32::from((10..20).contains(&(index % 30)))).collect());
        let band = band.unwrap();
        let (sure, maybe) = (eroded(&band, 3), dilated(&band, 3));

        for y in 0..8 {
            for x in 0..30 {
                assert_eq!(sure.get(x, y), f32::from((13..17).contains(&x)), "the eroded band at ({x}, {y})");
                assert_eq!(maybe.get(x, y), f32::from((7..23).contains(&x)), "the dilated band at ({x}, {y})");
            }
        }
    }
}
