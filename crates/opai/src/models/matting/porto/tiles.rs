//! Porto's tiles: which 1024-pixel tiles of a photograph hold an unknown pixel, what the graph is fed for each, and how
//! their alphas are joined into one matte.

// The tiles of the OpenSpec design of `add-lens-blur-8-matting` (X3): 1024 px with 128 px of overlap, corners clamped
// to the frame, so every tile of a photograph larger than the tile lies wholly inside it and one smaller than it runs
// off its right or bottom edge. What runs off is zeros in all four channels, which is the padding ViTMatte's own
// processor adds. Each tile's alpha is weighted by a feather that ramps over the overlap on all four sides, so
// neighbouring tiles cross-fade rather than meet at a seam; a pixel's alpha is the weighted mean of the tiles over it.

use imaging::plane::Plane;
use imaging::tensor::Sampler;

/// The graph's tile, in pixels on each side.
pub(crate) const TILE: u32 = 1024;

/// How far neighbouring tiles overlap, in pixels: the width of the cross-fade between them.
pub(crate) const OVERLAP: u32 = 128;

/// The planes the graph is fed: the photograph's three, then the trimap.
pub(crate) const PLANES: usize = 4;

/// Whether a trimap value marks an unknown pixel: anything between sure background (0) and sure subject (1).
pub(crate) fn unknown(value: f32) -> bool {
    value > 0.0 && value < 1.0
}

/// The top-left corners of the tiles of `trimap` that hold an unknown pixel, row by row.
///
/// Tiles step by `TILE - OVERLAP` from the top left; the last of a row or column is moved back to end at the frame's
/// edge rather than run off it, unless the frame is smaller than a tile.
pub(crate) fn tiles(trimap: &Plane) -> Vec<(u32, u32)> {
    let (width, height) = (trimap.width(), trimap.height());
    let starts = |size: u32| {
        let mut starts: Vec<u32> = (0..size.saturating_sub(OVERLAP).max(1))
            .step_by((TILE - OVERLAP) as usize)
            .map(|start| start.min(size.saturating_sub(TILE)))
            .collect();
        starts.dedup();

        starts
    };
    let (columns, rows) = (starts(width), starts(height));

    rows.iter()
        .flat_map(|&y| columns.iter().map(move |&x| (x, y)))
        .filter(|&(x, y)| {
            (y..(y + TILE).min(height))
                .any(|row| (x..(x + TILE).min(width)).any(|column| unknown(trimap.get(column, row))))
        })
        .collect()
}

/// The graph's input for the tile at (`x0`, `y0`): four `TILE` x `TILE` planes, the photograph's red, green and blue
/// scaled from `[0, 1]` to `[-1, 1]`, then the trimap as it is; zeros where the tile runs off the frame.
///
/// `out` is `PLANES * TILE * TILE` long and is overwritten.
pub(crate) fn tensor(sampler: &Sampler<'_>, trimap: &Plane, x0: u32, y0: u32, out: &mut [f32]) {
    let side = TILE as usize;
    let plane = side * side;
    debug_assert_eq!(out.len(), PLANES * plane, "a tile's tensor is four planes of a tile");

    out.fill(0.0);

    let (width, height) = (trimap.width(), trimap.height());
    let (columns, rows) = ((width - x0).min(TILE) as usize, (height - y0).min(TILE) as usize);
    let mut pixels = vec![[0_u16; 3]; width as usize];

    for row in 0..rows {
        #[allow(clippy::cast_possible_truncation, reason = "a row of a tile of a picture whose height is a u32")]
        let y = y0 + row as u32;
        sampler.row(y, &mut pixels);

        for (column, pixel) in pixels[x0 as usize..][..columns].iter().enumerate() {
            let at = row * side + column;
            for (channel, value) in pixel.iter().enumerate() {
                // ViTMatte's processor: rescaled to [0, 1], then normalised by a mean and a deviation of 0.5.
                out[channel * plane + at] = f32::from(*value) / f32::from(u16::MAX) * 2.0 - 1.0;
            }
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a column of a tile of a picture whose width is a u32"
            )]
            let x = x0 + column as u32;
            out[3 * plane + at] = trimap.get(x, y);
        }
    }
}

