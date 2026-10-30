//! The XMP editor and the rewrite policy, against packets shaped as Lightroom and ExifTool write them.

use std::path::Path;

use chrono::DateTime;

use super::edit::{Array, Xmp, ns};
use super::{Context, iim_date, rewrite};
use crate::export::metadata::iptc;
use crate::export::metadata::pixels::Geometry;
use crate::image::{Affine, Edits};
use crate::models::{FloatPrecision, Scale, Sharpen, Strength, Upscale};

/// A packet shaped as Lightroom writes one: most properties as attributes of one description, Camera Raw settings,
/// a gain-map description, a history whose events are attribute-form `rdf:li`s, a `DerivedFrom` written as an empty
/// element with attributes, keywords and a hierarchy, under the older `xap` prefix for the XMP basic schema.
const LIGHTROOM: &str = r#"<?xpacket begin="﻿" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/" x:xmptk="Adobe XMP Core 7.0-c000">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:xap="http://ns.adobe.com/xap/1.0/"
    xmlns:xmpMM="http://ns.adobe.com/xap/1.0/mm/"
    xmlns:stEvt="http://ns.adobe.com/xap/1.0/sType/ResourceEvent#"
    xmlns:stRef="http://ns.adobe.com/xap/1.0/sType/ResourceRef#"
    xmlns:dc="http://purl.org/dc/elements/1.1/"
    xmlns:lr="http://ns.adobe.com/lightroom/1.0/"
    xmlns:tiff="http://ns.adobe.com/tiff/1.0/"
    xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
    xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/"
   xap:CreatorTool="Adobe Lightroom 9.6 (Macintosh)"
   xap:ModifyDate="2026-10-19T07:48:35-04:00"
   xap:MetadataDate="2026-10-19T07:48:35-04:00"
   xap:Rating="3"
   xmpMM:DocumentID="xmp.did:2d0aef45"
   xmpMM:InstanceID="xmp.iid:2d0aef45"
   xmpMM:OriginalDocumentID="5862F7E7"
   tiff:Orientation="6"
   tiff:ImageWidth="6192"
   crs:Version="18.7"
   crs:Exposure2012="-0.40"
   crs:HasCrop="True"
   hdrgm:Version="1.0">
   <dc:subject>
    <rdf:Bag>
     <rdf:li>hawk</rdf:li>
     <rdf:li>bird</rdf:li>
    </rdf:Bag>
   </dc:subject>
   <lr:hierarchicalSubject>
    <rdf:Bag>
     <rdf:li>animals|hawk</rdf:li>
    </rdf:Bag>
   </lr:hierarchicalSubject>
   <xmpMM:History>
    <rdf:Seq>
     <rdf:li stEvt:action="edited" stEvt:when="2026-10-18T21:13:32Z" stEvt:softwareAgent="Adobe Lightroom 9.6 (Macintosh)" stEvt:changed="/"/>
     <rdf:li stEvt:action="saved" stEvt:instanceID="xmp.iid:2d0aef45" stEvt:when="2026-10-19T07:48:35-04:00" stEvt:softwareAgent="Adobe Lightroom 9.6 (Macintosh)" stEvt:changed="/"/>
    </rdf:Seq>
   </xmpMM:History>
   <xmpMM:DerivedFrom stRef:documentID="5862F7E7" stRef:originalDocumentID="5862F7E7"/>
   <crs:ToneCurvePV2012>
    <rdf:Seq>
     <rdf:li>0, 0</rdf:li>
    </rdf:Seq>
   </crs:ToneCurvePV2012>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

/// A little-endian EXIF block with IFD0 Software `Camera FW 1.0` and DateTime, and an Exif IFD with OffsetTime.
fn exif_with_software() -> Vec<u8> {
    let mut b = b"II\x2a\0\x08\0\0\0".to_vec();
    // IFD0 at 8: Software (14 bytes at 50), DateTime (20 at 64), Exif pointer (to 84). Ends at 8 + 42 = 50.
    b.extend(3_u16.to_le_bytes());
    for (tag, kind, count, value) in [(0x0131_u16, 2_u16, 14_u32, 50_u32), (0x0132, 2, 20, 64), (0x8769, 4, 1, 84)] {
        b.extend(tag.to_le_bytes());
        b.extend(kind.to_le_bytes());
        b.extend(count.to_le_bytes());
        b.extend(value.to_le_bytes());
    }
    b.extend(0_u32.to_le_bytes());
    b.extend(b"Camera FW 1.0\0");
    b.extend(b"2026:10:18 16:03:50\0");
    // Exif IFD at 84: OffsetTime "-04:00" (7 bytes at 102).
    b.extend(1_u16.to_le_bytes());
    b.extend(0x9010_u16.to_le_bytes());
    b.extend(2_u16.to_le_bytes());
    b.extend(7_u32.to_le_bytes());
    b.extend(102_u32.to_le_bytes());
    b.extend(0_u32.to_le_bytes());
    b.extend(b"-04:00\0");
    b
}

fn context<'a>(output_type: &'static str, exif: Option<&'a [u8]>, iim: Option<&'a iptc::Iim>) -> Context<'a> {
    Context {
        source: Path::new("/pictures/hawk.jpg"),
        source_type: Some("image/jpeg".into()),
        output_type,
        width: 2394,
        height: 2992,
        now: DateTime::parse_from_rfc3339("2026-10-22T12:00:00-04:00").expect("a time"),
        document_id: "xmp.did:NEW".into(),
        instance_id: "xmp.iid:NEW".into(),
        exif,
        iim,
        gain_map: None,
        edits: &NO_EDITS,
        geometry: None,
    }
}

