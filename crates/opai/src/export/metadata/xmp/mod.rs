//! The XMP an export writes: the source's, with the properties that would be false of the exported file rewritten.
//!
//! | Property | What it becomes |
//! |---|---|
//! | `xmpMM:DocumentID`, `xmpMM:InstanceID` | new `xmp.did:`/`xmp.iid:` UUIDs: the export is a new file |
//! | `xmpMM:OriginalDocumentID` | kept; the source's DocumentID where absent |
//! | `xmpMM:DerivedFrom` | the source's document, instance and original document IDs |
//! | `xmpMM:History` | kept, a `created` event for the source's software first where it had none, then a `converted` or `saved` event for Open Photo AI |
//! | `xmp:CreatorTool` | kept; the source's EXIF Software (else Open Photo AI) where absent |
//! | `xmp:ModifyDate`, `xmp:MetadataDate` | the export time |
//! | `tiff:Software`, `tiff:DateTime`, `tiff:ImageWidth`, `tiff:ImageLength`, `exif:PixelXDimension`, `exif:PixelYDimension` | replaced where present, as the EXIF's are |
//! | `tiff:Orientation` | 1, where present, as the EXIF's: the decode already turned the pixels upright |
//! | `crs:*`, `xmpNote:HasExtendedXMP` | dropped |
//! | `hdrgm:*`, `Container:*`, `Item:*` | dropped, unless the export writes the gain map (below) |
//! | `exif:SubjectArea`, `exif:SubjectLocation`, `exif:FocalPlaneXResolution`, `exif:FocalPlaneYResolution` | moved and scaled with the pixels, as the EXIF's are ([`pixels`](super::pixels)); removed where they can no longer be true |
//! | `GImage:*`, `GFocus:*` | dropped from an edited picture: the original image the export no longer is, and the focus it was rendered with |
//! | `stEvt:parameters` of that event, `opai:*`, `Iptc4xmpExt:DigitalSourceType` | what was rendered, by [`record`](super::record) |
//! | `GDepth:*` | dropped from an edited picture, unless only an upscale changed its pixels, in which case `GDepth:ImageWidth` and `GDepth:ImageHeight` (the colour image's size, by Google's specification) become the export's |
//!
//! **With the gain map written** (a JPEG export of a gain-map JPEG, [`Context::gain_map`]), what describes it is kept:
//! the `hdrgm:` properties as written, and a `Container:Directory` with its `Primary` item (its optional `Item:Length`
//! removed: the primary's length is not known while its XMP is written) and its `GainMap` item (its `Item:Length` set to
//! the written gain map's), every `Item:Padding` removed (the file is tightly packed), and every other item — a Motion
//! Photo's video, a depth map — removed with the `GCamera:MotionPhoto*` properties, since none is carried. A directory
//! without a `GainMap` item is dropped whole. Neither form is invented where the source lacked it.
//!
//! Everything else is carried as written. For a PNG or WebP export, which cannot hold IPTC-IIM, the source's IPTC Core
//! values are added as the XMP properties the IPTC Photo Metadata Standard maps them to, where the XMP lacks them.

pub(crate) mod edit;
pub(crate) mod tree;

#[cfg(test)]
mod tests;

use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use rust_sak::image::{ExifIfd, ExifValue, exif_field};

use super::exif::software;
use super::iptc::Iim;
use super::pixels::{Geometry, scaled_rational};
use super::record;
use crate::image::Edits;
use crate::models::Operation;
use edit::{Array, Xmp, ns};

