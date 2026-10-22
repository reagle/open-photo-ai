//! A photograph's colour profile, end to end: loaded, cropped, enhanced and exported in every format that can hold it.

use super::*;
use crate::export::{WriteOptions, write};
use crate::image::ImageFormat;
use crate::image::test_support::{DISPLAY_P3, written_p3};

#[tokio::test]
async fn a_display_p3_photograph_is_exported_as_display_p3_after_a_crop_and_an_enhancement() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("p3.jpg");
    std::fs::write(&path, written_p3(ImageFormat::Jpeg)).expect("writable");

    let loaded = crate::image::load_blocking(&path).expect("decodable");
    assert_eq!(loaded.metadata().icc_profile.as_deref(), Some(DISPLAY_P3));

    // A crop as the GUI makes one: new pixels and a new identity, derived from the source.
    let framed = loaded.pixels().crop_imm(4, 2, 24, 16);
    let cropped = loaded.derive(framed, format!("{}-cropped", loaded.identity()));

    // A one-step chain on the fake backend: no model file is read.
    let backend = Fake::new();
    let enhanced = process(&backend, None, &cropped, &[kyoto(2.0)], None).await.expect("ran").picture;
    assert_eq!(enhanced.dimensions(), (48, 32), "the chain did not run on the crop");

    for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::Tiff, ImageFormat::WebP] {
        let destination = dir.path().join(format!("out.{}", format.extension()));
        let written = write(&enhanced, &destination, format, WriteOptions::default()).await.expect("written");

        let bytes = std::fs::read(&written.path).expect("readable");
        assert_eq!(
            rust_sak::image::icc_profile_bytes(&bytes).expect("a readable header").as_deref(),
            Some(DISPLAY_P3),
            "{format:?} lost the profile"
        );
    }
}
