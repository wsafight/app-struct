use sha2::{Digest, Sha256};

#[must_use]
pub fn sha256_hex(content: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(content.as_ref()))
}

#[must_use]
pub fn sha256_tagged(content: impl AsRef<[u8]>) -> String {
    format!("sha256:{}", sha256_hex(content))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_stable_hex_and_tagged_digests() {
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(sha256_hex(b"abc"), expected);
        assert_eq!(sha256_tagged(b"abc"), format!("sha256:{expected}"));
    }
}
