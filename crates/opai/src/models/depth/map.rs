//! What a depth run produces: the distance of every pixel from the camera, in metres, and the focal length the model
//! read off the photograph.

// Kept as metres, the unit every question asked of it is in, and stored as 16-bit disparity. The two meet in `new`,
// which holds the map already quantised: what a served run hands back is then the same value a computed one did,
// rather than one a few parts in a hundred thousand away, and a caller comparing the two finds them equal.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// The distance of every pixel of a photograph from the camera, in metres, at the depth model's resolution.
///
/// Row-major, `width * height` values, every one finite and positive: a pixel the model could not place (the sky,
/// mostly) holds the farthest distance in the map, so it is treated as the farthest thing in the photograph rather than
/// as a hole. The map covers the photograph's own frame, at the photograph's aspect, at most as large as the graph
/// that produced it and as the photograph itself — 1280 x 853 for a 6000 x 4000 one, and the photograph's own size
/// for one smaller than the graph.
///
/// Held exactly as the run cache keeps it — disparity quantised to 16 bits between the nearest and the farthest
/// distance — so a map served from the cache equals the map that was computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(into = "Compact", try_from = "Compact")]
pub struct DepthMap {
    /// Columns.
    width: u32,
    /// Rows.
    height: u32,
    /// The distances, row-major.
    metres: Arc<[f32]>,
    /// The 35 mm-equivalent focal length the model's geometry implies, where it could be recovered.
    focal_35mm: Option<f32>,
}

impl DepthMap {
    /// A map of `width` by `height` distances, quantised as the cache keeps them.
    ///
    /// `metres` must hold `width * height` finite, positive values; [`recover`](super::recover) guarantees that for
    /// a model's output. A focal length that is not finite and positive is absent.
    pub(crate) fn new(width: u32, height: u32, metres: &[f32], focal_35mm: Option<f32>) -> Self {
        debug_assert!(width > 0 && height > 0, "a map with no pixels");
        debug_assert_eq!(metres.len(), width as usize * height as usize, "a map whose plane does not fit its size");

        let scale = Quantisation::spanning(metres);
        let metres = metres.iter().map(|&distance| scale.decode(scale.encode(distance))).collect();

        Self { width, height, metres, focal_35mm: focal_35mm.filter(|focal| focal.is_finite() && *focal > 0.0) }
    }

    /// The map's width and height.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Every distance, in metres, row-major.
    pub fn metres(&self) -> &[f32] {
        &self.metres
    }

    /// The distance of the pixel at column `x` and row `y`, in metres.
    ///
    /// # Panics
    ///
    /// Where the pixel is outside the map.
    pub fn metres_at(&self, x: u32, y: u32) -> f32 {
        assert!(x < self.width && y < self.height, "({x}, {y}) is outside a {}x{} map", self.width, self.height);

        self.metres[y as usize * self.width as usize + x as usize]
    }

    /// The 35 mm-equivalent focal length the model's geometry implies, in millimetres, where it could be recovered.
    ///
    /// An estimate, and a rough one: the model reads a field of view off the photograph's content. A lens's own EXIF
    /// is the better answer wherever there is one.
    pub fn focal_35mm(&self) -> Option<f32> {
        self.focal_35mm
    }

    /// The map as disparity, `1 / metres`, row-major: what a blur's radius is linear in.
    pub fn disparity(&self) -> Vec<f32> {
        self.metres.iter().map(|distance| distance.recip()).collect()
    }