/// What the rewrite needs to know about the export.
pub(crate) struct Context<'a> {
    /// The source photograph, for the warnings.
    pub(crate) source: &'a Path,
    /// The source's media type (`image/jpeg`, `image/x-arw`), if it is known.
    pub(crate) source_type: Option<String>,
    /// The export's media type.
    pub(crate) output_type: &'static str,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// The export's local time and UTC offset.
    pub(crate) now: DateTime<FixedOffset>,
    /// The export's new XMP document and instance IDs, `xmp.did:…` and `xmp.iid:…`.
    pub(crate) document_id: String,
    pub(crate) instance_id: String,
    /// The source's EXIF as it was carried, for the software and the date it records.
    pub(crate) exif: Option<&'a [u8]>,
    /// The source's IPTC-IIM, to map into the XMP, for an export that cannot hold IIM itself.
    pub(crate) iim: Option<&'a Iim>,
    /// The length of the gain-map JPEG the export writes after the primary, if it writes one.
    pub(crate) gain_map: Option<usize>,
    /// What was done to the source to make the exported pixels.
    pub(crate) edits: &'a Edits,
    /// How the source's pixels moved onto the exported ones, or `None` where they did not.
    pub(crate) geometry: Option<&'a Geometry>,
}

/// The XMP to write: `packet` rewritten for `context`; a new packet when the source had none but carried EXIF or
/// IPTC (`described`); `None` when it had no metadata at all, or when `packet` does not parse — logged, because a
/// packet that cannot be rewritten must not be copied unchanged.
pub(crate) fn rewrite(packet: Option<&[u8]>, described: bool, context: &Context<'_>) -> Option<Vec<u8>> {
    let mut xmp = match packet {
        Some(packet) => match Xmp::parse(packet) {
            Ok(xmp) => xmp,
            Err(error) => {
                tracing::warn!(op = "export", path = %context.source.display(), %error, "XMP left out: it could not be read");
                return None;
            }
        },
        None if described => Xmp::empty(),
        None => return None,
    };
    apply(&mut xmp, context);
    Some(xmp.to_bytes())
}

