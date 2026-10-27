//! Box filters over interleaved planes: the mean over a square window around every pixel, and the mean of every
//! block of a reduction.
//!
//! The first caller is the guided upsampling ([`guided`](crate::guided)), which solves its linear models over box
//! windows at a reduced resolution.

// Box means by running sums, so the cost per pixel does not grow with the window. The window is clipped at the edges
// of the plane and divided by the number of pixels it actually holds, the original paper's convention, rather than
// reflected: a reflected window counts the pixels next to an edge twice.

use crate::rows::for_each_row;

/// `data`, `N` interleaved values a pixel over `width` x `height`, reduced by `factor` on each axis: every block's
/// mean, with the blocks along the right and bottom edges as wide as what is left.
pub fn reduced<const N: usize>(data: &[f32], width: usize, height: usize, factor: usize) -> Vec<f32> {
    let (small_width, small_height) = (width.div_ceil(factor), height.div_ceil(factor));
    let mut out = vec![0.0_f32; N * small_width * small_height];

    for_each_row(&mut out, N * small_width, small_height, |row, out| {
        let ys = row * factor..((row + 1) * factor).min(height);

        for (column, mean) in out.as_chunks_mut::<N>().0.iter_mut().enumerate() {
            let xs = column * factor..((column + 1) * factor).min(width);
            let mut sum = [0.0_f32; N];

            for y in ys.clone() {
                for pixel in data[N * (y * width + xs.start)..N * (y * width + xs.end)].as_chunks::<N>().0 {
                    sum = std::array::from_fn(|channel| sum[channel] + pixel[channel]);
                }
            }

            let count = (ys.len() * xs.len()) as f32;
            *mean = sum.map(|value| value / count);
        }
    });

    out
}

/// The mean of each of `data`'s `channels` interleaved planes over the `2 * radius + 1` square around every pixel,
/// clipped at the edges and divided by what it holds.
pub fn box_mean(data: &[f32], channels: usize, width: usize, height: usize, radius: usize) -> Vec<f32> {
    let row_len = channels * width;
    let mut across = vec![0.0_f32; data.len()];

    // Along each row by a running sum in `f64`, so a long row accumulates no drift from what it has already passed.
    for_each_row(&mut across, row_len, height, |y, out| {
        let row = &data[row_len * y..][..row_len];
        let mut prefix = vec![0.0_f64; row_len + channels];

        for (index, value) in row.iter().enumerate() {
            prefix[index + channels] = prefix[index] + f64::from(*value);
        }

        for x in 0..width {
            let (first, last) = (x.saturating_sub(radius), (x + radius).min(width - 1));
            let count = (last - first + 1) as f64;

            for channel in 0..channels {
                let sum = prefix[channels * (last + 1) + channel] - prefix[channels * first + channel];
                out[channels * x + channel] = (sum / count) as f32;
            }
        }
    });

    // Down each column as a direct sum over the window's rows, which keeps the rows independent of each other and so
    // splittable across cores; the window is a handful of rows at the reduced resolution.
    let mut out = vec![0.0_f32; data.len()];

    for_each_row(&mut out, row_len, height, |y, out| {
        let (first, last) = (y.saturating_sub(radius), (y + radius).min(height - 1));

        for source in first..=last {
            for (value, add) in out.iter_mut().zip(&across[row_len * source..][..row_len]) {
                *value += add;
            }
        }

        let count = (last - first + 1) as f32;
        for value in out.iter_mut() {
            *value /= count;
        }
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reproducible stream of values in `[0, 1)`.
    fn noise(count: usize, seed: u32) -> Vec<f32> {
        let mut state = seed;

        (0..count)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1 << 24) as f32
            })
            .collect()
    }

    #[test]
    fn a_box_mean_is_the_mean_of_the_clipped_window() {
        let (width, height, channels) = (23_usize, 17_usize, 2_usize);
        let data = noise(channels * width * height, 3);

        for radius in [0, 1, 4, 30] {
            let out = box_mean(&data, channels, width, height, radius);

            for y in 0..height {
                for x in 0..width {
                    let (xs, ys) = (
                        x.saturating_sub(radius)..=(x + radius).min(width - 1),
                        y.saturating_sub(radius)..=(y + radius).min(height - 1),
                    );
                    let count = (xs.clone().count() * ys.clone().count()) as f32;

                    for channel in 0..channels {
                        let sum: f32 = ys
                            .clone()
                            .flat_map(|sy| xs.clone().map(move |sx| (sx, sy)))
                            .map(|(sx, sy)| data[channels * (sy * width + sx) + channel])
                            .sum();
                        let got = out[channels * (y * width + x) + channel];

                        assert!(
                            (got - sum / count).abs() < 1e-5,
                            "radius {radius} at ({x}, {y}): {got} against {}",
                            sum / count
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_reduction_averages_whole_blocks_and_what_is_left_at_the_edges() {
        // 5x3 by 2: blocks of 2x2, then a column one wide, and a bottom row one tall.
        let data: Vec<f32> = (0..15).flat_map(|index| [index as f32, 1.0]).collect();
        let out = reduced::<2>(&data, 5, 3, 2);

        assert_eq!(out.len(), 2 * 3 * 2);
        assert_eq!(out[0], (0.0 + 1.0 + 5.0 + 6.0) / 4.0);
        assert_eq!(out[2 * 2], (4.0 + 9.0) / 2.0, "the one-wide column is not the mean of what it holds");
        assert_eq!(out[2 * 3], (10.0 + 11.0) / 2.0, "the one-tall row is not the mean of what it holds");
        assert_eq!(out[2 * 5], 14.0, "the corner block is not its one pixel");
        assert!(out.as_chunks::<2>().0.iter().all(|pixel| pixel[1] == 1.0), "a constant channel changed");
    }
}
