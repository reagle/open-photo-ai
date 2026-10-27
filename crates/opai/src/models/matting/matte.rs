//! What a matting run produces: how much of each pixel of a photograph the subject covers.

// Held as 16-bit levels, the form the run cache keeps, for the depth map's reason: a matte served from the cache is
// then the same value a computed one was, not one a level away, and a lens blur keyed on its signature is served the
// render it made.

use std::sync::Arc;

use imaging::plane::Plane;
use serde::{Deserialize, Serialize};

use crate::models::handle::digest;

/// How much of each pixel of a photograph a subject covers, from 0 (none of it: background) to 1 (all of it), at the
/// photograph's own size.
///
/// Row-major, `width * height` values, held as 16-bit levels exactly as the run cache keeps them, so a matte served
/// from the cache equals the matte that was computed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "Compact", try_from = "Compact")]
pub struct Matte {
    /// Columns.
    width: u32,
    /// Rows.
    height: u32,
    /// The coverage of every pixel, row-major, 0 for none and 65 535 for all of it.
    alpha: Arc<[u16]>,
}

/// The level that stands for the whole pixel.
const FULL: f32 = 65_535.0;

impl Matte {
    /// A matte of `width` by `height` coverages, each clamped to `[0, 1]` (one that is not a number is 0) and quantised
    /// as the cache keeps them.
    pub(crate) fn new(width: u32, height: u32, alpha: &[f32]) -> Self {
        debug_assert_eq!(alpha.len(), width as usize * height as usize, "a matte whose plane does not fit its size");

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "rounded and clamped to 16 bits"
        )]
        let alpha = alpha.iter().map(|&value| (value.clamp(0.0, 1.0) * FULL).round() as u16).collect();

        Self { width, height, alpha }
    }

    /// The matte's width and height.
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The coverage of the pixel at column `x` and row `y`, from 0 to 1.
    ///
    /// # Panics
    ///
    /// Where the pixel is outside the matte.
    #[cfg(test)]
    pub(crate) fn alpha_at(&self, x: u32, y: u32) -> f32 {
        assert!(
            x < self.width && y < self.height,
            "({x}, {y}) is outside a {}x{} matte",
            self.width,
            self.height
        );

        f32::from(self.alpha[y as usize * self.width as usize + x as usize]) / FULL
    }

    /// Every pixel's coverage, from 0 to 1, row-major.
    pub(crate) fn coverage(&self) -> Vec<f32> {
        self.alpha.iter().map(|&level| f32::from(level) / FULL).collect()
    }

    /// The coverage as a plane, for the render.
    pub(crate) fn plane(&self) -> Plane {
        Plane::from_vec(self.width, self.height, self.coverage()).expect("a matte holds one coverage a pixel")
    }

    /// A 64-bit digest of the matte, which two mattes share exactly when they are equal.
    ///
    /// Stable across processes and releases (XXH3 over the dimensions and every level, a chunk at a time), so what is
    /// keyed on it can be persisted.
    pub(crate) fn signature(&self) -> u64 {
        let size = [self.width.to_le_bytes(), self.height.to_le_bytes()].concat();

        digest(&size, self.alpha.iter().map(|level| level.to_le_bytes()))
    }
}

// Run-length rather than a level a pixel: all but the trimap's unknown band, a tenth of the photograph or less, is 0 or
// 65 535 in long runs, so a 24-megapixel matte is a few megabytes of JSON rather than seventy, and is read back in tens
// of milliseconds.
/// A [`Matte`] as it is serialized: its levels, run-length coded.
#[derive(Serialize, Deserialize)]
struct Compact {
    width: u32,
    height: u32,
    /// The levels, row-major, each repeated pixel run of one level written once.
    levels: Vec<u16>,
    /// How many pixels each of `levels` stands for, in order.
    runs: Vec<u32>,
}

impl From<Matte> for Compact {
    fn from(matte: Matte) -> Self {
        let (mut levels, mut runs): (Vec<u16>, Vec<u32>) = (Vec::new(), Vec::new());

        for &level in matte.alpha.iter() {
            match (levels.last(), runs.last_mut()) {
                (Some(&last), Some(run)) if last == level && *run < u32::MAX => *run += 1,
                _ => {
                    levels.push(level);
                    runs.push(1);
                }
            }
        }

        Self { width: matte.width, height: matte.height, levels, runs }
    }
}