/// The table of the module documentation, in order.
fn apply(xmp: &mut Xmp, context: &Context<'_>) {
    let source_document = xmp.get(ns::XMP_MM, "DocumentID");
    let source_instance = xmp.get(ns::XMP_MM, "InstanceID");
    let original_document = xmp.get(ns::XMP_MM, "OriginalDocumentID");
    let exif_software = context.exif.and_then(|exif| ascii(exif, ExifIfd::Primary, 0x0131));
    let exported_by = software(context.now);
    let now = iso(context.now);

    // What would be false of the export, or describes what it does not carry.
    xmp.remove_namespace(ns::CRS);
    match context.gain_map {
        Some(length) => keep_gain_map(xmp, length),
        None => {
            for namespace in [ns::HDRGM, ns::CONTAINER, ns::ITEM] {
                xmp.remove_namespace(namespace);
            }
        }
    }
    xmp.remove(ns::XMP_NOTE, "HasExtendedXMP");
    if !context.edits.is_empty() {
        without_original(xmp, context);
    }

    // A new file, derived from the source.
    xmp.set(ns::XMP_MM, "DocumentID", &context.document_id);
    xmp.set(ns::XMP_MM, "InstanceID", &context.instance_id);
    if original_document.is_none()
        && let Some(document) = &source_document
    {
        xmp.set(ns::XMP_MM, "OriginalDocumentID", document);
    }
    let derived_from: Vec<_> = [
        ("documentID", &source_document),
        ("instanceID", &source_instance),
        ("originalDocumentID", &original_document),
    ]
    .into_iter()
    .filter_map(|(field, value)| value.clone().map(|value| (ns::ST_REF, field, value)))
    .collect();
    xmp.remove(ns::XMP_MM, "DerivedFrom");
    if !derived_from.is_empty() {
        xmp.set_struct(ns::XMP_MM, "DerivedFrom", &derived_from);
    }

    // Who made it, and when.
    if !xmp.has(ns::XMP, "CreatorTool") {
        xmp.set(ns::XMP, "CreatorTool", exif_software.as_deref().unwrap_or(&exported_by));
    }
    xmp.set(ns::XMP, "ModifyDate", &now);
    xmp.set(ns::XMP, "MetadataDate", &now);

    // The history: the source's, its creation where it recorded none, then this export.
    if xmp.array_len(ns::XMP_MM, "History").unwrap_or(0) == 0
        && let Some(created_by) = &exif_software
    {
        let mut created =
            vec![(ns::ST_EVT, "action", "created".to_owned()), (ns::ST_EVT, "softwareAgent", created_by.clone())];
        if let Some(when) = context.exif.and_then(source_time) {
            created.push((ns::ST_EVT, "when", when));
        }
        xmp.append_struct(ns::XMP_MM, "History", &created);
    }
    let converted = context.source_type.as_deref().is_none_or(|source| source != context.output_type);
    let mut event = vec![(ns::ST_EVT, "action", if converted { "converted" } else { "saved" }.to_owned())];
    let conversion = converted
        .then(|| context.source_type.as_ref().map(|source| format!("from {source} to {}", context.output_type)))
        .flatten();
    let parameters = match (conversion, record::parameters(context.edits)) {
        (Some(conversion), Some(edits)) => Some(format!("{conversion}; {edits}")),
        (conversion, edits) => conversion.or(edits),
    };
    if let Some(parameters) = parameters {
        event.push((ns::ST_EVT, "parameters", parameters));
    }
    event.extend([
        (ns::ST_EVT, "changed", "/".to_owned()),
        (ns::ST_EVT, "instanceID", context.instance_id.clone()),
        (ns::ST_EVT, "when", now.clone()),
        (ns::ST_EVT, "softwareAgent", format!("Open Photo AI {}", env!("CARGO_PKG_VERSION"))),
    ]);
    xmp.append_struct(ns::XMP_MM, "History", &event);

    // The mirrors of what the EXIF rewrite changed.
    let mut replace = |namespace: &str, local: &str, value: String| {
        if xmp.has(namespace, local) {
            xmp.set(namespace, local, &value);
        }
    };
    replace(ns::TIFF, "Software", exported_by.clone());
    replace(ns::TIFF, "DateTime", now.clone());
    replace(ns::TIFF, "ImageWidth", context.width.to_string());
    replace(ns::TIFF, "ImageLength", context.height.to_string());
    replace(ns::EXIF, "PixelXDimension", context.width.to_string());
    replace(ns::EXIF, "PixelYDimension", context.height.to_string());
    replace(ns::TIFF, "Orientation", "1".to_owned());
    if let Some(geometry) = context.geometry {
        move_pixel_fields(xmp, geometry);
    }

    // What was rendered, beside what the camera recorded.
    let focal_35mm = context.exif.and_then(|exif| match exif_field(exif, ExifIfd::Exif, 0xA405) {
        Some(ExifValue::Short(focal)) => Some(u32::from(focal)),
        _ => None,
    });
    record::record(xmp, context.edits, (context.width, context.height), focal_35mm);

    if let Some(iim) = context.iim {
        map_iim(xmp, iim);
    }
}

/// The gain-map description of a primary whose gain map is written after it, `length` bytes long: `hdrgm:` kept, and
/// the `Container:Directory` reduced to the primary and the gain map with the gain map's true length. See the module
/// documentation.
fn keep_gain_map(xmp: &mut Xmp, length: usize) {
    let mut has_gain_map = false;
    let semantic = (ns::ITEM, "Semantic");
    let found = xmp.edit_seq(ns::CONTAINER, "Directory", |item| {
        item.remove(ns::ITEM, "Padding");
        match item.get(semantic.0, semantic.1).as_deref() {
            Some("Primary") => {
                item.remove(ns::ITEM, "Length");
                true
            }
            Some("GainMap") if !has_gain_map => {
                has_gain_map = item.set(ns::ITEM, "Length", &length.to_string(), semantic);
                has_gain_map
            }
            _ => false,
        }
    });
    if found && !has_gain_map {
        // A directory that does not locate the gain map would mislead a reader that trusts it over the MPF index.
        xmp.remove_namespace(ns::CONTAINER);
        xmp.remove_namespace(ns::ITEM);
    }
    xmp.remove_starting(ns::GCAMERA, "MotionPhoto");
    xmp.remove_namespace_if_unused(ns::GCAMERA);
}

