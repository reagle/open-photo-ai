//! The image the application is working on: its pixels, where they came from, what identifies them, and what the
//! file said about them beside the pixels.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ::image::DynamicImage;
use rust_sak::image::Metadata;

use super::fidelity::Fidelity;

/// A decoded image together with where its pixels came from and the identity that describes them.
///
/// This is the one description of "an image the application is working on" that inference, the per-operation cache
/// and the export queue all work from.
///
/// # Identity
///
/// [`identity`](Self::identity) is a lowercase hexadecimal string, and what holds of every picture is that it
/// describes **these** pixels: it is never carried onto different ones and is never stale. The fields are private
/// and the pixels cannot be altered in place. It is what the per-operation cache is keyed on.
///
/// Where the value comes from depends on who produced the picture:
///
/// - **Loaded** through [`load`](fn@super::load), it is the XXH3-64 of the **whole** of the file's bytes — the same
///   value [`rust_sak::crypto::xxh3_file`] produces for that file. That equality is the loader's guarantee and the
///   point of it: a file listing can hash a path without decoding it, and later recognise the loaded image as the
///   same one.
/// - **Computed** from other pixels, it is derived from the identity of what they were computed from together with
///   what was done to them. No file on disk hashes to it, and none needs to.
///
/// [`Picture::new`] is the one place the invariant can be broken, because it is the one place pixels and an identity
/// meet without having been read from the same file; see its documentation.
///
/// # Metadata
///
/// [`metadata`](Self::metadata) is what the source file carried beside its pixels — today its ICC colour profile,
/// byte for byte — and what an export embeds again. It is a label on the pixels, not part of them: nothing here
/// converts a colour, and the identity does not cover it separately (a loaded identity already hashes the whole file,
/// profile included). **Every picture derived from another keeps it**, through [`derive`](Self::derive), which is
/// why a crop or an enhancement of a Display P3 photograph is still written as Display P3.
///
/// # Fidelity
///
/// [`fidelity`](Self::fidelity) is what the source file held that a written file can lose — its depth, HDR, a colour
/// description not carried as a profile — recorded by [`load`](fn@super::load) and kept by [`derive`](Self::derive)
/// like the metadata, so an export compares what it writes against the file the user chose; see
/// [`export::losses`](crate::export::losses).
///
/// # Cost of cloning
///
/// Cloning shares the pixels and the metadata rather than copying them, which after an 8x upscale is the difference
/// between a refcount bump and several hundred megabytes.
#[derive(Debug, Clone)]
pub struct Picture {
    /// Where the pixels came from, never where they are going.
    path: PathBuf,
    // Behind an `Arc` so the pixels cannot be altered in place while `identity` goes on describing what they used to
    // be — the failure the reference's type warns about and cannot prevent. Sharing is also what lets the asynchronous
    // operations exist at all: `spawn_blocking` requires `'static`, so a borrow of the caller's image cannot cross onto
    // a blocking thread, and a copy would defeat the purpose of going there.
    /// The decoded pixels, shared rather than owned.
    pixels: Arc<DynamicImage>,
    /// The identity of these pixels, lowercase hexadecimal. See the type's documentation for where it comes from.
    identity: String,
    /// What the source file carried beside the pixels, shared for the same reason the pixels are: the GUI clones
    /// pictures freely, and a large camera profile is hundreds of kilobytes.
    metadata: Arc<Metadata>,
    /// What the source file held that a written file can lose, shared like the metadata.
    fidelity: Arc<Fidelity>,
}

impl Picture {
    /// Builds an image from pixels and the identity that describes them.
    ///
    /// **The caller is responsible for the pairing.** Nothing here can check that `identity` describes `pixels` —
    /// a hash is opaque, and when the pixels were decoded the bytes behind them are gone. Loading a file goes
    /// through [`load`](fn@super::load), which derives both from the same bytes; this constructor exists for the cases
    /// that cannot.
    ///
    /// The picture carries **no metadata**, and its fidelity is the pixels' own (their depth, no HDR). Pixels computed
    /// from another picture are built with [`derive`](Self::derive) instead, which keeps that picture's.
    pub fn new(path: impl Into<PathBuf>, pixels: impl Into<Arc<DynamicImage>>, identity: impl Into<String>) -> Self {
        let pixels = pixels.into();
        let fidelity = Arc::new(Fidelity::of_pixels(&pixels));
        Self { path: path.into(), pixels, identity: identity.into(), metadata: Arc::default(), fidelity }
    }

