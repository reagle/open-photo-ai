//! The IPTC an export writes: of the source's Photoshop resource block, only the IPTC-IIM data and its digest.
//!
//! A JPEG's `APP13` holds a list of image resources, each `8BIM`, a 16-bit ID, a Pascal-string name padded to an even
//! length, a 32-bit size and the data padded to an even length. Resource `0x0404` is the IPTC-IIM record — keywords,
//! caption, creator, copyright, location — and `0x0425` the MD5 digest of it that tells a reader the record is current;
//! the IIM is not changed, so the digest stays true. Every other resource is left out: the Photoshop thumbnail
//! (`0x040C`) shows the unenhanced original, and slices, paths and print settings describe a different document.

use std::path::Path;

/// The IPTC-IIM record.
const IIM: u16 = 0x0404;
/// The MD5 digest of the IPTC-IIM record.
const DIGEST: u16 = 0x0425;

/// `block` reduced to its IPTC-IIM resource and digest, each byte for byte, or `None` when it does not walk as a
/// resource list or holds neither — logged at `debug` with the source, since a block like that has nothing to keep.
pub(crate) fn filter(block: &[u8], source: &Path) -> Option<Vec<u8>> {
    let kept = resources(block).map(|resources| {
        resources
            .into_iter()
            .filter(|resource| matches!(resource.id, IIM | DIGEST) && resource.signature == *b"8BIM")
            .fold(Vec::new(), |mut kept, resource| {
                kept.extend_from_slice(resource.bytes);
                // Restore a missing final padding byte, so a resource kept after it still starts on an even offset.
                if kept.len() % 2 == 1 {
                    kept.push(0);
                }
                kept
            })
    });
    match kept {
        Some(kept) if !kept.is_empty() => Some(kept),
        Some(_) => None,
        None => {
            tracing::debug!(op = "export", path = %source.display(), "IPTC left out: the resource block does not walk");
            None
        }
    }
}

/// The IPTC-IIM record of a Photoshop block, decoded: `(record, dataset, text)` in stored order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Iim(Vec<(u8, u8, String)>);

impl Iim {
    /// Every value of application-record dataset `number` (record 2), in order.
    pub(crate) fn all(&self, number: u8) -> Vec<String> {
        self.0
            .iter()
            .filter(|(record, dataset, _)| (*record, *dataset) == (2, number))
            .map(|(.., text)| text.clone())
            .collect()
    }

    /// The first value of application-record dataset `number`.
    pub(crate) fn first(&self, number: u8) -> Option<String> {
        self.all(number).into_iter().next()
    }
}

/// The IPTC-IIM record in `block`'s resource `0x0404`, or `None` when there is none or it does not walk.
///
/// Each dataset is `0x1C`, a record number, a dataset number, a big-endian 16-bit length and the data; an extended
/// (32-bit) length is refused, since no text dataset needs one. Text is UTF-8 when dataset 1:90 says so (`ESC % G`),
/// otherwise UTF-8 if it is valid and Latin-1 if not, which is what readers in practice assume.
pub(crate) fn iim(block: &[u8]) -> Option<Iim> {
    let resource = resources(block)?
        .into_iter()
        .find(|resource| resource.id == IIM && resource.signature == *b"8BIM")?;
    let data = resource_data(resource.bytes)?;

    let mut raw = Vec::new();
    let mut at = 0;
    while at < data.len() {
        if data[at] != 0x1C {
            // Trailing padding is zeros; anything else is not a dataset.
            if data[at..].iter().all(|&byte| byte == 0) {
                break;
            }
            return None;
        }
        let record = *data.get(at + 1)?;
        let number = *data.get(at + 2)?;
        let length = u16::from_be_bytes(data.get(at + 3..at + 5)?.try_into().ok()?);
        if length & 0x8000 != 0 {
            return None;
        }
        let value = data.get(at + 5..at + 5 + usize::from(length))?;
        raw.push((record, number, value));
        at += 5 + usize::from(length);
    }

    let utf8 = raw.iter().any(|(record, number, value)| (*record, *number) == (1, 90) && *value == b"\x1b%G");
    let text = |value: &[u8]| match std::str::from_utf8(value) {
        Ok(text) => text.to_owned(),
        Err(_) if utf8 => String::from_utf8_lossy(value).into_owned(),
        Err(_) => value.iter().map(|&byte| char::from(byte)).collect(),
    };
    Some(Iim(raw.into_iter().map(|(record, number, value)| (record, number, text(value))).collect()))
}

/// A resource's data, from its whole bytes.
fn resource_data(bytes: &[u8]) -> Option<&[u8]> {
    let name = usize::from(*bytes.get(6)?);
    let size_at = 6 + 1 + name + ((1 + name) & 1);
    let size = usize::try_from(u32::from_be_bytes(bytes.get(size_at..size_at + 4)?.try_into().ok()?)).ok()?;
    bytes.get(size_at + 4..(size_at + 4).checked_add(size)?)
}

/// One image resource: its signature, ID, and its whole bytes, header and padding included.
struct Resource<'a> {
    signature: [u8; 4],
    id: u16,
    bytes: &'a [u8],
}