/// Google's description of the original photograph, taken out of an edited one's: `GImage:` holds the original image
/// itself — with whatever an erase removed still in it — and `GFocus:` the focus its blur was rendered at. `GDepth:`
/// goes too, its map outlining what was edited, unless only an upscale changed the pixels: the map then still
/// describes the same scene, and its `ImageWidth` and `ImageHeight`, the colour image's size, become the export's.
fn without_original(xmp: &mut Xmp, context: &Context<'_>) {
    xmp.remove_namespace(ns::GIMAGE);
    xmp.remove_namespace(ns::GFOCUS);

    let upscaled_only = context.edits.operations().iter().all(|operation| matches!(operation, Operation::Upscale(_)))
        && !context.edits.masked()
        && context.edits.frame().is_none_or(|frame| frame.to_pixels.is_scale());
    if upscaled_only && !context.edits.operations().is_empty() {
        let mut replace = |local: &str, value: u32| {
            if xmp.has(ns::GDEPTH, local) {
                xmp.set(ns::GDEPTH, local, &value.to_string());
            }
        };
        replace("ImageWidth", context.width);
        replace("ImageHeight", context.height);
    } else {
        xmp.remove_namespace(ns::GDEPTH);
    }
}

/// The `exif:` mirrors of the fields [`pixels`](super::pixels) moves in the EXIF, moved the same way from their own
/// values, so that a packet whose EXIF lacks them is made true too. One that cannot be read or moved is removed.
fn move_pixel_fields(xmp: &mut Xmp, geometry: &Geometry) {
    for local in ["SubjectArea", "SubjectLocation"] {
        let Some(values) = xmp.seq_values(ns::EXIF, local) else {
            if xmp.has(ns::EXIF, local) {
                xmp.remove(ns::EXIF, local);
            }
            continue;
        };
        let numbers: Option<Vec<u32>> = values.iter().map(|value| value.trim().parse().ok()).collect();
        xmp.remove(ns::EXIF, local);
        if let Some(moved) = numbers.and_then(|numbers| geometry.subject(&numbers)) {
            let moved: Vec<String> = moved.iter().map(u32::to_string).collect();
            xmp.set_array(ns::EXIF, local, Array::Seq, &moved);
        }
    }

    let (x, y) = geometry.pitch();
    for (local, by) in [("FocalPlaneXResolution", x), ("FocalPlaneYResolution", y)] {
        let Some(value) = xmp.get(ns::EXIF, local).filter(|_| (by - 1.0).abs() >= 1e-6) else {
            continue;
        };
        let rational = value.split_once('/').map_or_else(
            || value.trim().parse().ok().map(|numerator| (numerator, 1)),
            |(numerator, denominator)| Some((numerator.trim().parse().ok()?, denominator.trim().parse().ok()?)),
        );
        match rational.and_then(|(numerator, denominator)| scaled_rational(numerator, denominator, by)) {
            Some((numerator, denominator)) => xmp.set(ns::EXIF, local, &format!("{numerator}/{denominator}")),
            None => xmp.remove(ns::EXIF, local),
        }
    }
}

/// How each IPTC Core dataset of record 2 is written in XMP, by the IPTC Photo Metadata Standard.
enum Shape {
    Simple,
    Array(Array),
}

