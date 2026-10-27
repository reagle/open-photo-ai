//! What a gain map's values mean: the parameters its `hdrgm` XMP or ISO 21496-1 metadata records, read so that an
//! enhancement that moves light around the frame can rebuild the HDR rendition and encode a gain map of its own.
//!
//! Nothing else interprets them: a turn, a crop or a resize does not change the ratios a gain map holds, so the rest of
//! the stack carries the metadata as bytes. Read from the gain-map image's ISO 21496-1 payload first, laid out as
//! `libultrahdr` reads it, and otherwise from its own XMP — the Adobe specification's `hdrgm` properties, as attributes
//! or elements, one value for every channel or an `rdf:Seq` of one per channel.

use crate::export::metadata::xmp::edit::{Xmp, ns};

/// How one channel of a gain map is applied, as both specifications state it:
///
/// - the stored value `g` in `0..=1` is a gain `G = 2^(min + (max - min) * g^(1 / gamma))`;
/// - the HDR rendition is `(SDR + offset_sdr) * G - offset_hdr`, in linear light.
///
/// `min`, `max` and the capacities are in stops (log2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GainMapParams {
    /// The log2 gain a stored 0 stands for, per channel.
    pub(crate) min: [f32; 3],
    /// The log2 gain a stored 1 stands for, per channel.
    pub(crate) max: [f32; 3],
    /// The exponent the stored values were encoded with, per channel.
    pub(crate) gamma: [f32; 3],
    /// Added to the SDR rendition before the gain, per channel.
    pub(crate) offset_sdr: [f32; 3],
    /// Subtracted from the HDR rendition after it, per channel.
    pub(crate) offset_hdr: [f32; 3],
    /// The display headroom, in stops, below which the SDR rendition is shown as it is.
    pub(crate) capacity_min: f32,
    /// The display headroom, in stops, at and above which the whole gain is applied.
    pub(crate) capacity_max: f32,
    /// Whether the base image is the HDR rendition and the gain map takes it down to SDR.
    pub(crate) base_is_hdr: bool,
    /// Whether the gain is applied in the base rendition's colour space, rather than the alternate's: the ISO 21496-1
    /// flag `0x40`. Always true of the `hdrgm` XMP, which has no such choice.
    pub(crate) in_base_colour_space: bool,
    /// How many channels the metadata describes: 1 (the same values for all three) or 3.
    pub(crate) channels: u8,
}

/// Why a gain map's parameters could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ParamsError {
    /// Neither an XMP packet with `hdrgm` properties nor an ISO 21496-1 payload.
    #[error("it carries no gain-map parameters")]
    Missing,
    /// The `hdrgm` XMP could not be read: not XML, a required property absent or a value that is not a number.
    #[error("its hdrgm XMP is unreadable: {0}")]
    Xmp(String),
    /// The ISO 21496-1 payload is truncated or of a version this does not read.
    #[error("its ISO 21496-1 metadata is unreadable: {0}")]
    Iso(&'static str),
    /// The values read describe no gain map: a gamma that is not positive, a maximum below the minimum, a value that is
    /// not finite.
    #[error("its parameters are out of range")]
    OutOfRange,
}

impl GainMapParams {
    /// The parameters `iso` (the gain-map image's ISO 21496-1 payload) or else `xmp` (its own packet) record.
    ///
    /// The payload is preferred where both are present, as `libultrahdr` prefers it: it is the standard, and it alone
    /// says in which colour space the gain applies. A payload that does not read falls through to the XMP, and where
    /// neither reads the payload's error is the one returned, as the one that was preferred.
    pub(crate) fn read(xmp: Option<&[u8]>, iso: Option<&[u8]>) -> Result<Self, ParamsError> {
        let unread = match iso.map(Self::from_iso) {
            Some(Ok(params)) => return Ok(params),
            Some(Err(error)) => Some(error),
            None => None,
        };
        match (xmp.map(Self::from_xmp), unread) {
            (Some(Ok(params)), _) => Ok(params),
            (_, Some(error)) | (Some(Err(error)), None) => Err(error),
            (None, None) => Err(ParamsError::Missing),
        }
    }