    /// This picture with `metadata` in place of what it carried — how [`load`](fn@super::load) attaches what the file
    /// held beside its pixels.
    #[must_use]
    pub fn with_metadata(self, metadata: Metadata) -> Self {
        Self { metadata: Arc::new(metadata), ..self }
    }

    /// This picture with `fidelity` in place of what it recorded — how [`load`](fn@super::load) attaches what the file
    /// held that an export can lose.
    #[must_use]
    pub fn with_fidelity(self, fidelity: Fidelity) -> Self {
        Self { fidelity: Arc::new(fidelity), ..self }
    }

    /// A picture computed from this one: `pixels` under `identity`, keeping this picture's path, metadata and fidelity.
    ///
    /// **The one way to build a derived picture**, so that a crop, an enhancement or a cache hit cannot forget the
    /// source's colour profile. The pairing of `pixels` and `identity` is the caller's, exactly as for
    /// [`new`](Self::new); the metadata is shared, not copied.
    pub fn derive(&self, pixels: impl Into<Arc<DynamicImage>>, identity: impl Into<String>) -> Self {
        Self {
            path: self.path.clone(),
            pixels: pixels.into(),
            identity: identity.into(),
            metadata: Arc::clone(&self.metadata),
            fidelity: Arc::clone(&self.fidelity),
        }
    }

    /// Where the pixels were read from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The decoded pixels.
    pub fn pixels(&self) -> &DynamicImage {
        &self.pixels
    }

    /// The pixels as a shared handle, for a caller that needs to keep them beyond this image's lifetime.
    ///
    /// Cheap: a refcount bump, not a copy. This is how an operation hands the same pixels to a blocking thread
    /// without the caller giving up its own image.
    pub fn shared_pixels(&self) -> Arc<DynamicImage> {
        Arc::clone(&self.pixels)
    }

    /// What identifies these pixels, lowercase hexadecimal. See the type's documentation for where it comes from.
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// What the source file carried beside the pixels; see the type's documentation.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// The metadata as a shared handle, for the asynchronous [`save`](fn@super::save) and
    /// [`encode`](fn@super::encode). A refcount bump, not a copy.
    pub fn shared_metadata(&self) -> Arc<Metadata> {
        Arc::clone(&self.metadata)
    }

    /// What the source file held that a written file can lose; see the type's documentation.
    pub fn fidelity(&self) -> &Fidelity {
        &self.fidelity
    }

