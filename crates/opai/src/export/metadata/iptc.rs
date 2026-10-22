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
    fn truncations_and_byte_flips_never_panic() {
        let block = [
            resource(IIM, b"name", b"\x1c\x02\x19\x00\x04hawk"),
            resource(0x040C, b"", &[0xFF; 33]),
            resource(DIGEST, b"", &[7; 16]),
        ]
        .concat();
        for len in 0..block.len() {
            let _ = filter(&block[..len], source());
        }
        let mut flipped = block.clone();
        for at in 0..flipped.len() {
            for value in [0x00, 0xFF, flipped[at] ^ 0x01] {
                let original = std::mem::replace(&mut flipped[at], value);
                let _ = filter(&flipped, source());
                flipped[at] = original;
            }
        }
    }
}
