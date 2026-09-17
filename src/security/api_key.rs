//! API key authentication utilities.
//!
//! API key disimpan sebagai hash di database. Verifikasi menggunakan
//! constant-time comparison untuk mencegah timing attack.
