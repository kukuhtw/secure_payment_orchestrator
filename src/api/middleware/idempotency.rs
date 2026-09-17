//! Idempotency middleware.
//!
//! Memeriksa `Idempotency-Key` header untuk mencegah duplikasi request.
//! Menggunakan Redis lock + database UNIQUE constraint.