const IIM_TO_XMP: [(u8, &str, &str, Shape); 17] = [
    (5, ns::DC, "title", Shape::Array(Array::Alt)),
    (25, ns::DC, "subject", Shape::Array(Array::Bag)),
    (80, ns::DC, "creator", Shape::Array(Array::Seq)),
    (85, ns::PHOTOSHOP, "AuthorsPosition", Shape::Simple),
    (120, ns::DC, "description", Shape::Array(Array::Alt)),
    (122, ns::PHOTOSHOP, "CaptionWriter", Shape::Simple),
    (105, ns::PHOTOSHOP, "Headline", Shape::Simple),
    (116, ns::DC, "rights", Shape::Array(Array::Alt)),
    (110, ns::PHOTOSHOP, "Credit", Shape::Simple),
    (115, ns::PHOTOSHOP, "Source", Shape::Simple),
    (40, ns::PHOTOSHOP, "Instructions", Shape::Simple),
    (90, ns::PHOTOSHOP, "City", Shape::Simple),
    (92, ns::IPTC_CORE, "Location", Shape::Simple),
    (95, ns::PHOTOSHOP, "State", Shape::Simple),
    (101, ns::PHOTOSHOP, "Country", Shape::Simple),
    (100, ns::IPTC_CORE, "CountryCode", Shape::Simple),
    (103, ns::PHOTOSHOP, "TransmissionReference", Shape::Simple),
];

/// Adds each IPTC Core value of `iim` whose XMP property is absent. An XMP value already there wins.
fn map_iim(xmp: &mut Xmp, iim: &Iim) {
    for (number, namespace, local, shape) in &IIM_TO_XMP {
        if xmp.has(namespace, local) {
            continue;
        }
        let values = iim.all(*number);
        match shape {
            Shape::Simple => {
                if let Some(value) = values.first() {
                    xmp.set(namespace, local, value);
                }
            }
            Shape::Array(kind) => {
                // An `rdf:Alt` holds the one `x-default` value; the others hold every repetition.
                let values: Vec<String> =
                    if *kind == Array::Alt { values.into_iter().take(1).collect() } else { values };
                if !values.is_empty() {
                    xmp.set_array(namespace, local, *kind, &values);
                }
            }
        }
    }
    if !xmp.has(ns::PHOTOSHOP, "DateCreated")
        && let Some(date) = iim_date(iim.first(55).as_deref(), iim.first(60).as_deref())
    {
        xmp.set(ns::PHOTOSHOP, "DateCreated", &date);
    }
}

/// IIM DateCreated (`CCYYMMDD`) and TimeCreated (`HHMMSS±HHMM`) as an XMP date; the date alone if the time is absent
/// or not in that form, `None` if the date is not.
fn iim_date(date: Option<&str>, time: Option<&str>) -> Option<String> {
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let date = date.filter(|date| date.len() == 8 && digits(date))?;
    let day = format!("{}-{}-{}", &date[..4], &date[4..6], &date[6..8]);
    let Some(time) = time.filter(|time| time.len() == 11 && time.is_ascii()) else {
        return Some(day);
    };
    let (clock, zone) = time.split_at(6);
    if !digits(clock) || !matches!(&zone[..1], "+" | "-") || !digits(&zone[1..]) {
        return Some(day);
    }
    Some(format!("{day}T{}:{}:{}{}:{}", &clock[..2], &clock[2..4], &clock[4..], &zone[..3], &zone[3..]))
}

/// An ISO 8601 time with its offset, as XMP dates are written.
pub(crate) fn iso(time: DateTime<FixedOffset>) -> String {
    time.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// The source's EXIF DateTime as an XMP date, with the offset its OffsetTime records, if any.
fn source_time(exif: &[u8]) -> Option<String> {
    let time = NaiveDateTime::parse_from_str(&ascii(exif, ExifIfd::Primary, 0x0132)?, "%Y:%m:%d %H:%M:%S").ok()?;
    let mut when = time.format("%Y-%m-%dT%H:%M:%S").to_string();
    if let Some(offset) = ascii(exif, ExifIfd::Exif, 0x9010).filter(|offset| offset.len() == 6) {
        when.push_str(&offset);
    }
    Some(when)
}

fn ascii(exif: &[u8], ifd: ExifIfd, tag: u16) -> Option<String> {
    match exif_field(exif, ifd, tag)? {
        ExifValue::Ascii(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        _ => None,
    }
}
