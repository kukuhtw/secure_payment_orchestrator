# Secure Payment Orchestrator (SPO)

> **Status:** Proof of Concept | **Teknologi:** Rust, Axum, Tokio, PostgreSQL, Redis

---

## Untuk Siapa Aplikasi Ini?

| Persona | Peran | Kebutuhan |
| --- | --- | --- |
| **🏢 Merchant Developer** | Developer yang mengintegrasikan pembayaran ke aplikasi e-commerce atau fintech | API yang sederhana dan konsisten, tidak perlu berurusan dengan banyak provider berbeda |
| **👤 Customer** | Pengguna akhir yang melakukan pembayaran | Pengalaman bayar yang lancar, status transaksi yang jelas |
| **🔧 Operations Analyst** | Tim internal yang memantau dan mengelola transaksi | Kemampuan menelusuri status transaksi, menjalankan rekonsiliasi, melihat audit trail |
| **🔒 Security Reviewer** | Auditor keamanan yang memeriksa sistem | Verifikasi bahwa autentikasi, signature webhook, dan perlindungan data sudah benar |

---

## Masalah Apa yang Ingin Diselesaikan?

Integrasi pembayaran langsung dari aplikasi merchant ke beberapa payment provider menimbulkan sejumlah masalah serius:

### 🚫 1. Fragmentasi API Provider
Setiap provider (Midtrans, Xendit, Stripe, GoPay, dll) memiliki:
- Kontrak API yang berbeda
- Format status yang berbeda (SUCCESS/COMPLETED/settlement)
- Autentikasi yang berbeda (Basic Auth vs Bearer vs OAuth)
- Error code dan format yang berbeda

**Dampak:** Merchant harus menulis kode integrasi ulang setiap kali ganti atau menambah provider.

### 🚫 2. Risiko Transaksi Ganda
Ketika jaringan terputus setelah request dikirim tapi sebelum response diterima:
- Merchant tidak tahu apakah transaksi berhasil atau gagal
- Merchant mengirim ulang request → bisa menghasilkan **dua pembayaran** untuk satu order
- Kerugian finansial langsung dan komplain dari customer

### 🚫 3. Webhook Palsu / Berulang
Provider mengirim notifikasi via webhook, tetapi:
- Webhook bisa dipalsukan jika tidak ada signature verification
- Webhook yang sama bisa dikirim berulang (duplicate event)
- Webhook lama bisa di-replay oleh attacker

### 🚫 4. Status Transaksi Tidak Pasti
Ketika provider timeout setelah request terkirim:
- Apakah transaksi berhasil atau gagal? Tidak diketahui
- Provider sementara down → retry terus menerus → beban sistem
- Tanpa circuit breaker → request tetap dikirim ke provider yang rusak

### 🚫 5. Kesulitan Audit
- Perubahan status tidak tercatat → sulit debug masalah
- Percobaan ke provider tidak tersimpan → tidak tahu apa yang terjadi
- Tim operasional tidak punya alat untuk rekonsiliasi

---

## Manfaat Aplikasi Ini

| Manfaat | Penjelasan |
| --- | --- |
| **🔌 Satu API untuk Semua Provider** | Merchant hanya perlu mengenal satu API contract. Ganti provider? Cukup konfigurasi, tanpa perubahan kode merchant |
| **🛡️ Idempotency Garansi** | Request yang sama dengan key yang sama → diproses tepat satu kali. Tidak ada transaksi ganda |
| **📡 Webhook Aman** | Setiap webhook diverifikasi dengan HMAC SHA-256. Webhook palsu, expired, dan duplicate ditolak otomatis |
| **🔄 Retry Cerdas** | Error sementara (timeout, 502, 503) di-retry dengan exponential backoff. Error permanen (validation error) tidak di-retry |
| **⛑️ Circuit Breaker** | Jika provider gagal 5 kali berturut-turut, sistem berhenti mengirim request. Provider diuji berkala sampai pulih |
| **🔍 Status Tidak Pasti?** | Timeout setelah request terkirim → masuk reconciliation. Operations bisa query status ke provider |
| **📝 Audit Trail Lengkap** | Setiap perubahan status, percobaan ke provider, dan webhook tercatat dengan actor, waktu, dan metadata |
| **🏗️ Provider Adapter Pattern** | Provider baru cukup implementasi satu trait. Tidak perlu mengubah domain pembayaran |

---

Secure Payment Orchestrator adalah layanan backend berbasis **Rust** yang menyediakan satu antarmuka pembayaran terpadu untuk menghubungkan merchant dengan beberapa payment provider. Sistem ini menangani pemilihan provider, pencegahan transaksi ganda (idempotency), retry aman, failover terbatas, webhook terverifikasi (HMAC), rekonsiliasi, dan audit trail lengkap.

> **⚠️ Peringatan:** Ini adalah Proof of Concept (POC) yang menggunakan **provider simulasi** dan **tidak memproses uang sungguhan**. Jangan gunakan di production.

---

## Daftar Isi