    /// The image's width and height, in pixels.
    pub fn dimensions(&self) -> (u32, u32) {
        // Derived from the pixels on every call rather than stored. A stored pair would be a second answer to a
        // question the pixels already answer, and a second answer is a thing that can disagree — the same class of
        // defect as a stale identity, which this type otherwise spends its shape preventing.
        (self.pixels.width(), self.pixels.height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::RgbaImage;

    /// An image of a known size, built rather than decoded so the test does not depend on a fixture.
    fn image_of(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::new(width, height))
    }

    #[test]
    fn the_reported_dimensions_are_the_pixels_own() {
        let source = Picture::new("/pictures/wide.png", image_of(640, 480), "0123456789abcdef");

        assert_eq!(source.dimensions(), (640, 480));
        // Derived rather than stored, so the two cannot disagree: the same numbers the pixels themselves report.
        assert_eq!(source.dimensions(), (source.pixels().width(), source.pixels().height()));
    }

    #[test]
    fn the_path_and_the_identity_are_reported_back_as_given() {
        let source = Picture::new("/pictures/holiday.jpg", image_of(2, 2), "cafebabecafebabe");

        assert_eq!(source.path(), Path::new("/pictures/holiday.jpg"));
        assert_eq!(source.identity(), "cafebabecafebabe");
    }

    #[test]
    fn cloning_shares_the_pixel_buffer_rather_than_copying_it() {
        let source = Picture::new("/pictures/large.tif", image_of(64, 64), "deadbeefdeadbeef");
        let copy = source.clone();

        assert!(
            std::ptr::eq(source.pixels(), copy.pixels()),
            "the clone decoded a second buffer instead of sharing the first"
        );
        assert_eq!(Arc::strong_count(&source.shared_pixels()), 3, "the handle is not the one the clone shares");
    }

    /// Metadata carrying `profile` as its ICC profile.
    fn profiled(profile: &[u8]) -> Metadata {
        let mut metadata = Metadata::default();
        metadata.icc_profile = Some(profile.to_vec());
        metadata
    }

    #[test]
    fn a_new_picture_carries_no_metadata() {
        let source = Picture::new("/pictures/holiday.jpg", image_of(2, 2), "cafebabecafebabe");

        assert_eq!(source.metadata(), &Metadata::default());
        assert_eq!(source.metadata().icc_profile, None);
    }

    #[test]
    fn a_derived_picture_keeps_the_path_and_the_profile_and_takes_its_own_pixels_and_identity() {
        let source = Picture::new("/pictures/p3.jpg", image_of(8, 8), "cafebabecafebabe")
            .with_metadata(profiled(b"a display p3 profile"));

        let derived = source.derive(image_of(4, 2), "0123456789abcdef");

        assert_eq!(derived.path(), source.path());
        assert_eq!(derived.identity(), "0123456789abcdef");
        assert_eq!(derived.dimensions(), (4, 2));
        assert_eq!(derived.metadata().icc_profile.as_deref(), Some(&b"a display p3 profile"[..]));
        // Shared rather than copied.
        assert!(Arc::ptr_eq(&source.shared_metadata(), &derived.shared_metadata()));
        // And a derivation of a derivation still has it.
        assert_eq!(derived.derive(image_of(1, 1), "f00df00df00df00d").metadata(), source.metadata());
    }

    #[test]
    fn a_new_picture_records_its_own_pixels_and_a_derived_one_keeps_its_sources_fidelity() {
        use super::super::fidelity::Depth;
        use rust_sak::image::Hdr;

        let source = Picture::new("/pictures/hdr.jpg", DynamicImage::new_rgb16(4, 4), "cafebabecafebabe");
        assert_eq!(source.fidelity(), &Fidelity::of_colour(::image::ColorType::Rgb16));

        let mut fidelity = Fidelity::of_colour(::image::ColorType::Rgb8);
        fidelity.hdr = Some(Hdr::GainMap);
        let source = source.with_fidelity(fidelity.clone());
        // An enhancement that comes back at another depth still answers for the source.
        let derived = source.derive(image_of(8, 8), "0123456789abcdef").derive(image_of(2, 2), "f00df00df00df00d");

        assert_eq!(derived.fidelity(), &fidelity);
        assert_eq!(derived.fidelity().depth, Depth { bits: 8, float: false });
    }

    #[test]
    fn cloning_shares_the_metadata_rather_than_copying_it() {
        let source =
            Picture::new("/pictures/p3.jpg", image_of(2, 2), "cafebabecafebabe").with_metadata(profiled(b"profile"));
        let copy = source.clone();

        assert!(Arc::ptr_eq(&source.shared_metadata(), &copy.shared_metadata()));
    }

    #[test]
    fn the_shared_handle_outlives_the_image_it_came_from() {
        // What an operation does: take the pixels onto a blocking thread, where nothing borrowed can go, while the
        // caller keeps — or in this case drops — its own image.
        let pixels = {
            let source = Picture::new("/pictures/small.png", image_of(8, 8), "f00df00df00df00d");
            source.shared_pixels()
        };

        assert_eq!((pixels.width(), pixels.height()), (8, 8));
    }
}
