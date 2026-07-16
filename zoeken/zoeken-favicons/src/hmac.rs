//! HMAC-SHA256 helpers matching SearXNG `webutils.new_hmac` / `is_hmac_of`.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Hex-encoded HMAC-SHA256 of `value` under UTF-8 `secret_key`.
#[must_use]
pub fn new_hmac(secret_key: &str, value: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret_key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(value);
    hex::encode(mac.finalize().into_bytes())
}

/// Constant-time equality check against `new_hmac(secret_key, value)`.
#[must_use]
pub fn is_hmac_of(secret_key: &str, value: &[u8], hmac_to_check: &str) -> bool {
    let expected = new_hmac(secret_key, value);
    expected.len() == hmac_to_check.len()
        && expected
            .as_bytes()
            .iter()
            .zip(hmac_to_check.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_python_vector() {
        // hmac.new(b"secret", b"https://example.com/a.png", hashlib.sha256).hexdigest()
        let digest = new_hmac("secret", b"https://example.com/a.png");
        assert_eq!(
            digest,
            "c6dd0d682952f86d632ae5f8237c55e51e8ed658c5f5c33cd66c14e15ec9f33b"
        );
        assert!(is_hmac_of("secret", b"https://example.com/a.png", &digest));
        assert!(!is_hmac_of(
            "secret",
            b"https://example.com/a.png",
            "deadbeef"
        ));
        assert!(!is_hmac_of("other", b"https://example.com/a.png", &digest));
    }

    #[test]
    fn rejects_length_mismatch() {
        let digest = new_hmac("k", b"v");
        assert!(!is_hmac_of("k", b"v", &digest[..digest.len() - 1]));
    }
}
