//! A plane an operation is handed rather than given a parameter for, held as operations hold injected values: shared,
//! and compared, hashed and printed by a signature of its values.
//!
//! Two callers: a lens blur's keep-sharp weight, and a matting's trimap.

use std::sync::Arc;

use imaging::plane::Plane;

/// A plane as an operation holds it: shared, and compared, hashed and printed by a signature of its values, computed
/// once.
#[derive(Clone)]
pub(crate) struct PlaneHandle {
    plane: Arc<Plane>,
    signature: u64,
}

impl PlaneHandle {
    pub(crate) fn new(plane: Arc<Plane>) -> Self {
        let size = [plane.width().to_le_bytes(), plane.height().to_le_bytes()].concat();
        let signature = digest(&size, plane.as_slice().iter().map(|value| value.to_bits().to_le_bytes()));

        Self { plane, signature }
    }

    /// `plane`, signed by the caller with `signature`: one that already names what the plane was drawn from, as a
    /// mask's does, so its millions of values need not be hashed again. Two planes share one exactly when they are
    /// equal, as with [`new`](Self::new), only where the caller's signature is that good.
    pub(crate) fn signed(plane: Arc<Plane>, signature: u64) -> Self {
        Self { plane, signature }
    }

    /// The plane itself.
    pub(crate) fn plane(&self) -> &Arc<Plane> {
        &self.plane
    }

    /// A 64-bit digest of the plane's size and values, stable across processes, which two planes share exactly when
    /// they are equal.
    pub(crate) fn signature(&self) -> u64 {
        self.signature
    }
}

/// How many bytes of values [`digest`] hashes at a time.
const CHUNK: usize = 1 << 20;

/// A 64-bit digest of `header` and then every one of `values`, as bytes, stable across processes: XXH3 over the header
/// and the XXH3 of each mebibyte of the values in turn.
///
/// A chunk at a time rather than the whole, so a 24-megapixel plane is never copied into a second buffer of its bytes
/// to be hashed: what is held is one chunk and eight bytes a chunk.
pub(crate) fn digest<const N: usize>(header: &[u8], values: impl IntoIterator<Item = [u8; N]>) -> u64 {
    let hash = |bytes: &[u8]| u64::from_str_radix(&rust_sak::crypto::xxh3_bytes(bytes), 16).unwrap_or_default();
    let mut digests = header.to_vec();
    let mut chunk = Vec::with_capacity(CHUNK);

    for value in values {
        chunk.extend_from_slice(&value);
        if chunk.len() >= CHUNK {
            digests.extend_from_slice(&hash(&chunk).to_le_bytes());
            chunk.clear();
        }
    }
    digests.extend_from_slice(&hash(&chunk).to_le_bytes());

    hash(&digests)
}

impl PartialEq for PlaneHandle {
    fn eq(&self, other: &Self) -> bool {
        self.signature == other.signature
    }
}

impl Eq for PlaneHandle {}

impl std::hash::Hash for PlaneHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.signature.hash(state);
    }
}

impl std::fmt::Debug for PlaneHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Plane({}x{}, {:016x})", self.plane.width(), self.plane.height(), self.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_tells_apart_any_one_value_and_the_size_across_chunks() {
        // Three chunks and a part, so a value in the last, partial one counts as one in the first does.
        let values: Vec<u32> = (0..800_000).collect();
        let of = |header: &[u8], values: &[u32]| digest(header, values.iter().map(|value| value.to_le_bytes()));
        let base = of(&[1, 2], &values);

        assert_eq!(of(&[1, 2], &values), base);
        assert_ne!(of(&[2, 1], &values), base);
        for index in [0, 300_000, 799_999] {
            let mut changed = values.clone();
            changed[index] += 1;
            assert_ne!(of(&[1, 2], &changed), base, "a change at {index} was not seen");
        }
        assert_ne!(of(&[1, 2], &values[..799_999]), base);
    }
}