/// Why a serialized matte was refused.
#[derive(Debug, thiserror::Error)]
#[error("a stored matte is malformed: {0}")]
struct Malformed(&'static str);

impl TryFrom<Compact> for Matte {
    type Error = Malformed;

    fn try_from(compact: Compact) -> Result<Self, Self::Error> {
        if compact.width == 0 || compact.height == 0 {
            return Err(Malformed("it has no pixels"));
        }

        if compact.levels.len() != compact.runs.len() {
            return Err(Malformed("its levels and its runs differ in number"));
        }

        let pixels = compact.width as usize * compact.height as usize;
        let total = compact.runs.iter().map(|&run| run as usize).sum::<usize>();
        if total != pixels {
            return Err(Malformed("its runs do not cover its size"));
        }

        let mut alpha = Vec::with_capacity(pixels);
        for (&level, &run) in compact.levels.iter().zip(&compact.runs) {
            alpha.resize(alpha.len() + run as usize, level);
        }

        Ok(Self { width: compact.width, height: compact.height, alpha: alpha.into() })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A `width` x `height` matte: background on the left, the subject on the right, and a soft band between.
    pub(crate) fn soft_edge(width: u32, height: u32) -> Matte {
        let alpha: Vec<f32> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| ((x as f32 - width as f32 / 2.0 + (y % 3) as f32) / 6.0 + 0.5).clamp(0.0, 1.0))
            .collect();

        Matte::new(width, height, &alpha)
    }

    #[test]
    fn a_matte_round_trips_through_its_serialized_form_unchanged() {
        let matte = soft_edge(97, 61);
        let json = serde_json::to_string(&matte).expect("a matte serializes");
        let back: Matte = serde_json::from_str(&json).expect("a matte deserializes");

        assert_eq!(back, matte);
        assert_eq!(back.signature(), matte.signature());
    }

    #[test]
    fn the_serialized_form_runs_the_levels_together() {
        let matte = Matte::new(4, 2, &[0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 0.0]);
        let json = serde_json::to_string(&matte).expect("a matte serializes");

        assert_eq!(json, r#"{"width":4,"height":2,"levels":[0,32768,65535,0],"runs":[3,1,3,1]}"#);
    }

    #[test]
    fn coverage_is_quantised_to_sixteen_bits_and_clamped() {
        let matte = Matte::new(4, 1, &[-0.2, 0.25, 1.3, f32::NAN]);

        assert_eq!(matte.alpha_at(0, 0), 0.0);
        assert!((matte.alpha_at(1, 0) - 0.25).abs() <= 0.5 / FULL);
        assert_eq!(matte.alpha_at(2, 0), 1.0);
        assert_eq!(matte.alpha_at(3, 0), 0.0, "a coverage that is not a number is none");
        assert_eq!(matte.coverage().len(), 4);
        assert_eq!(matte.plane().get(2, 0), 1.0);
    }

    #[test]
    fn a_malformed_stored_matte_is_refused_rather_than_read() {
        for json in [
            r#"{"width":2,"height":2,"levels":[0],"runs":[3]}"#,
            r#"{"width":2,"height":2,"levels":[0,1],"runs":[4]}"#,
            r#"{"width":2,"height":2,"levels":[0],"runs":[5]}"#,
            r#"{"width":0,"height":2,"levels":[],"runs":[]}"#,
        ] {
            assert!(serde_json::from_str::<Matte>(json).is_err(), "{json} was read as a matte");
        }
    }

    #[test]
    fn the_signature_changes_with_one_pixel() {
        let matte = soft_edge(64, 48);
        let mut alpha = matte.coverage();
        alpha[1000] = 1.0 - alpha[1000];

        assert_ne!(Matte::new(64, 48, &alpha).signature(), matte.signature());
        assert_eq!(Matte::new(64, 48, &matte.coverage()).signature(), matte.signature());
    }

    /// The cache's read of a 24-megapixel matte whose unknown band is a tenth of it, which the design's rule wants
    /// under 100 ms: `cargo test -p opai --release -- --ignored a_large_matte_is_read_back_quickly --nocapture`.
    #[test]
    #[ignore = "a timing, run by hand in release"]
    fn a_large_matte_is_read_back_quickly() {
        let (width, height) = (6000_u32, 4000_u32);
        // A soft band 300 px wide winding down the middle, with background and subject either side.
        let alpha: Vec<f32> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| {
                let edge = 3000.0 + 400.0 * (y as f32 / 300.0).sin();
                ((x as f32 - edge) / 300.0 + 0.5).clamp(0.0, 1.0)
            })
            .collect();
        let matte = Matte::new(width, height, &alpha);

        let started = std::time::Instant::now();
        let json = serde_json::to_vec(&matte).expect("a matte serializes");
        let written = started.elapsed();
        let started = std::time::Instant::now();
        let back: Matte = serde_json::from_slice(&json).expect("a matte deserializes");
        let read = started.elapsed();

        assert_eq!(back, matte);
        println!("24 MP matte: {} bytes of JSON, written in {written:?}, read in {read:?}", json.len());
    }
}