    /// The parameters of an `hdrgm` XMP packet, with the Adobe specification's defaults for what it leaves out:
    /// minimum 0, gamma 1, offsets 1/64, minimum capacity 0, an SDR base. `GainMapMax` and `HDRCapacityMax` have no
    /// default.
    pub(crate) fn from_xmp(packet: &[u8]) -> Result<Self, ParamsError> {
        let xmp = Xmp::parse(packet).map_err(|error| ParamsError::Xmp(error.to_string()))?;
        if !xmp.has(ns::HDRGM, "GainMapMax") && !xmp.has(ns::HDRGM, "Version") {
            return Err(ParamsError::Missing);
        }

        let mut channels = 1;
        let mut per_channel = |name: &str, default: Option<f32>| -> Result<[f32; 3], ParamsError> {
            let values = match (xmp.get(ns::HDRGM, name), xmp.seq_values(ns::HDRGM, name)) {
                (Some(value), _) => vec![value],
                (None, Some(values)) => values,
                (None, None) => {
                    let value = default.ok_or_else(|| ParamsError::Xmp(format!("hdrgm:{name} is absent")))?;
                    return Ok([value; 3]);
                }
            };
            let numbers = values.iter().map(|value| number(name, value)).collect::<Result<Vec<_>, _>>()?;
            match numbers[..] {
                [one] => Ok([one; 3]),
                [r, g, b] => {
                    channels = 3;
                    Ok([r, g, b])
                }
                _ => Err(ParamsError::Xmp(format!("hdrgm:{name} has {} values", numbers.len()))),
            }
        };
        let min = per_channel("GainMapMin", Some(0.0))?;
        let max = per_channel("GainMapMax", None)?;
        let gamma = per_channel("Gamma", Some(1.0))?;
        let offset_sdr = per_channel("OffsetSDR", Some(1.0 / 64.0))?;
        let offset_hdr = per_channel("OffsetHDR", Some(1.0 / 64.0))?;

        let scalar = |name: &str, default: Option<f32>| match xmp.get(ns::HDRGM, name) {
            Some(value) => number(name, &value),
            None => default.ok_or_else(|| ParamsError::Xmp(format!("hdrgm:{name} is absent"))),
        };
        let capacity_min = scalar("HDRCapacityMin", Some(0.0))?;
        let capacity_max = scalar("HDRCapacityMax", None)?;
        let base_is_hdr = match xmp.get(ns::HDRGM, "BaseRenditionIsHDR").as_deref().map(str::trim) {
            None => false,
            Some(value) if value.eq_ignore_ascii_case("true") => true,
            Some(value) if value.eq_ignore_ascii_case("false") => false,
            Some(value) => return Err(ParamsError::Xmp(format!("hdrgm:BaseRenditionIsHDR is {value:?}"))),
        };

        let in_base_colour_space = true;
        Self {
            min,
            max,
            gamma,
            offset_sdr,
            offset_hdr,
            capacity_min,
            capacity_max,
            base_is_hdr,
            in_base_colour_space,
            channels,
        }
        .checked()
    }