static NO_EDITS: Edits = Edits::NONE;

fn rewritten(packet: Option<&str>, described: bool, context: &Context<'_>) -> (String, Xmp) {
    let bytes = rewrite(packet.map(str::as_bytes), described, context).expect("a packet");
    let text = String::from_utf8(bytes).expect("UTF-8");
    let xmp = Xmp::parse(text.as_bytes()).expect("what is written parses");
    (text, xmp)
}

// ── The editor ──────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn properties_are_found_in_either_form_under_any_prefix() {
    let xmp = Xmp::parse(LIGHTROOM.as_bytes()).unwrap();
    // `xap:` is the XMP basic namespace: found by URI, not by the prefix.
    assert_eq!(xmp.get(ns::XMP, "Rating").as_deref(), Some("3"));
    assert_eq!(xmp.get(ns::XMP_MM, "DocumentID").as_deref(), Some("xmp.did:2d0aef45"));
    assert!(xmp.has(ns::DC, "subject"));
    assert_eq!(xmp.array_len(ns::DC, "subject"), Some(2));
    assert_eq!(xmp.array_len(ns::XMP_MM, "History"), Some(2));
    // An array is not a simple value.
    assert_eq!(xmp.get(ns::DC, "subject"), None);
    assert!(!xmp.has(ns::PHOTOSHOP, "City"));
}

#[test]
fn removing_a_namespace_takes_its_attributes_and_elements() {
    let mut xmp = Xmp::parse(LIGHTROOM.as_bytes()).unwrap();
    xmp.remove_namespace(ns::CRS);
    let text = String::from_utf8(xmp.to_bytes()).unwrap();
    assert!(!text.contains("crs:Version") && !text.contains("crs:ToneCurvePV2012") && !text.contains("crs:HasCrop"));
    // The rest is still there and the packet still parses.
    assert!(text.contains("<rdf:li>animals|hawk</rdf:li>"));
    // Nothing is left under `crs:`, so the namespace is no longer declared either.
    assert!(!text.contains("camera-raw-settings"), "{text}");
    assert!(Xmp::parse(text.as_bytes()).is_ok());
}

#[test]
fn set_replaces_wherever_the_property_was_and_declares_what_it_needs() {
    let mut xmp = Xmp::parse(LIGHTROOM.as_bytes()).unwrap();
    xmp.set(ns::XMP, "Rating", "5");
    xmp.set(ns::PHOTOSHOP, "City", "Cambridge & Boston");
    let text = String::from_utf8(xmp.to_bytes()).unwrap();

    // The attribute is gone and an element under the packet's own prefix took its place.
    assert!(!text.contains("xap:Rating=\"3\""));
    assert!(text.contains("<xap:Rating>5</xap:Rating>"), "{text}");
    // A namespace the packet never bound is declared, and the value escaped.
    assert!(text.contains("xmlns:photoshop=\"http://ns.adobe.com/photoshop/1.0/\""));
    assert!(text.contains("<photoshop:City>Cambridge &amp; Boston</photoshop:City>"));
    let again = Xmp::parse(text.as_bytes()).unwrap();
    assert_eq!(again.get(ns::XMP, "Rating").as_deref(), Some("5"));
    assert_eq!(again.get(ns::PHOTOSHOP, "City").as_deref(), Some("Cambridge & Boston"));
}

#[test]
fn a_prefix_bound_to_another_namespace_is_not_reused() {
    let packet = "<x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\
                  <rdf:Description rdf:about='' xmlns:xmp='urn:something-else' xmp:Thing='1'/></rdf:RDF></x:xmpmeta>";
    let mut xmp = Xmp::parse(packet.as_bytes()).unwrap();
    xmp.set(ns::XMP, "Rating", "2");
    let text = String::from_utf8(xmp.to_bytes()).unwrap();
    assert!(
        text.contains("xmlns:xmp1=\"http://ns.adobe.com/xap/1.0/\"") && text.contains("<xmp1:Rating>2</xmp1:Rating>")
    );
    let again = Xmp::parse(text.as_bytes()).unwrap();
    assert_eq!(again.get(ns::XMP, "Rating").as_deref(), Some("2"));
    assert_eq!(again.get("urn:something-else", "Thing").as_deref(), Some("1"));
}

#[test]
fn structures_and_arrays_are_written_and_appended_to() {
    let mut xmp = Xmp::empty();
    xmp.set_struct(ns::XMP_MM, "DerivedFrom", &[(ns::ST_REF, "documentID", "xmp.did:1".into())]);
    xmp.set_array(ns::DC, "subject", Array::Bag, &["a".into(), "b".into()]);
    xmp.set_array(ns::DC, "title", Array::Alt, &["T".into()]);
    assert!(xmp.append_struct(ns::XMP_MM, "History", &[(ns::ST_EVT, "action", "saved".into())]));
    assert!(xmp.append_struct(ns::XMP_MM, "History", &[(ns::ST_EVT, "action", "converted".into())]));
    let text = String::from_utf8(xmp.to_bytes()).unwrap();

    let again = Xmp::parse(text.as_bytes()).unwrap();
    assert_eq!(again.array_len(ns::XMP_MM, "History"), Some(2));
    assert_eq!(again.array_len(ns::DC, "subject"), Some(2));
    assert!(text.contains("<rdf:li xml:lang=\"x-default\">T</rdf:li>"), "{text}");
    assert!(text.contains("<xmpMM:DerivedFrom rdf:parseType=\"Resource\">"), "{text}");
    assert!(text.contains("<stRef:documentID>xmp.did:1</stRef:documentID>"));
    assert!(text.find("saved").unwrap() < text.find("converted").unwrap(), "appended out of order");
}

