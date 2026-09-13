//! Authentication middleware.
//!
//! Validates `Authorization: Bearer <api_key>` header.
//! API key dibandingkan menggunakan constant-time comparison terhadap hash di database.