    /// The parameters of an ISO 21496-1 payload (the gain-map image's `APP2` after its URN): big-endian, a minimum and
    /// a writer version, a flags byte (multichannel `0x80`, base colour space `0x40`, backward direction `0x04`, common
    /// denominator `0x08`), the base and alternate headroom, then per channel the gain-map minimum, maximum, gamma and
    /// the base and alternate offsets, each a fraction.
    pub(crate) fn from_iso(payload: &[u8]) -> Result<Self, ParamsError> {
        let mut reader = Reader { bytes: payload, at: 0 };
        let minimum_version = reader.u16()?;
        let _writer_version = reader.u16()?;
        if minimum_version != 0 {
            return Err(ParamsError::Iso("a minimum version other than 0"));
        }
        let flags = reader.u8()?;
        let channels: u8 = if flags & 0x80 != 0 { 3 } else { 1 };
        let in_base_colour_space = flags & 0x40 != 0;
        let backward = flags & 0x04 != 0;
        let common = (flags & 0x08 != 0).then(|| reader.u32()).transpose()?;

        let mut fraction = |signed: bool| -> Result<f32, ParamsError> {
            let numerator = if signed { f64::from(reader.i32()?) } else { f64::from(reader.u32()?) };
            let denominator = match common {
                Some(common) => common,
                None => reader.u32()?,
            };
            if denominator == 0 {
                return Err(ParamsError::Iso("a zero denominator"));
            }
            #[allow(
                clippy::cast_possible_truncation,
                reason = "a ratio of two 32-bit integers, read as stops or gains"
            )]
            Ok((numerator / f64::from(denominator)) as f32)
        };
        let base_headroom = fraction(false)?;
        let alternate_headroom = fraction(false)?;

        // Per channel, in the order written: the gain-map minimum, maximum and gamma, the base and alternate offsets.
        let mut per_channel = [[0.0_f32; 5]; 3];
        for fields in per_channel.iter_mut().take(usize::from(channels)) {
            *fields = [fraction(true)?, fraction(true)?, fraction(false)?, fraction(true)?, fraction(true)?];
        }
        if channels == 1 {
            per_channel = [per_channel[0]; 3];
        }
        let values: [[f32; 3]; 5] = std::array::from_fn(|field| per_channel.map(|fields| fields[field]));
        let [min, max, gamma, base_offset, alternate_offset] = values;

        // The offsets are named for the renditions rather than for SDR and HDR: in the backward direction the base is
        // the HDR one.
        let base_is_hdr = backward || base_headroom > alternate_headroom;
        let (offset_sdr, offset_hdr) =
            if base_is_hdr { (alternate_offset, base_offset) } else { (base_offset, alternate_offset) };
        let (capacity_min, capacity_max) = if base_is_hdr {
            (alternate_headroom, base_headroom)
        } else {
            (base_headroom, alternate_headroom)
        };

        Self {
            min,
            max,
            gamma,
            offset_sdr,
            offset_hdr,
            capacity_min,
            capacity_max,
            base_is_hdr,
            in_base_colour_space,
            channels,
        }
        .checked()
    }

    /// These parameters, if they describe a gain map that can be applied.
    fn checked(self) -> Result<Self, ParamsError> {
        let channel_ok = |c: usize| {
            [self.min[c], self.max[c], self.gamma[c], self.offset_sdr[c], self.offset_hdr[c]]
                .iter()
                .all(|v| v.is_finite())
                && self.gamma[c] > 0.0
                && self.max[c] >= self.min[c]
        };
        let ok = (0..3).all(channel_ok) && self.capacity_min.is_finite() && self.capacity_max.is_finite();
        if ok { Ok(self) } else { Err(ParamsError::OutOfRange) }
    }
}

/// `value`, the text of `hdrgm:{name}`, as a number.
fn number(name: &str, value: &str) -> Result<f32, ParamsError> {
    value
        .trim()
        .parse()
        .map_err(|_| ParamsError::Xmp(format!("hdrgm:{name} is {value:?}, not a number")))
}

