//! A photograph's EXIF and IPTC, end to end: loaded, cropped, enhanced and exported, and true of the result.

use rust_sak::image::{ExifIfd, ExifValue, Metadata, encode_writer_with, exif_field, metadata_bytes};

use super::*;
use crate::export::metadata::xmp::edit::{self, ns};
use crate::export::{WriteOptions, write};
use crate::image::ImageFormat;
use crate::image::test_support::{
    DISPLAY_P3, IPTC, MAKER_NOTE_DATA, camera_exif, display_p3, photoshop_block, sample_image,
};

/// XMP as Lightroom leaves it on an exported JPEG: an identity, a history, a rating, keywords and Camera Raw settings.
const LIGHTROOM_XMP: &str = "<?xpacket begin='\u{feff}' id='W5M0MpCehiHzreSzNTczkc9d'?>\
<x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\
<rdf:Description rdf:about='' xmlns:xmp='http://ns.adobe.com/xap/1.0/' xmlns:xmpMM='http://ns.adobe.com/xap/1.0/mm/' \
xmlns:stEvt='http://ns.adobe.com/xap/1.0/sType/ResourceEvent#' xmlns:dc='http://purl.org/dc/elements/1.1/' \
xmlns:crs='http://ns.adobe.com/camera-raw-settings/1.0/' xmp:Rating='3' xmp:CreatorTool='Adobe Lightroom 9.6 (Macintosh)' \
xmpMM:DocumentID='xmp.did:source' xmpMM:InstanceID='xmp.iid:source' crs:Exposure2012='-0.40'>\
<dc:subject><rdf:Bag><rdf:li>hawk</rdf:li><rdf:li>bird</rdf:li></rdf:Bag></dc:subject>\
<xmpMM:History><rdf:Seq><rdf:li stEvt:action='saved' stEvt:instanceID='xmp.iid:source'/></rdf:Seq></xmpMM:History>\
</rdf:Description></rdf:RDF></x:xmpmeta><?xpacket end='w'?>";

fn ascii(text: &str) -> Option<ExifValue> {
    Some(ExifValue::Ascii(text.to_owned()))
}