#[test]
fn a_packet_without_rdf_is_refused_and_one_without_a_description_gains_one() {
    assert!(Xmp::parse(b"<x:xmpmeta xmlns:x='adobe:ns:meta/'/>").is_err());
    let mut xmp = Xmp::parse(
        b"<x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'/></x:xmpmeta>",
    )
    .unwrap();
    xmp.set(ns::XMP, "Rating", "1");
    assert_eq!(Xmp::parse(&xmp.to_bytes()).unwrap().get(ns::XMP, "Rating").as_deref(), Some("1"));
}

// ── The policy ──────────────────────────────────────────────────────────────────────────────────────────────────

#[test]
fn a_lightroom_packet_gets_a_new_identity_a_history_event_and_loses_its_develop_settings() {
    let (text, xmp) = rewritten(Some(LIGHTROOM), true, &context("image/jpeg", None, None));

    assert_eq!(xmp.get(ns::XMP_MM, "DocumentID").as_deref(), Some("xmp.did:NEW"));
    assert_eq!(xmp.get(ns::XMP_MM, "InstanceID").as_deref(), Some("xmp.iid:NEW"));
    assert_eq!(xmp.get(ns::XMP_MM, "OriginalDocumentID").as_deref(), Some("5862F7E7"));
    assert!(text.contains("<stRef:documentID>xmp.did:2d0aef45</stRef:documentID>"), "{text}");
    assert!(text.contains("<stRef:instanceID>xmp.iid:2d0aef45</stRef:instanceID>"));
    assert!(text.contains("<stRef:originalDocumentID>5862F7E7</stRef:originalDocumentID>"));
    assert_eq!(xmp.get(ns::XMP, "CreatorTool").as_deref(), Some("Adobe Lightroom 9.6 (Macintosh)"));
    assert_eq!(xmp.get(ns::XMP, "ModifyDate").as_deref(), Some("2026-10-22T12:00:00-04:00"));
    assert_eq!(xmp.get(ns::XMP, "MetadataDate").as_deref(), Some("2026-10-22T12:00:00-04:00"));

    // The two events Lightroom recorded, then this export's: saved, since JPEG stays JPEG.
    assert_eq!(xmp.array_len(ns::XMP_MM, "History"), Some(3));
    let last = &text[text.rfind("<rdf:li").unwrap()..];
    assert!(last.contains("<stEvt:action>saved</stEvt:action>"), "{last}");
    assert!(last.contains("<stEvt:instanceID>xmp.iid:NEW</stEvt:instanceID>"));
    assert!(last.contains("<stEvt:when>2026-10-22T12:00:00-04:00</stEvt:when>"));
    assert!(last.contains(&format!(
        "<stEvt:softwareAgent>Open Photo AI {}</stEvt:softwareAgent>",
        env!("CARGO_PKG_VERSION")
    )));

    // Dropped, kept, and mirrored.
    assert!(!text.contains("crs:") || !text.contains("crs:Version"));
    assert!(!xmp.has(ns::CRS, "Version") && !xmp.has(ns::CRS, "ToneCurvePV2012") && !xmp.has(ns::HDRGM, "Version"));
    assert_eq!(xmp.get(ns::XMP, "Rating").as_deref(), Some("3"));
    assert!(text.contains("<rdf:li>hawk</rdf:li>") && text.contains("<rdf:li>animals|hawk</rdf:li>"));
    assert_eq!(xmp.get(ns::TIFF, "ImageWidth").as_deref(), Some("2394"));
    // The source's 6 described pixels the decode has already turned upright.
    assert_eq!(xmp.get(ns::TIFF, "Orientation").as_deref(), Some("1"));
    // Mirrors are replaced only where they were: no ImageLength is invented.
    assert!(!xmp.has(ns::TIFF, "ImageLength") && !xmp.has(ns::EXIF, "PixelXDimension"));
}

#[test]
fn a_format_change_is_recorded_as_a_conversion() {
    let (text, _) = rewritten(Some(LIGHTROOM), true, &context("image/png", None, None));
    let last = &text[text.rfind("<rdf:li").unwrap()..];
    assert!(last.contains("<stEvt:action>converted</stEvt:action>"), "{last}");
    assert!(last.contains("<stEvt:parameters>from image/jpeg to image/png</stEvt:parameters>"));
}