/// A big-endian cursor over an ISO 21496-1 payload that answers a truncation as an error.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], ParamsError> {
        let bytes = self.bytes.get(self.at..self.at + N).ok_or(ParamsError::Iso("it is truncated"))?;
        self.at += N;
        Ok(bytes.try_into().expect("N bytes"))
    }

    fn u8(&mut self) -> Result<u8, ParamsError> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, ParamsError> {
        self.take().map(u16::from_be_bytes)
    }

    fn u32(&mut self) -> Result<u32, ParamsError> {
        self.take().map(u32::from_be_bytes)
    }

    fn i32(&mut self) -> Result<i32, ParamsError> {
        self.take().map(i32::from_be_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::test_support::{HAWK_GAIN_MAP_ISO, HAWK_GAIN_MAP_XMP};

    fn close(a: [f32; 3], b: [f32; 3], tolerance: f32) -> bool {
        a.iter().zip(&b).all(|(a, b)| (a - b).abs() <= tolerance)
    }

    #[test]
    fn the_hawks_xmp_and_iso_payload_give_the_same_values() {
        let xmp = GainMapParams::from_xmp(HAWK_GAIN_MAP_XMP).expect("readable");
        let iso = GainMapParams::from_iso(HAWK_GAIN_MAP_ISO).expect("readable");

        assert!(close(xmp.min, [-0.575_491, -0.573_529, -0.571_247], 1e-6), "{xmp:?}");
        assert!(close(xmp.max, [2.418_243, 2.513_733, 2.679_443], 1e-6), "{xmp:?}");
        assert!(close(xmp.gamma, [0.647_195, 0.866_89, 0.856_296], 1e-6), "{xmp:?}");
        assert_eq!((xmp.offset_sdr, xmp.offset_hdr), ([0.015_625; 3], [0.015_625; 3]));
        assert_eq!((xmp.capacity_min, xmp.channels, xmp.base_is_hdr), (0.0, 3, false));
        assert!((xmp.capacity_max - 2.299_988).abs() < 1e-6);

        for (a, b) in [(xmp.min, iso.min), (xmp.max, iso.max), (xmp.gamma, iso.gamma)] {
            assert!(close(a, b, 1e-5), "{a:?} against {b:?}");
        }
        assert_eq!((iso.offset_sdr, iso.offset_hdr), (xmp.offset_sdr, xmp.offset_hdr));
        assert!((iso.capacity_max - xmp.capacity_max).abs() < 1e-5);
        assert_eq!((iso.capacity_min, iso.channels, iso.base_is_hdr), (0.0, 3, false));

        assert!(xmp.in_base_colour_space && iso.in_base_colour_space, "Lightroom sets the flag");

        // The XMP read where the payload does not, and the payload where the XMP says nothing of a gain map.
        assert_eq!(GainMapParams::read(Some(HAWK_GAIN_MAP_XMP), Some(&[0; 4])), Ok(xmp));
        let unrelated = b"<x:xmpmeta xmlns:x='adobe:ns:meta/'>\
            <rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'/></x:xmpmeta>";
        assert_eq!(GainMapParams::read(Some(unrelated), Some(HAWK_GAIN_MAP_ISO)), Ok(iso));
        assert_eq!(GainMapParams::read(None, None), Err(ParamsError::Missing));
    }

    /// An `hdrgm` packet in element form with `body` inside its description.
    fn element_xmp(body: &str) -> Vec<u8> {
        format!(
            r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/">{body}</rdf:Description>
            </rdf:RDF></x:xmpmeta>"#
        )
        .into_bytes()
    }

    #[test]
    fn the_iso_payload_is_preferred_and_the_xmp_read_whenever_it_does_not_read() {
        let iso = GainMapParams::from_iso(HAWK_GAIN_MAP_ISO).expect("readable");

        // Where the two disagree, the payload wins.
        let other = element_xmp(
            "<hdrgm:Version>1.0</hdrgm:Version><hdrgm:GainMapMax>3.5</hdrgm:GainMapMax>\
            <hdrgm:HDRCapacityMax>3.5</hdrgm:HDRCapacityMax>",
        );
        let xmp = GainMapParams::from_xmp(&other).expect("readable");
        assert_ne!(xmp.max, iso.max);
        assert_eq!(GainMapParams::read(Some(&other), Some(HAWK_GAIN_MAP_ISO)), Ok(iso));

        // A broken packet — a version and no maximum — beside a good payload.
        let broken = element_xmp("<hdrgm:Version>1.0</hdrgm:Version>");
        assert!(matches!(GainMapParams::from_xmp(&broken), Err(ParamsError::Xmp(_))));
        assert_eq!(GainMapParams::read(Some(&broken), Some(HAWK_GAIN_MAP_ISO)), Ok(iso));

        // A payload that does not read: the XMP, and where that does not read either, the payload's error.
        assert_eq!(GainMapParams::read(Some(&other), Some(&HAWK_GAIN_MAP_ISO[..9])), Ok(xmp));
        let truncated = Err(ParamsError::Iso("it is truncated"));
        assert_eq!(GainMapParams::read(Some(&broken), Some(&HAWK_GAIN_MAP_ISO[..9])), truncated);
        assert_eq!(GainMapParams::read(None, Some(&HAWK_GAIN_MAP_ISO[..9])), truncated);
        assert!(matches!(GainMapParams::read(Some(&broken), None), Err(ParamsError::Xmp(_))));
    }

    #[test]
    fn the_iso_payload_says_whether_the_gain_applies_in_the_base_colour_space() {
        let mut alternate = HAWK_GAIN_MAP_ISO.to_vec();
        alternate[4] &= !0x40;

        let params = GainMapParams::from_iso(&alternate).expect("readable");

        assert!(!params.in_base_colour_space);
        assert_eq!(params.channels, 3);
    }

    #[test]
    fn element_form_xmp_and_a_single_channel_read_alike() {
        let packet =
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/">
              <hdrgm:Version>1.0</hdrgm:Version>
              <hdrgm:GainMapMax>3.5</hdrgm:GainMapMax>
              <hdrgm:HDRCapacityMax>3.5</hdrgm:HDRCapacityMax>
              <hdrgm:Gamma><rdf:Seq><rdf:li>2</rdf:li></rdf:Seq></hdrgm:Gamma>
              <hdrgm:BaseRenditionIsHDR>False</hdrgm:BaseRenditionIsHDR>
            </rdf:Description></rdf:RDF></x:xmpmeta>"#;

        let params = GainMapParams::from_xmp(packet).expect("readable");

        assert_eq!(params.channels, 1);
        assert_eq!((params.min, params.max, params.gamma), ([0.0; 3], [3.5; 3], [2.0; 3]));
        assert_eq!((params.offset_sdr, params.offset_hdr), ([1.0 / 64.0; 3], [1.0 / 64.0; 3]), "the defaults");
        assert_eq!((params.capacity_min, params.capacity_max, params.base_is_hdr), (0.0, 3.5, false));

        // Attribute form, one value, an HDR base.
        let attributes =
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description rdf:about="" xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" hdrgm:Version="1.0"
              hdrgm:GainMapMin="-1" hdrgm:GainMapMax="0" hdrgm:HDRCapacityMax="2" hdrgm:BaseRenditionIsHDR="True"/>
            </rdf:RDF></x:xmpmeta>"#;
        let hdr = GainMapParams::from_xmp(attributes).expect("readable");
        assert_eq!((hdr.min, hdr.max, hdr.base_is_hdr), ([-1.0; 3], [0.0; 3], true));
    }

    #[test]
    fn a_single_channel_iso_payload_is_spread_over_three() {
        // Version 0/0, no flags but the common denominator 4: headroom 0 and 8/4, then min -2/4, max 12/4, gamma
        // 4/4, offsets 1/4 and 2/4.
        let mut payload = vec![0, 0, 0, 0, 0x08];
        for value in [4_u32, 0, 8] {
            payload.extend(value.to_be_bytes());
        }
        for value in [-2_i32, 12, 4, 1, 2] {
            payload.extend(value.to_be_bytes());
        }

        let params = GainMapParams::from_iso(&payload).expect("readable");

        assert_eq!(params.channels, 1);
        assert_eq!((params.min, params.max, params.gamma), ([-0.5; 3], [3.0; 3], [1.0; 3]));
        assert_eq!((params.offset_sdr, params.offset_hdr), ([0.25; 3], [0.5; 3]));
        assert_eq!((params.capacity_min, params.capacity_max, params.base_is_hdr), (0.0, 2.0, false));
    }

    #[test]
    fn a_truncated_payload_and_garbage_xmp_are_refused_without_panicking() {
        for length in 0..HAWK_GAIN_MAP_ISO.len() {
            assert!(GainMapParams::from_iso(&HAWK_GAIN_MAP_ISO[..length]).is_err(), "{length} bytes");
        }
        let mut versioned = HAWK_GAIN_MAP_ISO.to_vec();
        versioned[1] = 1;
        assert_eq!(GainMapParams::from_iso(&versioned), Err(ParamsError::Iso("a minimum version other than 0")));

        assert!(matches!(GainMapParams::from_xmp(b"\xff not xml"), Err(ParamsError::Xmp(_))));
        let garbage = String::from_utf8_lossy(HAWK_GAIN_MAP_XMP).replace("2.418243", "lots");
        assert!(matches!(GainMapParams::from_xmp(garbage.as_bytes()), Err(ParamsError::Xmp(_))));
        let no_max = String::from_utf8_lossy(HAWK_GAIN_MAP_XMP).replace("hdrgm:HDRCapacityMax=\"2.299988\"", "");
        assert!(matches!(GainMapParams::from_xmp(no_max.as_bytes()), Err(ParamsError::Xmp(_))));
        let no_gamma = String::from_utf8_lossy(HAWK_GAIN_MAP_XMP).replace("0.647195", "-1");
        assert_eq!(GainMapParams::from_xmp(no_gamma.as_bytes()), Err(ParamsError::OutOfRange));
        // Every truncation of the packet either reads or is refused.
        for length in 0..HAWK_GAIN_MAP_XMP.len() {
            let _ = GainMapParams::from_xmp(&HAWK_GAIN_MAP_XMP[..length]);
        }
    }
}