- [Untuk Siapa Aplikasi Ini?](#untuk-siapa-aplikasi-ini)
- [Masalah Apa yang Ingin Diselesaikan?](#masalah-apa-yang-ingin-diselesaikan)
- [Manfaat Aplikasi Ini](#manfaat-aplikasi-ini)
- [Fitur](#fitur)
- [Tech Stack](#tech-stack)
- [Arsitektur](#arsitektur)
- [Struktur Proyek](#struktur-proyek)
- [Prasyarat](#prasyarat)
- [Instalasi & Menjalankan](#instalasi--menjalankan)
- [API Documentation](#api-documentation)
- [Contoh Penggunaan](#contoh-penggunaan)
- [Dokumentasi Lengkap](#dokumentasi-lengkap)
- [Keamanan](#keamanan)
- [Roadmap](#roadmap)
- [License](#license)

---

## Fitur

### Core Payment
- ✅ Satu API konsisten untuk semua provider
- ✅ Provider selection berdasarkan availability dan priority
- ✅ Penyimpanan payment attempt untuk audit trail

### Idempotency & Reliability
- ✅ Idempotency key dengan database constraint + Redis lock
- ✅ Request hash untuk deteksi perubahan payload
- ✅ Timeout, retry dengan exponential backoff (maks 5 attempt)
- ✅ Circuit breaker per-provider (5 failures → OPEN 30 detik)
- ✅ Reconciliation untuk status tidak pasti

### Security
- ✅ API key authentication (hash + constant-time comparison)
- ✅ Webhook HMAC SHA-256 dengan timestamp tolerance (±5 menit)
- ✅ Replay protection (event ID unique constraint)
- ✅ Per-merchant data isolation
- ✅ Structured logging dengan redaction data sensitif

### Observability
- ✅ Structured JSON logging dengan Tracing
- ✅ Prometheus metrics (counter, histogram)
- ✅ Health check & readiness probe
- ✅ Request ID & correlation ID untuk tracing

### Provider Adapter
- ✅ Pattern trait-based untuk isolasi provider
- ✅ 3 provider simulator: Alpha, Beta, Gamma
- ✅ Error mapping dan classification (retryable vs non-retryable)
---

## Struktur Proyek

```
spo/
├── 📁 src/                          # Source code Rust
│   ├── 📁 api/                      # Route handlers, middleware, DTO
│   ├── 📁 application/              # Service layer
│   ├── 📁 domain/                   # Entitas, state machine, rules
│   ├── 📁 infrastructure/           # PostgreSQL, Redis, repositories
│   ├── 📁 providers/                # Provider adapter trait + simulators
│   ├── 📁 security/                 # HMAC, hashing, constant-time
│   ├── 📁 observability/            # Logging, metrics, health
│   └── 📁 config/                   # Application configuration
├── 📁 simulators/                   # Provider simulator services
├── 📁 migrations/                   # SQLx database migrations
├── 📁 documentation/                # Dokumentasi proyek
│   ├── 📁 api/                      # API Contract + OpenAPI spec
│   ├── 📁 architecture/             # Architecture document
│   ├── 📁 database/                 # Database migrations & schema
│   └── 📁 erd/                      # ERD document
├── 📁 tests/                        # Integration + concurrency tests
├── Cargo.toml                       # Rust project manifest
├── Dockerfile                       # Docker image build
├── docker-compose.yml               # Local development environment
├── .env.example                     # Environment variables template
└── README.md                        # This file
```

---

## Prasyarat

Sebelum menjalankan proyek, pastikan telah menginstal:

- **Rust** 1.80+ (gunakan [rustup](https://rustup.rs/))
- **Docker** & **Docker Compose** (untuk PostgreSQL, Redis, dan simulator)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verifikasi
rustc --version   # ≥ 1.80
cargo --version
```

---

## Instalasi & Menjalankan

### 1. Clone & Setup

```bash
git clone <repo-url>
cd spo
cp .env.example .env
```

### 2. Jalankan Dependencies

```bash
docker-compose up -d postgres redis
```

### 3. Jalankan Migrasi Database

```bash
cargo sqlx migrate run
```

### 4. Jalankan Aplikasi (Development)

```bash
cargo run
```

### 5. Verifikasi

```bash
curl http://localhost:8080/health
curl http://localhost:8080/ready

curl -X POST http://localhost:8080/api/v1/payments \
## API Documentation

### Endpoint Summary

| Method | Endpoint | Fungsi | Auth |
| --- | --- | --- | --- |
| `POST` | `/api/v1/payments` | Membuat pembayaran baru | Merchant API Key |
| `GET` | `/api/v1/payments/{payment_id}` | Detail pembayaran | Merchant API Key |
| `GET` | `/api/v1/payments` | Mencari pembayaran | Merchant API Key |
| `POST` | `/api/v1/payments/{payment_id}/cancel` | Membatalkan pembayaran | Merchant API Key |
| `POST` | `/api/v1/payments/{payment_id}/retry` | Retry manual | Operations Key |
| `POST` | `/api/v1/payments/{payment_id}/reconcile` | Rekonsiliasi status | Operations Key |
| `POST` | `/api/v1/webhooks/{provider}` | Menerima webhook provider | HMAC Signature |
| `GET` | `/health` | Liveness check | Tidak |
| `GET` | `/ready` | Readiness check | Tidak |
| `GET` | `/metrics` | Prometheus metrics | Internal |

📖 **API Contract:** [documentation/api/API-Contract-Secure-Payment-Orchestrator.md](./documentation/api/API-Contract-Secure-Payment-Orchestrator.md)  
📖 **OpenAPI:** [documentation/api/openapi.yaml](./documentation/api/openapi.yaml)

### Autentikasi

```
Authorization: Bearer sk_live_abc123def456
```

### Idempotency

Endpoint mutasi (`POST`) memerlukan header:

```
Idempotency-Key: checkout-order-10001
```

---

## Contoh Penggunaan

### Create Payment

```bash
curl -X POST http://localhost:8080/api/v1/payments \
  -H "Authorization: Bearer sk_live_demo_key_001" \
  -H "Idempotency-Key: order-001" \
  -H "Content-Type: application/json" \
  -d '{
    "merchant_reference": "ORDER-001",
    "amount": 150000,
    "currency": "IDR",
    "description": "Pembayaran ORDER-001"
  }'
```

Response:
```json
{
  "data": {
    "payment_id": "pay_01J8ZVX8B8",
    "merchant_reference": "ORDER-001",
    "status": "PENDING",
    "amount": 150000,
    "currency": "IDR",
    "provider": "ALPHA",
    "payment_url": "http://localhost:9091/pay/pay_01J8ZVX8B8",
    "created_at": "2026-09-13T10:00:00Z"
  }
}
```

### Get Payment

```bash
curl http://localhost:8080/api/v1/payments/pay_01J8ZVX8B8 \
  -H "Authorization: Bearer sk_live_demo_key_001"
```

### Reconciliation

```bash
curl -X POST http://localhost:8080/api/v1/payments/pay_01J8ZVX8B8/reconcile \
  -H "Authorization: Bearer sk_live_ops_key_001" \
  -H "Idempotency-Key: reconcile-001" \
  -H "Content-Type: application/json"
```

---

## Dokumentasi Lengkap

| Dokumen | Lokasi | Deskripsi |
| --- | --- | --- |
| **BRD** | [BRD-Secure-Payment-Orchestrator.md](./BRD-Secure-Payment-Orchestrator.md) | Business Requirements Document |
| **PRD** | [PRD-Secure-Payment-Orchestrator.md](./PRD-Secure-Payment-Orchestrator.md) | Product Requirements Document |
| **WBS** | [WBS-Secure-Payment-Orchestrator.md](./WBS-Secure-Payment-Orchestrator.md) | Work Breakdown Structure |
| **API Contract** | [documentation/api/API-Contract-Secure-Payment-Orchestrator.md](./documentation/api/API-Contract-Secure-Payment-Orchestrator.md) | API endpoints detail |
| **OpenAPI Spec** | [documentation/api/openapi.yaml](./documentation/api/openapi.yaml) | OpenAPI 3.0.3 specification |
| **Architecture** | [documentation/architecture/Architecture-Secure-Payment-Orchestrator.md](./documentation/architecture/Architecture-Secure-Payment-Orchestrator.md) | Architecture & design decisions |
| **ERD** | [documentation/erd/ERD-Secure-Payment-Orchestrator.md](./documentation/erd/ERD-Secure-Payment-Orchestrator.md) | Entity Relationship Diagram |

---

## Keamanan

1. **API Key hash** — Tidak disimpan plaintext, hanya hash (argon2/bcrypt)
2. **Constant-time comparison** — Mencegah timing attack
3. **Webhook HMAC SHA-256** — Signature dengan secret key per provider
4. **Timestamp tolerance** — Webhook hanya valid dalam window ±5 menit
5. **Replay protection** — Event ID unique constraint
6. **Data isolation** — Merchant hanya akses data miliknya sendiri
7. **Input validation** — Semua input divalidasi dan dibatasi ukurannya
8. **Parameterized queries** — SQLx prevents SQL injection
9. **Log redaction** — Data sensitif di-redact sebelum dicatat
10. **No secrets in repo** — Semua dari environment variable

---

## Roadmap

| Milestone | Target | Fitur |
| --- | --- | --- |
| **M1 - Core Payment** | ✅ Selesai | Axum API, state machine, PostgreSQL, create/get payment, Alpha simulator, idempotency, audit log |
| **M2 - Security & Reliability** | 📅 Dalam Progress | HMAC webhook, duplicate protection, retry, timeout, reconciliation, structured logging |
| **M3 - Multi-Provider & Portfolio Ready** | 📅 Planned | Beta & Gamma simulator, circuit breaker, Redis lock, metrics, CI, OpenAPI, Postman, threat model, demo script |

---

## License

Proyek ini dibuat untuk tujuan demonstrasi portfolio dan technical assessment.

---

*Dokumen diperbarui: 13 September 2026*