#[test]
fn a_source_without_xmp_gains_its_creator_and_history() {
    let exif = exif_with_software();
    let (text, xmp) = rewritten(None, true, &context("image/png", Some(&exif), None));

    assert_eq!(xmp.get(ns::XMP, "CreatorTool").as_deref(), Some("Camera FW 1.0"));
    assert_eq!(xmp.array_len(ns::XMP_MM, "History"), Some(2));
    let first = &text[text.find("<rdf:li").unwrap()..text.rfind("<rdf:li").unwrap()];
    assert!(first.contains("<stEvt:action>created</stEvt:action>"), "{text}");
    assert!(first.contains("<stEvt:softwareAgent>Camera FW 1.0</stEvt:softwareAgent>"));
    assert!(first.contains("<stEvt:when>2026-10-18T16:03:50-04:00</stEvt:when>"));
    // No source document, so nothing to derive from.
    assert!(!xmp.has(ns::XMP_MM, "DerivedFrom") && !xmp.has(ns::XMP_MM, "OriginalDocumentID"));
    assert!(text.starts_with("<?xpacket begin=") && text.ends_with("<?xpacket end='w'?>"));
}

#[test]
fn a_source_without_metadata_gets_none_and_a_malformed_packet_is_left_out_and_logged() {
    use crate::logging::{records, records_of_blocking};

    assert_eq!(rewrite(None, false, &context("image/jpeg", None, None)), None);
    let (log, written) = records_of_blocking("info", || {
        rewrite(Some(b"<x:xmpmeta><unclosed>"), true, &context("image/jpeg", None, None))
    });
    assert_eq!(written, None);
    assert_eq!(records(&log, "XMP left out: it could not be read").len(), 1, "{log}");
}

#[test]
fn iim_is_mapped_into_xmp_where_the_xmp_lacks_it() {
    let datasets = [
        &b"\x1c\x01\x5a\x00\x03\x1b%G"[..],
        b"\x1c\x02\x19\x00\x04hawk",
        b"\x1c\x02\x19\x00\x05eagle",
        b"\x1c\x02\x78\x00\x0bA red hawk.",
        b"\x1c\x02\x50\x00\x06Joseph",
        b"\x1c\x02\x5a\x00\x09Cambridge",
        b"\x1c\x02\x37\x00\x0820261018",
        b"\x1c\x02\x3c\x00\x0b160350-0400",
    ]
    .concat();
    let mut block = b"8BIM\x04\x04\0\0".to_vec();
    block.extend(u32::try_from(datasets.len()).unwrap().to_be_bytes());
    block.extend(&datasets);
    if block.len() % 2 == 1 {
        block.push(0);
    }
    let iim = iptc::iim(&block).expect("a record");

    // Into a packet that already has keywords: those win, and the rest are added.
    let (text, xmp) = rewritten(Some(LIGHTROOM), true, &context("image/png", None, Some(&iim)));
    assert_eq!(xmp.array_len(ns::DC, "subject"), Some(2));
    assert!(!text.contains("eagle"), "the IIM keywords overrode the XMP ones");
    assert!(text.contains("<rdf:li xml:lang=\"x-default\">A red hawk.</rdf:li>"), "{text}");
    assert_eq!(xmp.array_len(ns::DC, "creator"), Some(1));
    assert_eq!(xmp.get(ns::PHOTOSHOP, "City").as_deref(), Some("Cambridge"));
    assert_eq!(xmp.get(ns::PHOTOSHOP, "DateCreated").as_deref(), Some("2026-10-18T16:03:50-04:00"));

    // Into a new packet: every keyword.
    let (_, xmp) = rewritten(None, true, &context("image/webp", None, Some(&iim)));
    assert_eq!(xmp.array_len(ns::DC, "subject"), Some(2));
}

#[test]
fn iim_dates_are_read_with_or_without_their_time() {
    assert_eq!(iim_date(Some("20261018"), Some("160350-0400")).as_deref(), Some("2026-10-18T16:03:50-04:00"));
    assert_eq!(iim_date(Some("20261018"), None).as_deref(), Some("2026-10-18"));
    assert_eq!(iim_date(Some("20261018"), Some("16:03")).as_deref(), Some("2026-10-18"));
    assert_eq!(iim_date(Some("2026-10-18"), None), None);
    assert_eq!(iim_date(None, Some("160350-0400")), None);
    assert_eq!(iim_date(Some("20261018"), Some("16035é-040")).as_deref(), Some("2026-10-18"));
}

#[test]
fn rewriting_hostile_packets_never_panics() {
    let packet = LIGHTROOM.as_bytes();
    let exif = exif_with_software();
    let context = context("image/png", Some(&exif), None);
    for len in 0..packet.len() {
        if let Some(bytes) = rewrite(Some(&packet[..len]), true, &context) {
            assert!(Xmp::parse(&bytes).is_ok());
        }
    }
    let mut flipped = packet.to_vec();
    for at in 0..flipped.len() {
        for value in [b'<', b'>', b'"', b'/', b':', b'&', b'=', b' ', b'x', 0xFF] {
            let original = std::mem::replace(&mut flipped[at], value);
            if let Some(bytes) = rewrite(Some(&flipped), true, &context)
                && let Err(error) = Xmp::parse(&bytes)
            {
                panic!("a rewrite wrote what it cannot read (byte {at} = {value:#04x}): {error}");
            }
            flipped[at] = original;
        }
    }
}

// ── The review's cases ──────────────────────────────────────────────────────────────────────────────────────────

