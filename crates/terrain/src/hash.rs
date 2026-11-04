//! FNV-1a 64, the hash of SPEC-002 used for tile bodies and the pack content hash.

const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 of `bytes`.
///
/// ```
/// assert_eq!(terrain::fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
/// ```
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = OFFSET_BASIS;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(PRIME);
    }
    h
}