/// The weight a tile's pixel `at` along one axis is given: rising over the first `OVERLAP` pixels and falling over the
/// last, 1 between.
fn feather(at: usize) -> f32 {
    let ramp = |at: usize| ((at as f32 + 0.5) / OVERLAP as f32).min(1.0);

    ramp(at).min(ramp(TILE as usize - 1 - at))
}

/// The tiles' alphas, summed by their feathers, and the feathers summed: a matte once divided.
pub(crate) struct Joined {
    width: usize,
    /// Each pixel's alpha times its feather, summed over the tiles.
    alpha: Vec<f32>,
    /// Each pixel's feathers, summed over the tiles.
    weight: Vec<f32>,
}

impl Joined {
    /// Nothing joined yet over a `width` x `height` photograph.
    pub(crate) fn new(width: u32, height: u32) -> Self {
        let pixels = width as usize * height as usize;

        Self { width: width as usize, alpha: vec![0.0; pixels], weight: vec![0.0; pixels] }
    }

    /// Adds the graph's alpha for the tile at (`x0`, `y0`), `TILE` x `TILE` values of which only those inside the
    /// frame are read. Each is clamped to `[0, 1]`, since the graph's output slightly overshoots both.
    pub(crate) fn add(&mut self, tile: &[f32], x0: u32, y0: u32) {
        let side = TILE as usize;
        let height = self.alpha.len() / self.width;
        let (columns, rows) = ((self.width - x0 as usize).min(side), (height - y0 as usize).min(side));

        for row in 0..rows {
            let start = (y0 as usize + row) * self.width + x0 as usize;
            let (alpha, weight) = (&mut self.alpha[start..][..columns], &mut self.weight[start..][..columns]);
            let across = feather(row);

            for (column, (alpha, weight)) in alpha.iter_mut().zip(weight).enumerate() {
                let feather = across * feather(column);
                let value = tile[row * side + column];
                *alpha += if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) } * feather;
                *weight += feather;
            }
        }
    }

    /// The matte: the trimap's own value where it is known, and the tiles' feathered mean where it is not.
    pub(crate) fn finished(self, trimap: &Plane) -> Vec<f32> {
        trimap
            .as_slice()
            .iter()
            .zip(self.alpha.iter().zip(&self.weight))
            .map(
                |(&known, (&alpha, &weight))| {
                    if unknown(known) { (alpha / weight.max(1e-6)).clamp(0.0, 1.0) } else { known.round() }
                },
            )
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `width` x `height` trimap of sure background with unknown pixels at `points`.
    fn trimap(width: u32, height: u32, points: &[(u32, u32)]) -> Plane {
        let mut plane = Plane::new(width, height, 0.0);
        for &(x, y) in points {
            plane.as_mut_slice()[(y * width + x) as usize] = 0.5;
        }

        plane
    }

    #[test]
    fn only_the_tiles_holding_an_unknown_pixel_are_run() {
        // The spec's scenario: unknown pixels in one corner of a 6000 x 4000 photograph.
        let corner = trimap(6000, 4000, &[(5990, 3990), (5500, 3800)]);
        let found = tiles(&corner);

        assert!(!found.is_empty());
        assert!(found.iter().all(|&(x, y)| x >= 4000 && y >= 2600), "{found:?}");
        assert!(found.len() <= 4, "{found:?}");

        // Nothing unknown, nothing to run.
        assert!(tiles(&trimap(6000, 4000, &[])).is_empty());
    }

    #[test]
    fn the_tiles_cover_the_frame_ending_at_its_edges() {
        let everywhere = Plane::new(3000, 2000, 0.5);
        let found = tiles(&everywhere);

        // 0, 896, 1792 and the last moved back to 1976 across; 0, 896 and 976 down.
        let columns: Vec<u32> = found.iter().filter(|(_, y)| *y == 0).map(|(x, _)| *x).collect();
        let rows: Vec<u32> = found.iter().filter(|(x, _)| *x == 0).map(|(_, y)| *y).collect();
        assert_eq!(columns, vec![0, 896, 1792, 1976]);
        assert_eq!(rows, vec![0, 896, 976]);

        // A frame smaller than a tile is one tile, at the origin, running off it.
        assert_eq!(tiles(&Plane::new(600, 400, 0.5)), vec![(0, 0)]);
    }

    #[test]
    fn a_known_pixel_keeps_the_trimaps_value_exactly() {
        let mut map = Plane::new(300, 200, 0.0);
        for (index, value) in map.as_mut_slice().iter_mut().enumerate() {
            *value = [0.0, 0.5, 1.0][index % 3];
        }

        let mut joined = Joined::new(300, 200);
        joined.add(&vec![0.37; (TILE * TILE) as usize], 0, 0);
        let matte = joined.finished(&map);

        for (found, known) in matte.iter().zip(map.as_slice()) {
            if unknown(*known) {
                assert!((*found - 0.37).abs() < 1e-6, "{found}");
            } else {
                assert_eq!(*found, *known);
            }
        }
    }

    #[test]
    fn overlapping_tiles_cross_fade_with_no_seam() {
        // Two tiles across a 1920-pixel frame, one answering 0.2 everywhere and the other 0.8: in the overlap the
        // matte moves from one to the other smoothly and monotonically, and outside it each is its own tile's.
        let map = Plane::new(1920, 300, 0.5);
        let found = tiles(&map);
        assert_eq!(found, vec![(0, 0), (896, 0)]);

        let mut joined = Joined::new(1920, 300);
        joined.add(&vec![0.2; (TILE * TILE) as usize], 0, 0);
        joined.add(&vec![0.8; (TILE * TILE) as usize], 896, 0);
        let matte = joined.finished(&map);
        let row = &matte[150 * 1920..][..1920];

        assert!((row[500] - 0.2).abs() < 1e-6);
        assert!((row[1500] - 0.8).abs() < 1e-6);
        let overlap = &row[896..1024];
        assert!(overlap.windows(2).all(|pair| pair[1] >= pair[0]), "the cross-fade is not monotonic");
        let steepest = row.windows(2).map(|pair| (pair[1] - pair[0]).abs()).fold(0.0_f32, f32::max);
        assert!(steepest < 0.6 / 64.0, "a step of {steepest} between neighbours is a seam");
        assert!((overlap[64] - 0.5).abs() < 0.02, "{}", overlap[64]);
    }

    #[test]
    fn a_pixel_is_fed_as_the_processor_normalises_it_and_off_the_frame_is_zero() {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(600, 400, image::Rgb([255, 0, 51])));
        let map = Plane::new(600, 400, 0.5);
        let mut out = vec![9.0; PLANES * (TILE * TILE) as usize];

        tensor(&Sampler::new(&image), &map, 0, 0, &mut out);

        let plane = (TILE * TILE) as usize;
        let at = 10 * TILE as usize + 20;
        assert_eq!(out[at], 1.0);
        assert_eq!(out[plane + at], -1.0);
        assert!((out[2 * plane + at] - (51.0 / 255.0 * 2.0 - 1.0)).abs() < 1e-6, "{}", out[2 * plane + at]);
        assert_eq!(out[3 * plane + at], 0.5);

        // Past the photograph's 600 columns and 400 rows, all four planes are zeros.
        for channel in 0..PLANES {
            assert_eq!(out[channel * plane + 10 * TILE as usize + 700], 0.0);
            assert_eq!(out[channel * plane + 500 * TILE as usize + 20], 0.0);
        }
    }
}
