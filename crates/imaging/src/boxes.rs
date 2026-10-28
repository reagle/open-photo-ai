//! Box filters over interleaved planes: the mean of every block of a reduction.
//!
//! The caller here is the push-pull fill ([`fill`](crate::fill)), which builds its pyramid from reductions by two. (The
//! lens-blur line's copy of this module also has the box mean its guided upsampling and blur fusion use.)

use crate::rows::for_each_row;

/// `data`, `N` interleaved values a pixel over `width` x `height`, reduced by `factor` on each axis: every block's
/// mean, with the blocks along the right and bottom edges as wide as what is left.
pub fn reduced<const N: usize>(data: &[f32], width: usize, height: usize, factor: usize) -> Vec<f32> {
    reduced_with::<N>(width, height, factor, |x, y| std::array::from_fn(|channel| data[N * (y * width + x) + channel]))
}

/// [`reduced`] of the `N` values `at` answers for each pixel `(x, y)` of `width` x `height`.
///
/// For a caller whose values are worked out per pixel (a product of two planes, a weight times a colour), so the
/// full-size interleaved plane they would make is never allocated: at 24 megapixels, four values a pixel are 384 MB.
pub fn reduced_with<const N: usize>(
    width: usize,
    height: usize,
    factor: usize,
    at: impl Fn(usize, usize) -> [f32; N] + Sync,
) -> Vec<f32> {
    let (small_width, small_height) = (width.div_ceil(factor), height.div_ceil(factor));
    let mut out = vec![0.0_f32; N * small_width * small_height];

    for_each_row(&mut out, N * small_width, small_height, |row, out| {
        let ys = row * factor..((row + 1) * factor).min(height);

        for (column, mean) in out.as_chunks_mut::<N>().0.iter_mut().enumerate() {
            let xs = column * factor..((column + 1) * factor).min(width);
            let mut sum = [0.0_f32; N];

            for y in ys.clone() {
                for x in xs.clone() {
                    let pixel = at(x, y);
                    sum = std::array::from_fn(|channel| sum[channel] + pixel[channel]);
                }
            }

            let count = (ys.len() * xs.len()) as f32;
            *mean = sum.map(|value| value / count);
        }
    });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