    /// The distance at a point given as fractions of the frame (`0.0` the left or top edge, `1.0` the right or bottom),
    /// in metres: the median over a small window around it.
    ///
    /// The median rather than the pixel, so a point placed on an edge between a subject and what is behind it answers
    /// one of the two rather than a value between them, and a click a pixel off does not change the answer. The window
    /// is `2r + 1` pixels square, `r` half a percent of the short side and at least 4, clipped at the map's edges.
    pub fn focus_metres(&self, x: f64, y: f64) -> f32 {
        let (width, height) = (self.width as usize, self.height as usize);
        let radius = ((width.min(height) as f64 * FOCUS_WINDOW).round() as usize).max(MIN_FOCUS_RADIUS);

        // The pixel the point falls in, clamped so a fraction of exactly 1.0, or a stray one outside the frame, still
        // names a pixel of the map.
        let centre = |fraction: f64, size: usize| ((fraction.clamp(0.0, 1.0) * size as f64) as usize).min(size - 1);
        let (cx, cy) = (centre(x, width), centre(y, height));

        let mut window: Vec<f32> = (cy.saturating_sub(radius)..=(cy + radius).min(height - 1))
            .flat_map(|row| {
                let start = row * width;
                self.metres[start + cx.saturating_sub(radius)..=start + (cx + radius).min(width - 1)]
                    .iter()
                    .copied()
            })
            .collect();

        let middle = window.len() / 2;
        *window.select_nth_unstable_by(middle, f32::total_cmp).1
    }

    /// A 64-bit digest of the map, which two maps share exactly when they are equal.
    ///
    /// Stable across processes and releases (XXH3 over the dimensions, the focal length and every distance's bits),
    /// so what is keyed on it can be persisted.
    pub fn signature(&self) -> u64 {
        let mut bytes = Vec::with_capacity(12 + self.metres.len() * 4);
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        bytes.extend_from_slice(&self.focal_35mm.map_or(u32::MAX, f32::to_bits).to_le_bytes());
        for distance in self.metres.iter() {
            bytes.extend_from_slice(&distance.to_bits().to_le_bytes());
        }

        // `xxh3_bytes` renders the 64 bits as 16 hex digits, which is always a `u64` to read back.
        u64::from_str_radix(&rust_sak::crypto::xxh3_bytes(&bytes), 16).unwrap_or_default()
    }
}

/// The focus window's radius as a fraction of the map's short side.
const FOCUS_WINDOW: f64 = 0.005;

/// The focus window's smallest radius, in pixels.
const MIN_FOCUS_RADIUS: usize = 4;

/// The largest quantised disparity: 16 bits.
const LEVELS: f64 = 65_535.0;

/// The linear quantisation of disparity between a map's farthest distance (level 0) and its nearest (level 65 535).
///
/// Linear in disparity rather than in metres because that is where the precision is wanted: a blur's radius is linear
/// in disparity, so equal steps here are equal steps of blur, and a step is far finer than the model's accuracy.
#[derive(Debug, Clone, Copy)]
struct Quantisation {
    /// The disparity of level 0, the farthest distance.
    far: f64,
    /// The disparity one level spans; zero for a map at one distance.
    step: f64,
}

impl Quantisation {
    /// The quantisation spanning `metres`' nearest and farthest distances.
    fn spanning(metres: &[f32]) -> Self {
        let (near, far) = bounds(metres);

        Self::between(near, far)
    }

    /// The quantisation between `near` and `far` metres.
    fn between(near: f32, far: f32) -> Self {
        let (near, far) = (f64::from(near).recip(), f64::from(far).recip());

        Self { far, step: (near - far) / LEVELS }
    }

    /// The level `distance` is stored as.
    fn encode(self, distance: f32) -> u16 {
        if self.step == 0.0 {
            return 0;
        }

        (((f64::from(distance).recip() - self.far) / self.step).round().clamp(0.0, LEVELS)) as u16
    }

    /// The distance `level` stands for.
    fn decode(self, level: u16) -> f32 {
        (self.far + f64::from(level) * self.step).recip() as f32
    }
}

/// The nearest and the farthest of `metres`, or one metre for both where there is nothing.
fn bounds(metres: &[f32]) -> (f32, f32) {
    let near = metres.iter().copied().fold(f32::INFINITY, f32::min);
    let far = metres.iter().copied().fold(0.0_f32, f32::max);

    if near.is_finite() && near > 0.0 { (near, far) } else { (1.0, 1.0) }
}

