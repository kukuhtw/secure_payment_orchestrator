//! API key authentication utilities.
//!
//! API key disimpan sebagai hash di database. Verifikasi menggunakan
//! constant-time comparison untuk mencegah timing attack.

use uuid::Uuid;

/// Length of the stored lookup prefix — must match `api_keys.key_prefix VARCHAR(10)`.
pub const KEY_PREFIX_LEN: usize = 10;

/// Extract the token from an `Authorization: Bearer <token>` header value.
pub fn parse_bearer_token(header_value: &str) -> Option<&str> {
    header_value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

/// Derive the lookup prefix stored in `api_keys.key_prefix` from a full API key.
pub fn key_prefix(full_key: &str) -> &str {
    match full_key.char_indices().nth(KEY_PREFIX_LEN) {
        Some((byte_index, _)) => &full_key[..byte_index],
        None => full_key,
    }
}

/// Identity resolved from a validated API key. Attached to request extensions
/// by the authentication middleware so downstream handlers/services can scope
/// data access to the authenticated merchant.
#[derive(Debug, Clone)]
pub struct MerchantContext {
    pub merchant_id: Uuid,
    pub api_key_id: Uuid,
    pub permissions: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer_token() {
        assert_eq!(
            parse_bearer_token("Bearer sk_live_abc123def456"),
            Some("sk_live_abc123def456")
        );
    }

    #[test]
    fn rejects_non_bearer_scheme() {
        assert_eq!(parse_bearer_token("Basic dXNlcjpwYXNz"), None);
    }

    #[test]
    fn rejects_empty_bearer_token() {
        assert_eq!(parse_bearer_token("Bearer "), None);
    }

    #[test]
    fn derives_key_prefix() {
        assert_eq!(key_prefix("sk_live_abc123def456"), "sk_live_ab");
    }

    #[test]
    fn key_prefix_shorter_than_limit_returns_whole_key() {
        assert_eq!(key_prefix("short"), "short");
    }
}
