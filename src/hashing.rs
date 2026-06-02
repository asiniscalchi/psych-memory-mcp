//! Small hashing helper shared by the epistemic id generators.

use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `input`.
pub fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}