/// Every element and attribute prefix in `packet` is declared somewhere in it: a cheap check that what is written is
/// namespace-well-formed (the reviews found undeclared `rdf:` and `dc:` prefixes).
fn assert_prefixes_declared(packet: &str) {
    let nodes = super::tree::parse(packet.as_bytes()).expect("well-formed");
    fn walk(nodes: &[super::tree::Node], declared: &mut Vec<String>) {
        for node in nodes {
            let super::tree::Node::Element(element) = node else { continue };
            let before = declared.len();
            for (name, _) in element.attributes() {
                if let Some(prefix) = name.strip_prefix("xmlns:") {
                    declared.push(prefix.to_owned());
                }
            }
            let mut names = vec![element.name.as_str()];
            names.extend(element.attributes().iter().map(|(name, _)| name.as_str()));
            for name in names {
                if let Some((prefix, _)) = name.split_once(':')
                    && !matches!(prefix, "xml" | "xmlns")
                {
                    assert!(declared.iter().any(|bound| bound == prefix), "{prefix}: is not declared ({name})");
                }
            }
            walk(&element.children, declared);
            declared.truncate(before);
        }
    }
    walk(&nodes, &mut Vec::new());
}

#[test]
fn rdf_as_the_default_namespace_with_no_description_neither_panics_nor_writes_undeclared_prefixes() {
    // x-panic.jpg: the CLI aborted here.
    let packet = "<?xpacket begin='\u{feff}' id='W5M0MpCehiHzreSzNTczkc9d'?>\n<x:xmpmeta xmlns:x='adobe:ns:meta/'>\n\
                  <RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'></RDF>\n</x:xmpmeta>\n<?xpacket end='w'?>";
    let exif = exif_with_software();
    let (text, xmp) = rewritten(Some(packet), true, &context("image/png", Some(&exif), None));
    assert_prefixes_declared(&text);
    assert_eq!(xmp.array_len(ns::XMP_MM, "History"), Some(2));
    assert_eq!(xmp.get(ns::XMP, "CreatorTool").as_deref(), Some("Camera FW 1.0"));
}

#[test]
fn rdf_as_the_default_namespace_with_a_description_gets_declared_prefixes() {
    // x-defaultns.jpg: `rdf:Seq`/`rdf:li` were written with no `rdf` prefix in scope.
    let packet = "<x:xmpmeta xmlns:x='adobe:ns:meta/'><RDF xmlns='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\
                  <Description xmlns:dc='http://purl.org/dc/elements/1.1/' xmlns:xmp='http://ns.adobe.com/xap/1.0/' \
                  xmp:Rating='3'/></RDF></x:xmpmeta>";
    let (text, xmp) = rewritten(Some(packet), true, &context("image/jpeg", None, None));
    assert_prefixes_declared(&text);
    assert_eq!(xmp.get(ns::XMP, "Rating").as_deref(), Some("3"));
    assert_eq!(xmp.array_len(ns::XMP_MM, "History"), Some(1));
}

#[test]
fn prefixes_bound_to_other_namespaces_are_left_alone() {
    // x-collide.jpg: `xmpMM`, `stEvt`, `stRef` and `xmp` all bound to other namespaces.
    let packet = "<x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'>\
                  <rdf:Description rdf:about='' xmlns:xmpMM='urn:other' xmlns:stEvt='urn:evt' xmlns:stRef='urn:ref' \
                  xmlns:xmp='urn:notxmp' xmpMM:DocumentID='keep-me' xmp:Rating='5'/></rdf:RDF></x:xmpmeta>";
    let (text, xmp) = rewritten(Some(packet), true, &context("image/jpeg", None, None));
    assert_prefixes_declared(&text);
    assert_eq!(xmp.get("urn:other", "DocumentID").as_deref(), Some("keep-me"));
    assert_eq!(xmp.get("urn:notxmp", "Rating").as_deref(), Some("5"));
    assert_eq!(xmp.get(ns::XMP_MM, "DocumentID").as_deref(), Some("xmp.did:NEW"));
}

#[test]
fn control_characters_from_iim_and_exif_never_reach_the_xmp() {
    // ctl.jpg: a Latin-1 IIM caption with a BEL and NUL padding, and an EXIF Software with a control character.
    let datasets = [
        &b"\x1c\x02\x78\x00\x14caption\x07bell nul\x00\x00\x00\x00"[..],
        b"\x1c\x02\x19\x00\x08key\x01word",
    ]
    .concat();
    let mut block = b"8BIM\x04\x04\0\0".to_vec();
    block.extend(u32::try_from(datasets.len()).unwrap().to_be_bytes());
    block.extend(&datasets);
    if block.len() % 2 == 1 {
        block.push(0);
    }
    let iim = iptc::iim(&block).expect("a record");
    let mut exif = exif_with_software();
    let at = exif.windows(4).position(|window| window == b"FW 1").unwrap();
    exif[at + 2] = 0x02;

    let (text, xmp) = rewritten(None, true, &context("image/png", Some(&exif), Some(&iim)));
    assert!(!text.chars().any(|c| !super::tree::allowed(c)), "{text:?}");
    assert!(text.contains("captionbell nul"), "{text}");
    assert!(text.contains("<rdf:li>keyword</rdf:li>"), "{text}");
    assert_eq!(xmp.get(ns::XMP, "CreatorTool").as_deref(), Some("Camera FW1.0"));
}

