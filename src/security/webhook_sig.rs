//! Webhook HMAC SHA-256 signature verification.
//!
//! Format signature: `t=<unix_timestamp>,v1=<hmac_hex>`
//! Algoritma: HMAC SHA-256
//! Payload: timestamp + '.' + raw_body