/// The resources of `block`, or `None` at the first one that does not fit in it.
fn resources(block: &[u8]) -> Option<Vec<Resource<'_>>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < block.len() {
        let start = at;
        let signature: [u8; 4] = block.get(at..at + 4)?.try_into().ok()?;
        let id = u16::from_be_bytes(block.get(at + 4..at + 6)?.try_into().ok()?);
        // The name: a length byte and that many bytes, padded so the two together are even.
        let name = usize::from(*block.get(at + 6)?);
        let size_at = at + 6 + 1 + name + ((1 + name) & 1);
        let size = usize::try_from(u32::from_be_bytes(block.get(size_at..size_at + 4)?.try_into().ok()?)).ok()?;
        let data_end = (size_at + 4).checked_add(size).filter(|&end| end <= block.len())?;
        // The data is padded to an even length; the last resource's padding byte is sometimes missing.
        at = data_end + (size & 1);
        found.push(Resource { signature, id, bytes: &block[start..data_end.max(at.min(block.len()))] });
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One resource, as Photoshop writes it.
    fn resource(id: u16, name: &[u8], data: &[u8]) -> Vec<u8> {
        let mut bytes = b"8BIM".to_vec();
        bytes.extend(id.to_be_bytes());
        bytes.push(u8::try_from(name.len()).unwrap());
        bytes.extend(name);
        if (1 + name.len()) % 2 == 1 {
            bytes.push(0);
        }
        bytes.extend(u32::try_from(data.len()).unwrap().to_be_bytes());
        bytes.extend(data);
        if data.len() % 2 == 1 {
            bytes.push(0);
        }
        bytes
    }

    fn source() -> &'static Path {
        Path::new("/pictures/source.jpg")
    }

    #[test]
    fn the_iim_and_its_digest_are_kept_and_everything_else_dropped() {
        let iim = resource(IIM, b"", b"\x1c\x02\x19\x00\x04hawk");
        let digest = resource(DIGEST, b"", &[7; 16]);
        let block = [
            resource(0x03ED, b"", &[1; 16]), // resolution info
            iim.clone(),
            resource(0x040C, b"Thumb", &[0xFF; 101]), // the thumbnail, with an odd name and odd data
            digest.clone(),
            resource(0x0BB7, b"Path 1", b"x"), // a path
        ]
        .concat();

        assert_eq!(filter(&block, source()), Some([iim, digest].concat()));
    }

    #[test]
    fn a_block_with_neither_keeps_nothing() {
        let block = resource(0x040C, b"", &[0xFF; 10]);
        assert_eq!(filter(&block, source()), None);
        assert_eq!(filter(b"", source()), None);
    }

    #[test]
    fn a_missing_final_padding_byte_is_tolerated_and_restored() {
        let padded = resource(IIM, b"", b"\x1c\x02\x19\x00\x04hawk");
        let unpadded = &padded[..padded.len() - 1];
        assert_eq!(filter(unpadded, source()), Some(padded.clone()));
    }

    #[test]
    fn a_resource_that_overruns_the_block_drops_it_all() {
        let mut block = resource(IIM, b"", b"\x1c\x02\x19\x00\x04hawk");
        block[10..14].copy_from_slice(&1000_u32.to_be_bytes());
        assert_eq!(filter(&block, source()), None);
    }

    #[test]
    fn the_iim_record_is_read_with_its_character_set() {
        let utf8 = [
            &b"\x1c\x01\x5a\x00\x03\x1b%G"[..],
            b"\x1c\x02\x19\x00\x04hawk",
            b"\x1c\x02\x19\x00\x05caf\xc3\xa9",
        ]
        .concat();
        let block = resource(IIM, b"", &utf8);
        let record = iim(&block).expect("a record");
        assert_eq!(record.all(25), ["hawk", "café"]);
        assert_eq!(record.first(120), None);

        // Without 1:90, bytes that are not UTF-8 are Latin-1.
        let latin1 = resource(IIM, b"", b"\x1c\x02\x78\x00\x04caf\xe9");
        assert_eq!(iim(&latin1).expect("a record").first(120).as_deref(), Some("café"));

        // A dataset that overruns, an extended length, or a stray byte: no record.
        assert_eq!(iim(&resource(IIM, b"", b"\x1c\x02\x19\x00\x09hawk")), None);
        assert_eq!(iim(&resource(IIM, b"", b"\x1c\x02\x19\x80\x04hawk")), None);
        assert_eq!(iim(&resource(IIM, b"", b"\x1c\x02\x19\x00\x04hawkX")), None);
        assert_eq!(iim(&resource(0x040C, b"", b"\x1c\x02\x19\x00\x04hawk")), None);
    }

    #[test]
    fn truncations_and_byte_flips_never_panic() {
        let block = [
            resource(IIM, b"name", b"\x1c\x02\x19\x00\x04hawk"),
            resource(0x040C, b"", &[0xFF; 33]),
            resource(DIGEST, b"", &[7; 16]),
        ]
        .concat();
        for len in 0..block.len() {
            let _ = filter(&block[..len], source());
            let _ = iim(&block[..len]);
        }
        let mut flipped = block.clone();
        for at in 0..flipped.len() {
            for value in [0x00, 0xFF, 0x1C, 0x80, flipped[at] ^ 0x01] {
                let original = std::mem::replace(&mut flipped[at], value);
                let _ = filter(&flipped, source());
                let _ = iim(&flipped);
                flipped[at] = original;
            }
        }
    }
}