#[tokio::test]
async fn a_cropped_and_upscaled_photograph_is_exported_with_true_exif_and_its_iptc() {
    let camera = camera_exif();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("camera.jpg");
    let mut metadata: Metadata = display_p3();
    metadata.exif = Some(camera.exif.clone());
    metadata.xmp = Some(LIGHTROOM_XMP.as_bytes().to_vec());
    metadata.iptc = Some(photoshop_block());
    let mut bytes = Vec::new();
    encode_writer_with(&sample_image(), &mut bytes, ImageFormat::Jpeg, None, &metadata).expect("encodable");
    std::fs::write(&path, bytes).expect("writable");

    let loaded = crate::image::load_blocking(&path).expect("decodable");
    assert_eq!(loaded.metadata().exif.as_deref(), Some(&camera.exif[..]));

    // A crop as the GUI makes one, then a 2x upscale on the fake backend: no model file is read.
    let cropped = loaded.derive(loaded.pixels().crop_imm(4, 2, 24, 16), format!("{}-cropped", loaded.identity()));
    let backend = Fake::new();
    let enhanced = process(&backend, None, &cropped, &[kyoto(2.0)], None).await.expect("ran").picture;
    assert_eq!(enhanced.dimensions(), (48, 32));

    for format in [ImageFormat::Jpeg, ImageFormat::Png, ImageFormat::WebP] {
        let destination = dir.path().join(format!("out.{}", format.extension()));
        let written = write(&enhanced, &destination, format, WriteOptions::default()).await.expect("written");
        let read = metadata_bytes(&std::fs::read(&written.path).expect("readable"));
        let exif = read.exif.as_deref().unwrap_or_else(|| panic!("{format:?} has no EXIF"));
        let field = |ifd, tag| exif_field(exif, ifd, tag);

        // Rewritten: software, modification time, dimensions; no thumbnail.
        assert!(
            matches!(field(ExifIfd::Primary, 0x0131), Some(ExifValue::Ascii(name)) if name.starts_with("Open Photo AI ")),
            "{format:?}"
        );
        assert_ne!(field(ExifIfd::Primary, 0x0132), ascii("2026:10:18 16:03:50"), "{format:?}");
        assert_eq!(field(ExifIfd::Exif, 0xA002), Some(ExifValue::Short(48)), "{format:?}");
        assert_eq!(field(ExifIfd::Exif, 0xA003), Some(ExifValue::Short(32)), "{format:?}");
        assert!(!exif.windows(16).any(|window| window == [0xD8; 16]), "{format:?}: the thumbnail is still there");

        // Kept: camera, capture time and offset, orientation (a native source), location.
        assert_eq!(field(ExifIfd::Primary, 0x010F), ascii("Canon"), "{format:?}");
        assert_eq!(field(ExifIfd::Primary, 0x0112), Some(ExifValue::Short(6)), "{format:?}");
        assert_eq!(field(ExifIfd::Exif, 0x9003), ascii("2026:10:18 16:03:50"), "{format:?}");
        assert_eq!(field(ExifIfd::Exif, 0x9011), ascii("-04:00"), "{format:?}");
        assert_eq!(field(ExifIfd::Gps, 0x0001), ascii("N"), "{format:?}");

        // The MakerNote: byte for byte at the same offset, and its absolute pointer still finds its data.
        assert_eq!(&exif[camera.maker_note.clone()], &camera.exif[camera.maker_note.clone()], "{format:?}");
        assert_eq!(&exif[236..244], MAKER_NOTE_DATA, "{format:?}");

        // IPTC: the IIM record alone, in JPEG only. The profile: as before.
        let block = photoshop_block();
        let digest = &block[block.len() - 28..];
        let iptc = (format == ImageFormat::Jpeg).then(|| [IPTC, digest].concat());
        assert_eq!(read.iptc, iptc, "{format:?}");
        assert_eq!(read.icc_profile.as_deref(), Some(DISPLAY_P3), "{format:?}");

        // XMP: kept, made new, extended, and in step with the EXIF.
        let xmp = String::from_utf8(read.xmp.clone().expect("XMP")).expect("UTF-8");
        let parsed = edit::Xmp::parse(xmp.as_bytes()).expect("parses");
        assert_eq!(parsed.get(ns::XMP, "Rating").as_deref(), Some("3"), "{format:?}");
        assert_eq!(parsed.get(ns::XMP, "CreatorTool").as_deref(), Some("Adobe Lightroom 9.6 (Macintosh)"));
        assert!(
            parsed
                .get(ns::XMP_MM, "DocumentID")
                .is_some_and(|id| id.starts_with("xmp.did:") && id != "xmp.did:source")
        );
        assert_eq!(parsed.array_len(ns::XMP_MM, "History"), Some(2), "{format:?}");
        assert!(xmp.contains("<stRef:documentID>xmp.did:source</stRef:documentID>"), "{format:?}: {xmp}");
        assert!(!parsed.has(ns::CRS, "Exposure2012"), "{format:?}");
        assert!(xmp.contains("<rdf:li>hawk</rdf:li><rdf:li>bird</rdf:li>"), "{format:?}");
        let action = if format == ImageFormat::Jpeg { "saved" } else { "converted" };
        assert!(xmp.contains(&format!("<stEvt:action>{action}</stEvt:action>")), "{format:?}: {xmp}");
    }
}

#[tokio::test]
async fn a_jpegs_iptc_reaches_a_png_through_xmp() {
    // A source with IPTC keywords and no XMP at all: a PNG cannot hold the IIM, so its keywords go into a new packet.
    let dir = tempfile::tempdir().expect("a temporary directory");
    let path = dir.path().join("iptc.jpg");
    let mut metadata = Metadata::default();
    metadata.iptc = Some(IPTC.to_vec());
    let mut bytes = Vec::new();
    encode_writer_with(&sample_image(), &mut bytes, ImageFormat::Jpeg, None, &metadata).expect("encodable");
    std::fs::write(&path, bytes).expect("writable");
    let loaded = crate::image::load_blocking(&path).expect("decodable");

    let written = write(&loaded, &dir.path().join("out.png"), ImageFormat::Png, WriteOptions::default())
        .await
        .expect("written");
    let read = metadata_bytes(&std::fs::read(&written.path).expect("readable"));

    assert_eq!(read.iptc, None);
    let xmp = edit::Xmp::parse(&read.xmp.expect("XMP")).expect("parses");
    assert_eq!(xmp.array_len(ns::DC, "subject"), Some(1));
    assert_eq!(xmp.array_len(ns::XMP_MM, "History"), Some(1));
}
