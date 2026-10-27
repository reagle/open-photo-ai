//! A matted subject's own colour: what its pixels would be with the background's share of them taken out.
//!
//! The first caller is the lens blur's matte (`models::lens_blur`), which composites the subject over the blurred
//! background as one layer of this colour times its alpha. With the photograph's own colour instead, every mixed pixel
//! along a strand of hair carries the sharp background it was mixed with onto the blurred one.

// Blur fusion (Forte and Pitié, "Approximate fast foreground colour estimation", ICIP 2021), the two-pass form. Each
// pass takes the local foreground and background colours as box means weighted by the alpha and by one minus it,
// `F = mean(F a) / mean(a)` and `B = mean(B (1 - a)) / mean(1 - a)`, then corrects each pixel's foreground by what of
// the photograph the two do not explain, `F + a (I - a F - (1 - a) B)`, clamped to `[0, 1]`. The first pass takes both
// colours from the photograph; each later one from the pass before. No solve and no iterations: the cost is the box
// means'.
//
// A wide pass runs at a reduced resolution: one whose radius is over `DIRECT_RADIUS` box-reduces its products by about
// `radius / DIRECT_RADIUS` first, takes the means there and reads them back bilinearly. The means over a window that
// wide are smooth by construction, so the coarse grid costs nothing that shows, and the box means' column pass, a
// direct sum over the window's rows, stays a handful of rows. The narrow pass, where the strands are, runs at full
// resolution.
//
// One colour channel at a time, so no more than two full-size planes of products are held at once (with the alpha's
// means, which the three share): at 24 megapixels the seven interleaved together, with the box means' two working
// copies, would be 2 GB.

use crate::bilinear::Axis;
use crate::boxes::{box_mean, reduced_with};
use crate::plane::Plane;
use crate::rows::for_each_row;

/// The widest window, in pixels from the centre, a pass takes its box means over at full resolution.
const DIRECT_RADIUS: u32 = 16;

/// Keeps the weighted means finite where a window holds no foreground, or no background, at all.
const EPSILON: f32 = 1e-5;

/// The foreground colour of `image`, three interleaved values a pixel over `width` x `height`, under `alpha`, by blur
/// fusion with one pass for each of `radii` (window radii in pixels, widest first: 90 and then 6 on a 12 to 24
/// megapixel photograph).
///
/// `image` is linear light in `[0, 1]`, and so is the result. `alpha` is at the photograph's size; it is clamped to
/// `[0, 1]`, and one that is not a number is 0. Where it is 0 or 1 the pixel is the photograph's own colour: there is
/// no foreground there to estimate, or no background to take out.
///
/// # Panics
///
/// Panics where `image` is not three values for every pixel or `alpha` is not `width` x `height`.
pub fn foreground(image: &[f32], alpha: &Plane, width: u32, height: u32, radii: &[u32]) -> Vec<f32> {
    let (columns, rows) = (width as usize, height as usize);

    assert_eq!(
        image.len(),
        3 * columns * rows,
        "the photograph is not three values for each of {width}x{height}"
    );
    assert_eq!((alpha.width(), alpha.height()), (width, height), "the alpha is not the photograph's size");

    let mut foreground = image.to_vec();

    if columns * rows == 0 {
        return foreground;
    }

    let alpha = alpha.as_slice();
    let opacity = |x: usize, y: usize| {
        let value = alpha[y * columns + x];
        if value > 0.0 { value.min(1.0) } else { 0.0 }
    };

    // The background each channel's next pass starts from: the last pass's estimate, or the photograph before the
    // first.
    let mut background: Option<Vec<Grid>> = None;

    for &radius in radii {
        let factor = radius.div_ceil(DIRECT_RADIUS).max(1) as usize;
        let window = (radius as f32 / factor as f32).round() as usize;
        let (small_columns, small_rows) = (columns.div_ceil(factor), rows.div_ceil(factor));

        let coverage = box_mean(
            &reduced_with::<1>(columns, rows, factor, |x, y| [opacity(x, y)]),
            1,
            small_columns,
            small_rows,
            window,
        );
        let mut estimates = Vec::with_capacity(3);

        for channel in 0..3 {
            let products = reduced_with::<2>(columns, rows, factor, |x, y| {
                let (pixel, alpha) = (y * columns + x, opacity(x, y));
                let behind = background.as_ref().map_or(image[3 * pixel + channel], |grids| grids[channel].at(x, y, 1));

                [foreground[3 * pixel + channel] * alpha, behind * (1.0 - alpha)]
            });

            // The two means, into the weighted means they stand for: the foreground's and the background's colour.
            let mut means = box_mean(&products, 2, small_columns, small_rows, window);
            drop(products);

            for (pair, alpha) in means.as_chunks_mut::<2>().0.iter_mut().zip(&coverage) {
                *pair = [pair[0] / (alpha + EPSILON), pair[1] / (1.0 - alpha + EPSILON)];
            }

            let estimate = Grid::new(means, small_columns, small_rows, factor);

            for_each_row(&mut foreground, 3 * columns, rows, |y, out| {
                let original = &image[3 * columns * y..][..3 * columns];

                for x in 0..columns {
                    let (at, alpha) = (3 * x + channel, opacity(x, y));

                    out[at] = if alpha <= 0.0 || alpha >= 1.0 {
                        original[at]
                    } else {
                        let (front, behind) = (estimate.at(x, y, 0), estimate.at(x, y, 1));
                        // The clamp to [0, 1] is for SDR only: an HDR or gain-map photograph has light above 1, which
                        // the slice that carries one must let through.
                        (front + alpha * (original[at] - alpha * front - (1.0 - alpha) * behind)).clamp(0.0, 1.0)
                    };
                }
            });

            estimates.push(estimate);
        }

        background = Some(estimates);
    }

    foreground
}