// The JSON the run cache keeps: a million `u16`s is 4 to 6 MB of text, where the same distances as `f32`s would print
// about three times longer.
/// A [`DepthMap`] as it is serialized: 16-bit disparity between the two distances that bound it.
#[derive(Serialize, Deserialize)]
struct Compact {
    width: u32,
    height: u32,
    /// The nearest distance in the map, in metres: level 65 535.
    near: f32,
    /// The farthest distance in the map, in metres: level 0.
    far: f32,
    focal_35mm: Option<f32>,
    /// Every pixel's level, row-major.
    disparity: Vec<u16>,
}

impl From<DepthMap> for Compact {
    fn from(map: DepthMap) -> Self {
        let (near, far) = bounds(&map.metres);
        let scale = Quantisation::between(near, far);

        Self {
            width: map.width,
            height: map.height,
            near,
            far,
            focal_35mm: map.focal_35mm,
            disparity: map.metres.iter().map(|&distance| scale.encode(distance)).collect(),
        }
    }
}

/// Why a serialized map was refused.
#[derive(Debug, thiserror::Error)]
#[error("a stored depth map is malformed: {0}")]
struct Malformed(&'static str);

impl TryFrom<Compact> for DepthMap {
    type Error = Malformed;

    fn try_from(compact: Compact) -> Result<Self, Self::Error> {
        // Refused here because a map without pixels has nothing for `focus_metres` to answer, and it would panic.
        if compact.width == 0 || compact.height == 0 {
            return Err(Malformed("it has no pixels"));
        }

        if compact.disparity.len() != compact.width as usize * compact.height as usize {
            return Err(Malformed("its plane does not fit its size"));
        }

        if !(compact.near.is_finite() && compact.far.is_finite() && compact.near > 0.0 && compact.far >= compact.near) {
            return Err(Malformed("its bounds are not two positive distances, nearest first"));
        }

        let scale = Quantisation::between(compact.near, compact.far);

        Ok(Self {
            width: compact.width,
            height: compact.height,
            metres: compact.disparity.iter().map(|&level| scale.decode(level)).collect(),
            focal_35mm: compact.focal_35mm,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map whose distance grows with the column, from one metre to about forty, with a nearer square in it.
    fn ramp(width: u32, height: u32) -> DepthMap {
        let metres: Vec<f32> = (0..height)
            .flat_map(|y| {
                (0..width).map(move |x| {
                    let square = (10..20).contains(&x) && (10..20).contains(&y);
                    if square { 0.5 } else { 1.0 + 40.0 * x as f32 / width as f32 }
                })
            })
            .collect();

        DepthMap::new(width, height, &metres, Some(36.0))
    }

    #[test]
    fn a_map_round_trips_through_its_serialized_form_unchanged() {
        // Unchanged rather than within quantisation, because the map is held quantised: the cache's copy and the run's
        // are the same value.
        let map = ramp(97, 61);
        let json = serde_json::to_string(&map).expect("a map serializes");
        let back: DepthMap = serde_json::from_str(&json).expect("a map deserializes");

        assert_eq!(back, map);
        assert_eq!(back.signature(), map.signature());
    }

    #[test]
    fn quantisation_keeps_a_distance_within_half_a_level_of_disparity() {
        let exact: Vec<f32> = (0..1000).map(|i| 0.3 + i as f32 * 0.25).collect();
        let map = DepthMap::new(1000, 1, &exact, None);
        let level = (1.0 / 0.3 - 1.0 / exact[999]) / 65_535.0;

        for (stored, exact) in map.metres().iter().zip(&exact) {
            let error = (stored.recip() - exact.recip()).abs();
            assert!(error <= level * 0.501, "{exact} was stored as {stored}, {error} from it in disparity");
        }

        assert_eq!(map.metres()[0], 0.3, "the nearest distance moved");
        assert_eq!(map.metres()[999], exact[999], "the farthest distance moved");
    }

    #[test]
    fn the_serialized_form_is_sixteen_bit_disparity_with_its_bounds() {
        let map = DepthMap::new(3, 1, &[1.0, 2.0, 4.0], Some(24.0));
        let json = serde_json::to_string(&map).expect("a map serializes");

        assert_eq!(
            json,
            r#"{"width":3,"height":1,"near":1.0,"far":4.0,"focal_35mm":24.0,"disparity":[65535,21845,0]}"#
        );
    }

    #[test]
    fn a_map_at_one_distance_survives_quantisation() {
        let map = DepthMap::new(2, 2, &[3.0; 4], None);
        let back: DepthMap = serde_json::from_str(&serde_json::to_string(&map).unwrap()).unwrap();

        assert_eq!(back.metres(), &[3.0; 4]);
    }

    #[test]
    fn a_malformed_stored_map_is_refused_rather_than_read() {
        for json in [
            r#"{"width":2,"height":2,"near":1.0,"far":2.0,"focal_35mm":null,"disparity":[0,1,2]}"#,
            r#"{"width":1,"height":1,"near":0.0,"far":2.0,"focal_35mm":null,"disparity":[0]}"#,
            r#"{"width":1,"height":1,"near":3.0,"far":2.0,"focal_35mm":null,"disparity":[0]}"#,
            r#"{"width":0,"height":0,"near":1.0,"far":2.0,"focal_35mm":null,"disparity":[]}"#,
            r#"{"width":0,"height":3,"near":1.0,"far":2.0,"focal_35mm":null,"disparity":[]}"#,
            r#"{"width":3,"height":0,"near":1.0,"far":2.0,"focal_35mm":null,"disparity":[]}"#,
        ] {
            assert!(serde_json::from_str::<DepthMap>(json).is_err(), "{json} was read as a map");
        }
    }

    #[test]
    fn the_focus_distance_is_the_median_of_its_window() {
        // A step from one metre to five at column 50: a point well inside either half answers that half, and a
        // point on the step answers one of the two rather than something between them.
        let metres: Vec<f32> = (0..100 * 100).map(|i| if i % 100 < 50 { 1.0 } else { 5.0 }).collect();
        let map = DepthMap::new(100, 100, &metres, None);

        assert_eq!(map.focus_metres(0.2, 0.5), 1.0);
        assert_eq!(map.focus_metres(0.8, 0.5), 5.0);
        assert!([1.0, 5.0].contains(&map.focus_metres(0.5, 0.5)));
    }

    #[test]
    fn the_focus_window_is_clipped_at_an_edge_and_a_corner() {
        let map = ramp(200, 100);

        // The corners and the edges name pixels of the map, and the window keeps to it rather than panicking.
        for (x, y) in [(0.0, 0.0), (1.0, 1.0), (0.0, 1.0), (1.0, 0.0), (0.5, 0.0), (1.0, 0.5), (1.5, -0.5)] {
            let found = map.focus_metres(x, y);
            assert!(found.is_finite() && found > 0.0, "({x}, {y}) answered {found}");
        }

        // The window at the left edge holds columns 0 to 4 only, all near one metre.
        assert!(map.focus_metres(0.0, 0.5) < 2.0);
        // The square at (10..20, 10..20) is nearer than everything around it.
        assert_eq!(map.focus_metres(0.075, 0.15), 0.5);
    }

    #[test]
    fn the_signature_changes_with_one_pixel_and_with_the_focal_length() {
        let map = ramp(64, 48);
        let mut metres = map.metres().to_vec();
        metres[1000] *= 1.5;
        let changed = DepthMap::new(64, 48, &metres, map.focal_35mm());

        assert_ne!(changed.signature(), map.signature());
        assert_ne!(DepthMap::new(64, 48, map.metres(), None).signature(), map.signature());
        assert_eq!(DepthMap::new(64, 48, map.metres(), map.focal_35mm()).signature(), map.signature());
    }

    #[test]
    fn disparity_is_the_reciprocal_of_distance() {
        let map = DepthMap::new(2, 1, &[2.0, 4.0], None);

        assert_eq!(map.disparity(), vec![0.5, 0.25]);
        assert_eq!(map.metres_at(1, 0), 4.0);
        assert_eq!(map.dimensions(), (2, 1));
    }
}
