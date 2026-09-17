//! Hashing utilities for API key storage.
//!
//! Menggunakan argon2 untuk hashing dengan constant-time comparison.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;

/// Hash a plaintext secret (API key) using Argon2id with a random salt.
pub fn hash_secret(plain: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(plain.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a plaintext secret against a stored Argon2 PHC hash string.
///
/// Uses `argon2`'s constant-time comparison internally, preventing timing
/// attacks. Returns `false` (instead of erroring) for any malformed hash or
/// mismatch so callers can treat every failure mode identically.
pub fn verify_secret(plain: &str, hash: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };

    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed_hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_and_verifies_roundtrip() {
        let hash = hash_secret("sk_live_demo_key_001").expect("hash should succeed");
        assert!(verify_secret("sk_live_demo_key_001", &hash));
    }

    #[test]
    fn rejects_wrong_secret() {
        let hash = hash_secret("sk_live_demo_key_001").expect("hash should succeed");
        assert!(!verify_secret("sk_live_wrong_key", &hash));
    }

    #[test]
    fn rejects_malformed_hash() {
        assert!(!verify_secret(
            "sk_live_demo_key_001",
            "not-a-valid-phc-hash"
        ));
    }
}