/// Two interleaved planes of means at a reduced resolution, read back at the photograph's.
struct Grid {
    data: Vec<f32>,
    width: usize,
    across: Axis,
    down: Axis,
}

impl Grid {
    /// `data`, `width` x `height` reduced by `factor` from the photograph.
    fn new(data: Vec<f32>, width: usize, height: usize, factor: usize) -> Self {
        // Over `factor` times the reduced extent, and only as far as the photograph read: a side that is not a
        // multiple of the factor would otherwise be read at a scale that is not the reduction's.
        let across = Axis::new(width as u32, (width * factor) as u32);
        let down = Axis::new(height as u32, (height * factor) as u32);

        Self { data, width, across, down }
    }

    /// Plane `channel` read bilinearly at the photograph's `(x, y)`.
    #[inline]
    fn at(&self, x: usize, y: usize, channel: usize) -> f32 {
        let (row, column) = (self.down.tap(y), self.across.tap(x));
        let read = |row: usize, column: usize| self.data[2 * (row * self.width + column) + channel];

        let upper = read(row.lo, column.lo) * (1.0 - column.frac) + read(row.lo, column.hi) * column.frac;
        let lower = read(row.hi, column.lo) * (1.0 - column.frac) + read(row.hi, column.hi) * column.frac;

        upper * (1.0 - row.frac) + lower * row.frac
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Red over green with an alpha that ramps from 1 to 0 over `ramp` columns starting at `edge`.
    fn red_over_green(width: u32, height: u32, edge: u32, ramp: u32) -> (Vec<f32>, Plane) {
        let alpha: Vec<f32> = (0..width * height)
            .map(|index| 1.0 - ((index % width) as f32 - edge as f32 + 0.5).clamp(0.0, ramp as f32) / ramp as f32)
            .collect();
        let image = alpha.iter().flat_map(|alpha| [*alpha, 1.0 - alpha, 0.0]).collect();

        (image, Plane::from_vec(width, height, alpha).unwrap())
    }

    #[test]
    fn a_red_subject_over_green_comes_back_red_across_its_edge() {
        // The spec's scenario at both resolutions, with the passes the lens blur uses: a wide first pass, reduced by
        // six, and a narrow one at full size. The worst pixel is 4 percent off red, as in the prototype; a first
        // window not much wider than the ramp leaves its low-alpha end about a tenth green, in both.
        let (width, height) = (400_u32, 120_u32);
        let (image, alpha) = red_over_green(width, height, 140, 20);

        let out = foreground(&image, &alpha, width, height, &[90, 6]);

        for (index, (rgb, alpha)) in out.as_chunks::<3>().0.iter().zip(alpha.as_slice()).enumerate() {
            if *alpha > 0.0 && *alpha < 1.0 {
                let off = (rgb[0] - 1.0).abs().max(rgb[1]).max(rgb[2]);
                assert!(off < 0.1, "at {index} (alpha {alpha}) the foreground is {rgb:?}");
            }
        }

        // And it is the separation doing it: the photograph itself in the ramp is nowhere near red.
        let middle = (60 * width + 150) as usize;
        assert!(image[3 * middle + 1] > 0.4, "the ramp's middle is not a mix to begin with");
    }

    #[test]
    fn where_the_alpha_is_whole_or_absent_the_photograph_is_its_own_foreground() {
        let (width, height) = (64_u32, 48_u32);
        let image: Vec<f32> = (0..3 * width * height).map(|index| (index * 37 % 101) as f32 / 100.0).collect();

        // Everywhere 1, everywhere 0, and half and half with no mixed pixel at all.
        for alpha in [
            Plane::new(width, height, 1.0),
            Plane::new(width, height, 0.0),
            Plane::from_vec(width, height, (0..width * height).map(|index| f32::from(index % width < 30)).collect())
                .unwrap(),
        ] {
            let out = foreground(&image, &alpha, width, height, &[20, 3]);
            assert!(
                out.iter().zip(&image).all(|(a, b)| a.to_bits() == b.to_bits()),
                "a pixel with nothing to separate moved"
            );
        }
    }

    #[test]
    fn no_passes_return_the_photograph() {
        let (image, alpha) = red_over_green(40, 10, 10, 10);

        assert_eq!(foreground(&image, &alpha, 40, 10, &[]), image);
    }
}
