//! A single-channel `f32` plane of any size, and its bilinear resample to another size.
//!
//! The type a depth map, a blur radius and a keep-sharp weight are handed around as. Its first caller is the lens blur
//! (`models::lens_blur`), which brings a depth model's map to the photograph's size through [`guided`](crate::guided)
//! and hands [`defocus`](crate::defocus) a radius per pixel in one of these.

// A type rather than the `&[f32]` and two `u32`s the planes elsewhere in this crate travel as. Those are scratch inside
// one function, sized by the shape the function allocated; these cross from one family's model to another family's
// render, through a cache, at a size neither side chose, and a slice that has lost its width is a plane that can be
// read with the wrong stride and still look like a picture.

use thiserror::Error;

use crate::bilinear::Axis;
use crate::rows::for_each_row;

/// The values handed to [`Plane::from_vec`] were not one per pixel of the size it was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a {width}x{height} plane holds {expected} values, but {actual} were supplied")]
pub struct PlaneSize {
    /// How many values the size requires: `width * height`.
    pub expected: usize,
    /// How many the caller supplied.
    pub actual: usize,
    /// The width that was asked for.
    pub width: u32,
    /// The height that was asked for.
    pub height: u32,
}

/// One `f32` per pixel, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Plane {
    /// The number of values in a row.
    width: u32,
    /// The number of rows.
    height: u32,
    /// `width * height` values, row after row.
    data: Vec<f32>,
}

impl Plane {
    /// A `width` x `height` plane holding `fill` everywhere.
    pub fn new(width: u32, height: u32, fill: f32) -> Self {
        Self { width, height, data: vec![fill; (width as usize) * (height as usize)] }
    }

    /// `data` as a `width` x `height` plane, row-major.
    ///
    /// # Errors
    ///
    /// Returns [`PlaneSize`] when `data` is not exactly `width * height` values long.
    pub fn from_vec(width: u32, height: u32, data: Vec<f32>) -> Result<Self, PlaneSize> {
        let expected = (width as usize) * (height as usize);

        if data.len() != expected {
            return Err(PlaneSize { expected, actual: data.len(), width, height });
        }

        Ok(Self { width, height, data })
    }

    /// The number of values in a row.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The number of rows.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The value at `(x, y)`.
    ///
    /// # Panics
    ///
    /// Panics where `(x, y)` is outside the plane.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> f32 {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) is outside a {}x{} plane",
            self.width,
            self.height
        );

        self.data[(y as usize) * (self.width as usize) + (x as usize)]
    }

    /// Every value, row after row.
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Every value, row after row, for a caller that edits the plane in place.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }

    /// The plane resampled to `width` x `height`, bilinearly, with pixel centres aligned and the edges clamped.
    ///
    /// Any size to any size. A plane resized to its own size comes back unchanged, and a target with no area comes
    /// back empty.
    ///
    /// # Panics
    ///
    /// Panics where this plane has no area and the target has some: there is nothing to read it from.
    pub fn resized(&self, width: u32, height: u32) -> Plane {
        if (width, height) == (self.width, self.height) {
            return self.clone();
        }

        if width == 0 || height == 0 {
            return Plane::new(width, height, 0.0);
        }

        assert!(
            self.width > 0 && self.height > 0,
            "a {}x{} plane has nothing to resample",
            self.width,
            self.height
        );

        // `bilinear::Axis`, the align-centres table São Paulo and colorization read their graphs' planes back through,
        // rather than a second convention: a radius plane half a pixel off the photograph it was refined against puts
        // the edge between a sharp subject and a blurred background half a pixel into one of them. Its read is not
        // reused because it is of a square plane; the grouping below is the same.
        let (columns, rows) = (Axis::new(self.width, width), Axis::new(self.height, height));
        let stride = self.width as usize;

        let mut data = vec![0.0; (width as usize) * (height as usize)];

        for_each_row(&mut data, width as usize, height as usize, |y, out| {
            let row = rows.tap(y);
            let (top, bottom) = (&self.data[row.lo * stride..][..stride], &self.data[row.hi * stride..][..stride]);

            for (x, value) in out.iter_mut().enumerate() {
                let column = columns.tap(x);
                let upper = top[column.lo] * (1.0 - column.frac) + top[column.hi] * column.frac;
                let lower = bottom[column.lo] * (1.0 - column.frac) + bottom[column.hi] * column.frac;

                *value = upper * (1.0 - row.frac) + lower * row.frac;
            }
        });

        Plane { width, height, data }
    }

    /// The smallest and largest value, or `(0, 0)` for a plane with no area.
    pub(crate) fn range(&self) -> (f32, f32) {
        if self.data.is_empty() {
            return (0.0, 0.0);
        }

        self.data
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), value| (lo.min(*value), hi.max(*value)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane whose value is its column, so a resample along x is a statement about where each column landed.
    fn ramp(width: u32, height: u32) -> Plane {
        Plane::from_vec(width, height, (0..width * height).map(|index| (index % width) as f32).collect()).unwrap()
    }

    #[test]
    fn a_plane_resized_to_its_own_size_is_unchanged() {
        let plane = Plane::from_vec(7, 5, (0..35).map(|index| (index * 37 % 11) as f32).collect()).unwrap();

        assert_eq!(plane.resized(7, 5), plane);
    }

    #[test]
    fn a_ramp_enlarged_twice_is_still_a_ramp() {
        // Align-centres: destination column `i` reads source coordinate `(i + 0.5) / 2 - 0.5`, which on a ramp whose
        // value is its column is the value itself. Only the outer half pixel at each end clamps to the edge, so the
        // interior must step by exactly a half per column — an align-corners mapping would step by 15/31 instead.
        let enlarged = ramp(16, 3).resized(32, 6);

        for y in 0..6 {
            assert_eq!(enlarged.get(0, y), 0.0, "the first column did not clamp at the edge");
            assert_eq!(enlarged.get(31, y), 15.0, "the last column did not clamp at the edge");

            for x in 1..31 {
                let want = (x as f32 + 0.5) / 2.0 - 0.5;
                assert!(
                    (enlarged.get(x, y) - want).abs() < 1e-5,
                    "({x}, {y}) is {} rather than {want}",
                    enlarged.get(x, y)
                );
            }
        }
    }

    #[test]
    fn a_constant_plane_reduced_stays_constant() {
        let reduced = Plane::new(301, 199, 0.625).resized(37, 11);

        assert_eq!((reduced.width(), reduced.height()), (37, 11));
        assert!(reduced.as_slice().iter().all(|value| (value - 0.625).abs() < 1e-6), "a constant plane changed");
    }

    #[test]
    fn values_that_are_not_one_per_pixel_are_refused() {
        let error = Plane::from_vec(4, 3, vec![0.0; 11]).unwrap_err();

        assert_eq!(error, PlaneSize { expected: 12, actual: 11, width: 4, height: 3 });
        assert_eq!(error.to_string(), "a 4x3 plane holds 12 values, but 11 were supplied");
    }

    #[test]
    fn a_target_with_no_area_comes_back_empty() {
        let empty = ramp(5, 5).resized(0, 9);

        assert_eq!((empty.width(), empty.height()), (0, 9));
        assert!(empty.as_slice().is_empty());
    }
}