#[test]
fn a_namespace_still_in_use_keeps_its_declaration() {
    // `hdrgm:` removed, but a structure inside another property still spells something under that prefix: the
    // declaration must stay, or the packet would no longer parse.
    let packet = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/" hdrgm:Version="1.0"><dc:source rdf:parseType="Resource"><hdrgm:Odd>1</hdrgm:Odd></dc:source></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let mut xmp = Xmp::parse(packet.as_bytes()).unwrap();
    xmp.remove_namespace(ns::HDRGM);
    let text = String::from_utf8(xmp.to_bytes()).unwrap();

    assert!(!text.contains("hdrgm:Version"), "{text}");
    assert!(text.contains("xmlns:hdrgm="), "{text}");
    assert!(Xmp::parse(text.as_bytes()).is_ok());
}

#[test]
fn a_namespace_declared_above_the_descriptions_goes_once_unused() {
    // Declared on `rdf:RDF` and again on `x:xmpmeta`: once the `hdrgm:` properties are gone, neither declaration may
    // stay, or a reader that looks for the gain-map namespace anywhere would take the export for a gain-map image.
    let packet = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/"><rdf:Description rdf:about="" hdrgm:Version="1.0"><dc:format>image/jpeg</dc:format></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let mut xmp = Xmp::parse(packet.as_bytes()).unwrap();
    xmp.remove_namespace(ns::HDRGM);
    let text = String::from_utf8(xmp.to_bytes()).unwrap();

    assert!(!text.contains("hdr-gain-map") && !text.contains("hdrgm"), "{text}");
    assert!(text.contains("xmlns:dc=") && text.contains("<dc:format>image/jpeg</dc:format>"), "{text}");
    // The scope at `rdf:RDF` follows: properties can still be read and added.
    let mut xmp = Xmp::parse(text.as_bytes()).unwrap();
    assert_eq!(xmp.get(ns::DC, "format").as_deref(), Some("image/jpeg"));
    xmp.set(ns::HDRGM, "Version", "1.0");
    assert!(Xmp::parse(&xmp.to_bytes()).unwrap().has(ns::HDRGM, "Version"));
}

#[test]
fn a_namespace_declared_above_the_descriptions_stays_while_any_description_uses_it() {
    // A second description still holds an `hdrgm:` property under a redeclared prefix of another namespace: that is
    // no use of the `rdf:RDF` declaration, but `crs:` in it is.
    let packet = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"><rdf:Description rdf:about="" hdrgm:Version="1.0" crs:Version="15.0"/><rdf:Description rdf:about="" xmlns:hdrgm="urn:other"><hdrgm:Kept>1</hdrgm:Kept></rdf:Description></rdf:RDF></x:xmpmeta>"#;
    let mut xmp = Xmp::parse(packet.as_bytes()).unwrap();
    xmp.remove_namespace(ns::HDRGM);
    let text = String::from_utf8(xmp.to_bytes()).unwrap();

    assert!(!text.contains("hdr-gain-map") && !text.contains("hdrgm:Version"), "{text}");
    assert!(
        text.contains(r#"xmlns:hdrgm="urn:other""#) && text.contains("<hdrgm:Kept>1</hdrgm:Kept>"),
        "{text}"
    );
    assert!(text.contains("camera-raw-settings") && text.contains("crs:Version"), "{text}");
    assert!(Xmp::parse(text.as_bytes()).is_ok());
}

// ── The gain map, written ───────────────────────────────────────────────────────────────────────────────────────

/// [`context`] for a JPEG export that writes a gain map of `length` bytes.
fn with_gain_map(length: usize) -> Context<'static> {
    Context { gain_map: Some(length), ..context("image/jpeg", None, None) }
}

/// An Ultra HDR packet as a Pixel writes one, with a Motion Photo: a directory of the primary (with a length and a
/// padding), the gain map and the video, items in the attribute form, and the `GCamera` Motion Photo properties.
const PIXEL: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" xmlns:GCamera="http://ns.google.com/photos/1.0/camera/" xmlns:Container="http://ns.google.com/photos/1.0/container/" xmlns:Item="http://ns.google.com/photos/1.0/container/item/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" hdrgm:Version="1.0" GCamera:MotionPhoto="1" GCamera:MotionPhotoVersion="1" GCamera:MotionPhotoPresentationTimestampUs="968000" xmp:Rating="4">
 <Container:Directory>
  <rdf:Seq>
   <rdf:li rdf:parseType="Resource">
    <Container:Item Item:Mime="image/jpeg" Item:Semantic="Primary" Item:Length="0" Item:Padding="0"/>
   </rdf:li>
   <rdf:li rdf:parseType="Resource">
    <Container:Item Item:Mime="image/jpeg" Item:Semantic="GainMap" Item:Length="31337"/>
   </rdf:li>
   <rdf:li rdf:parseType="Resource">
    <Container:Item Item:Mime="video/mp4" Item:Semantic="MotionPhoto" Item:Length="2400111" Item:Padding="0"/>
   </rdf:li>
  </rdf:Seq>
 </Container:Directory>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;

#[test]
fn a_lightroom_packet_written_with_its_gain_map_keeps_hdrgm_and_still_loses_its_develop_settings() {
    let (text, xmp) = rewritten(Some(LIGHTROOM), true, &with_gain_map(233_897));

    assert_eq!(xmp.get(ns::HDRGM, "Version").as_deref(), Some("1.0"));
    assert!(!xmp.has(ns::CRS, "Version") && !text.contains("crs:"), "{text}");
    // Adobe style has no directory, and none is invented.
    assert!(!text.contains("Container"), "{text}");
    assert_prefixes_declared(&text);

    // Without the gain map, as before.
    let (text, xmp) = rewritten(Some(LIGHTROOM), true, &context("image/jpeg", None, None));
    assert!(!xmp.has(ns::HDRGM, "Version") && !text.contains("hdr-gain-map"), "{text}");
}

