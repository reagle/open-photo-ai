//! An Ultra HDR photograph's gain map, end to end: loaded, cropped, enhanced, served from the run cache and exported as
//! an Ultra HDR JPEG whose gain map is in register with the written image.

use std::sync::Arc;

use ::image::DynamicImage;
use rust_sak::image::{decode_bytes, metadata_bytes};

use super::*;
use crate::export::{WriteOptions, write};
use crate::image::ImageFormat;
use crate::image::test_support::written_ultra_hdr_of;

/// The first channel of `image` at `(x, y)`.
fn first(image: &DynamicImage, x: u32, y: u32) -> i32 {
    i32::from(image.to_rgb8().get_pixel(x, y).0[0])
}

/// The MP Entry of the gain map in the JPEG `file`: its size and its offset in the file.
fn gain_map_entry(file: &[u8]) -> (usize, usize) {
    let at = file.windows(4).position(|w| w == b"MPF\0").expect("an MPF index") + 4;
    let u32_at = |offset: usize| u32::from_be_bytes(file[offset..offset + 4].try_into().expect("four")) as usize;
    (u32_at(at + 50 + 20), at + u32_at(at + 50 + 24))
}

#[tokio::test]
async fn a_cropped_and_upscaled_ultra_hdr_photograph_keeps_its_gain_map_in_register_fresh_or_cached() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("hawk.jpg");
    // A half-resolution gain map, so the crop has to be re-expressed at the gain map's own scale.
    std::fs::write(&path, written_ultra_hdr_of((40, 32), (20, 16), 3)).expect("writable");
    let loaded = crate::image::load_blocking(&path).expect("decodable");
    let source_map = loaded.gain_map().expect("carried").clone();
    assert_eq!(source_map.scale(), (0.5, 0.5));

    // A crop as the GUI makes one: the same rectangle cut from the pixels, and from the gain map at its scale.
    let cropped = loaded
        .derive(loaded.pixels().crop_imm(8, 4, 24, 16), format!("{}-cropped", loaded.identity()))
        .with_gain_map(Some(source_map.reframed(|map, scale| {
            let at = |value: u32| (f64::from(value) * scale).round() as u32;
            map.crop_imm(at(8), at(4), at(24), at(16))
        })));
    let cropped_map = cropped.shared_gain_map().expect("reframed");
    assert_eq!((cropped_map.pixels().width(), cropped_map.pixels().height()), (12, 8));

    // A 2x upscale on the fake backend, then the same chain again, served from the cache.
    let backend = Fake::new();
    let cache = store();
    let fresh = process(&backend, Some(&cache), &cropped, &[kyoto(2.0)], None).await.expect("ran").picture;
    let ran = backend.log().ran.len();
    let cached = process(&backend, Some(&cache), &cropped, &[kyoto(2.0)], None).await.expect("ran").picture;
    assert_eq!(backend.log().ran.len(), ran, "the second run was not served from the cache");
    assert_eq!(fresh.dimensions(), (48, 32));

    let mut maps = Vec::new();
    for (name, picture) in [("fresh.jpg", &fresh), ("cached.jpg", &cached)] {
        assert!(
            Arc::ptr_eq(&picture.shared_gain_map().expect("kept"), &cropped_map),
            "{name}: not the cropped one"
        );

        let written = write(picture, &dir.path().join(name), ImageFormat::Jpeg, WriteOptions::default())
            .await
            .expect("written");
        assert_eq!(written.losses, [], "{name}");
        let file = std::fs::read(&written.path).expect("readable");
        let base = decode_bytes(&file).expect("decodable");
        let gain_map = metadata_bytes(&file).gain_map.expect("written");
        let (size, offset) = gain_map_entry(&file);
        assert_eq!((size, &file[offset..]), (gain_map.len(), &gain_map[..]), "{name}: the index locates it");
        let map = decode_bytes(&gain_map).expect("decodable");
        assert_eq!((base.width(), base.height()), (48, 32), "{name}");
        assert_eq!((map.width(), map.height()), (12, 8), "{name}: kept at the cropped source's resolution");

        // In register: the base and the gain map carry the same ramp across the scene, so each gain-map column
        // matches the base four columns to its right of it (the base was cropped then doubled; the map only cropped).
        for x in [0, 5, 11] {
            let (in_base, in_map) = (first(&base, x * 4 + 2, 16), first(&map, x, 4));
            assert!((in_base - in_map).abs() <= 12, "{name}: column {x}: base {in_base}, gain map {in_map}");
        }
        maps.push(gain_map);
    }
    assert_eq!(maps[0], maps[1], "a cached result writes the same gain map as a fresh one");
}