#[test]
fn a_pixel_directory_keeps_the_primary_and_the_gain_map_with_its_new_length_and_drops_the_video() {
    let (text, xmp) = rewritten(Some(PIXEL), true, &with_gain_map(4242));

    assert_eq!(xmp.get(ns::HDRGM, "Version").as_deref(), Some("1.0"));
    assert!(text.contains(r#"Item:Semantic="GainMap" Item:Length="4242""#), "{text}");
    assert!(text.contains(r#"Item:Semantic="Primary""#), "{text}");
    for gone in ["31337", "MotionPhoto", "video/mp4", "Item:Padding", r#"Item:Length="0""#, "GCamera"] {
        assert!(!text.contains(gone), "{gone}: {text}");
    }
    assert_eq!(xmp.get(ns::XMP, "Rating").as_deref(), Some("4"), "the rest is kept");
    assert_eq!(text.matches("<Container:Item").count(), 2, "{text}");
    assert_prefixes_declared(&text);

    // Without the gain map, the whole description of it goes.
    let (text, _) = rewritten(Some(PIXEL), true, &context("image/jpeg", None, None));
    for gone in ["hdrgm", "Container", "Item:", "MotionPhoto\""] {
        assert!(!text.contains(gone), "{gone}: {text}");
    }
}

#[test]
fn item_fields_in_the_element_form_and_inside_a_description_are_edited_alike() {
    let element_form = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:Container="http://ns.google.com/photos/1.0/container/" xmlns:Item="http://ns.google.com/photos/1.0/container/item/"><Container:Directory><rdf:Seq><rdf:li rdf:parseType="Resource"><Container:Item rdf:parseType="Resource"><Item:Semantic>Primary</Item:Semantic><Item:Padding>8</Item:Padding></Container:Item></rdf:li><rdf:li><rdf:Description><Container:Item><rdf:Description Item:Semantic="GainMap" Item:Mime="image/jpeg"/></Container:Item></rdf:Description></rdf:li></rdf:Seq></Container:Directory></rdf:Description></rdf:RDF></x:xmpmeta>"#;

    let (text, _) = rewritten(Some(element_form), true, &with_gain_map(77));

    // The gain map had no length: it gains one in the form its semantic is written in.
    assert!(text.contains(r#"Item:Semantic="GainMap" Item:Mime="image/jpeg" Item:Length="77""#), "{text}");
    assert!(
        text.contains("<Item:Semantic>Primary</Item:Semantic>") && !text.contains("Item:Padding"),
        "{text}"
    );
    assert_prefixes_declared(&text);

    // And an element-form length is replaced where it is.
    let with_length = element_form.replace("<Item:Padding>8</Item:Padding>", "<Item:Length>9</Item:Length>").replace(
        r#"<rdf:Description Item:Semantic="GainMap" Item:Mime="image/jpeg"/>"#,
        "<rdf:Description><Item:Semantic>GainMap</Item:Semantic><Item:Length>1</Item:Length></rdf:Description>",
    );
    let (text, _) = rewritten(Some(&with_length), true, &with_gain_map(78));
    assert!(text.contains("<Item:Length>78</Item:Length>") && !text.contains("<Item:Length>9<"), "{text}");
}

#[test]
fn a_directory_without_a_gain_map_item_is_dropped_whole() {
    let packet = PIXEL.replace(r#"Item:Semantic="GainMap""#, r#"Item:Semantic="Depth""#);

    let (text, xmp) = rewritten(Some(&packet), true, &with_gain_map(10));

    assert!(!text.contains("Container") && !text.contains("Item:"), "{text}");
    assert_eq!(xmp.get(ns::HDRGM, "Version").as_deref(), Some("1.0"));
}

#[test]
fn a_lightroom_size_packet_written_with_its_gain_map_stays_one_standard_segment() {
    // Lightroom's packets are mostly develop settings; the hawk's is 17,048 bytes. One padded to 120 kB with them still
    // fits one `APP1` once they are dropped, so it needs no extended XMP at all (were it to, `rust-sak` would keep
    // `hdrgm:Version` in the standard segment, where Android looks, and move the largest other properties).
    let padding: String = (0..3000).map(|n| format!("\n   crs:Setting{n}=\"{n:020}\"")).collect();
    let large = LIGHTROOM.replace(r#"crs:Version="18.7""#, &format!(r#"crs:Version="18.7"{padding}"#));
    assert!(large.len() > 120_000);

    let (text, xmp) = rewritten(Some(&large), true, &with_gain_map(233_897));

    assert!(text.len() < 65_502, "{} bytes", text.len());
    assert_eq!(xmp.get(ns::HDRGM, "Version").as_deref(), Some("1.0"));
}

#[test]
fn a_gain_map_declared_on_rdf_rdf_keeps_its_declaration_only_while_written() {
    let packet = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/"><rdf:Description rdf:about="" hdrgm:Version="1.0"/></rdf:RDF></x:xmpmeta>"#;

    // Written: `hdrgm:Version` and its declaration on `rdf:RDF` both stay.
    let (text, xmp) = rewritten(Some(packet), true, &with_gain_map(4242));
    assert_eq!(xmp.get(ns::HDRGM, "Version").as_deref(), Some("1.0"));
    assert!(text.contains(r#"xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/""#), "{text}");
    assert_prefixes_declared(&text);

    // Not written: neither does, so the SDR export is not read as carrying a gain map.
    let (text, _) = rewritten(Some(packet), true, &context("image/jpeg", None, None));
    assert!(!text.contains("hdr-gain-map") && !text.contains("hdrgm"), "{text}");
}

// ── Pixels and the original ─────────────────────────────────────────────────────────────────────────────────────

/// A packet as an old Google Camera Lens Blur writes one: the original image, its focus, its depth map, and the
/// subject's position and focal plane mirrored from the EXIF.
const LENS_BLUR: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description rdf:about="" xmlns:GImage="http://ns.google.com/photos/1.0/image/" xmlns:GFocus="http://ns.google.com/photos/1.0/focus/" xmlns:GDepth="http://ns.google.com/photos/1.0/depthmap/" xmlns:exif="http://ns.adobe.com/exif/1.0/" GImage:Mime="image/jpeg" GImage:Data="T1JJR0lOQUw=" GFocus:BlurAtInfinity="0.01" GFocus:FocalPointX="0.5" GDepth:Format="RangeInverse" GDepth:ImageWidth="4000" GDepth:ImageHeight="3000" GDepth:Data="REVQVEg=" exif:FocalPlaneXResolution="4000/1"><exif:SubjectArea><rdf:Seq><rdf:li>3000</rdf:li><rdf:li>1000</rdf:li><rdf:li>400</rdf:li><rdf:li>200</rdf:li></rdf:Seq></exif:SubjectArea></rdf:Description></rdf:RDF></x:xmpmeta>"#;

/// The context of an export of `edits` at `size`, its geometry worked out as the export works it out.
fn edited(edits: &'static Edits, size: (u32, u32)) -> Context<'static> {
    let geometry = Geometry::of(None, edits, size).map(|geometry| &*Box::leak(Box::new(geometry)));
    Context { width: size.0, height: size.1, edits, geometry, ..context("image/jpeg", None, None) }
}

#[test]
fn an_unedited_export_keeps_googles_original_and_depth() {
    let (_, xmp) = rewritten(Some(LENS_BLUR), true, &edited(&NO_EDITS, (4000, 3000)));
    assert_eq!(xmp.get(ns::GIMAGE, "Data").as_deref(), Some("T1JJR0lOQUw="));
    assert_eq!(xmp.get(ns::GDEPTH, "Data").as_deref(), Some("REVQVEg="));
    assert_eq!(xmp.seq_values(ns::EXIF, "SubjectArea").map(|values| values.len()), Some(4));
}

#[test]
fn an_edited_export_drops_the_original_its_focus_and_its_depth_map() {
    let edits = Box::leak(Box::new(Edits::NONE.after(
        &[Sharpen::moscow(FloatPrecision::Fp32, Strength::new(1.0).expect("a strength"))],
        false,
        (4000, 3000),
        (4000, 3000),
    )));
    let (text, xmp) = rewritten(Some(LENS_BLUR), true, &edited(edits, (4000, 3000)));
    assert!(!text.contains("GImage") && !text.contains("T1JJR0lOQUw="), "{text}");
    assert!(!text.contains("GFocus") && !text.contains("GDepth") && !text.contains("REVQVEg="), "{text}");
    // Nothing moved, so the subject is where it was.
    assert_eq!(xmp.seq_values(ns::EXIF, "SubjectArea").map(|values| values.len()), Some(4));
}

#[test]
fn an_upscale_alone_keeps_the_depth_map_sized_for_the_export_and_moves_the_pixel_fields() {
    let edits = Box::leak(Box::new(Edits::NONE.after(
        &[Upscale::kyoto(FloatPrecision::Fp16, Scale::new(2.0).expect("a scale"))],
        false,
        (4000, 3000),
        (8000, 6000),
    )));
    let (text, xmp) = rewritten(Some(LENS_BLUR), true, &edited(edits, (8000, 6000)));
    assert!(!text.contains("GImage") && !text.contains("GFocus"), "{text}");
    assert_eq!(xmp.get(ns::GDEPTH, "Data").as_deref(), Some("REVQVEg="));
    assert_eq!(xmp.get(ns::GDEPTH, "ImageWidth").as_deref(), Some("8000"));
    assert_eq!(xmp.get(ns::GDEPTH, "ImageHeight").as_deref(), Some("6000"));
    assert_eq!(
        xmp.seq_values(ns::EXIF, "SubjectArea"),
        Some(["6000", "2000", "800", "400"].map(String::from).to_vec())
    );
    assert_eq!(xmp.get(ns::EXIF, "FocalPlaneXResolution").as_deref(), Some("8000/1"));
}

#[test]
fn a_crop_that_leaves_the_subject_out_removes_its_mirror_and_the_depth_map() {
    let edits = Box::leak(Box::new(Edits::NONE.framed((4000, 3000), Affine::IDENTITY)));
    let (text, xmp) = rewritten(Some(LENS_BLUR), true, &edited(edits, (2000, 3000)));
    assert!(!xmp.has(ns::EXIF, "SubjectArea"), "{text}");
    assert!(!text.contains("GDepth"), "{text}");
    // A crop leaves the pixel pitch as it was.
    assert_eq!(xmp.get(ns::EXIF, "FocalPlaneXResolution").as_deref(), Some("4000/1"));
}
