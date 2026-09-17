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

> **⚠️ Peringatan:** Ini adalah Proof of Concept (POC). Midtrans, DOKU, dan NICEPAY
> menggunakan Sandbox, sedangkan Xendit menggunakan test-mode credential. Jangan gunakan
> di production.

---

## Apa Itu "Provider Simulator"? Apakah Ini Payment Gateway?

### ❌ Ini BUKAN Payment Gateway

Secure Payment Orchestrator **bukan** payment gateway seperti Midtrans, Xendit, Stripe, atau GoPay. Sistem ini **tidak**:
- Memproses pembayaran langsung dari customer
- Menyimpan data kartu kredit / PAN / CVV
- Berkomunikasi dengan bank atau processor
- Menyediakan halaman checkout / payment page
- Mengelola settlement atau payout

### ✅ Ini adalah Payment Orchestrator (Lapisan Abstraksi)

SPO adalah lapisan **orchestrator** yang duduk di **antara merchant dan payment gateway**:

```
┌──────────────┐         ┌──────────────────┐         ┌──────────────────────┐
│   Merchant   │ ───API──▶│  SPO Orchestrator │ ───API──▶│  Payment Gateway     │
│   App / Site │◀────────│   (Abstraction)   │◀────────│  (Midtrans/Xendit/   │
└──────────────┘         └──────────────────┘         │   Stripe/dll)        │
                                                      └──────────────────────┘
```

Fungsi SPO:
- **Menormalisasi** perbedaan API dari berbagai payment gateway
- **Melindungi** merchant dari transaksi ganda, webhook palsu, dan timeout
- **Menyediakan** audit trail terpusat untuk semua transaksi
- **Memudahkan** ganti atau nambah payment gateway tanpa mengubah kode merchant

### 🧪 Lalu Apa Itu "Provider Simulator"?

Provider simulator adalah **versi tiruan** dari payment gateway yang berjalan **lokal di komputer**.
Pada POC ini Alpha dipetakan ke Midtrans Sandbox, Beta ke Xendit test mode, dan Gamma ke
DOKU Sandbox.

| Aspek | Provider Simulator | Payment Gateway Sungguhan |
| --- | --- | --- |
| **API** | Sama contract-nya | Midtrans / Xendit / Stripe API |
| **Response** | Dikontrol (success/rejection/timeout) | Tergantung pembayaran customer |
| **Webhook** | Dikirim lokal | Dikirim dari server cloud |
| **Uang** | ❌ Tidak ada uang sungguhan | ✅ Memproses transaksi riil |
| **Koneksi** | Tidak digunakan oleh adapter riil | Midtrans, Xendit, DOKU, dan NICEPAY melalui internet |

**Tujuan simulator:**
1. **Membuktikan arsitektur** — bahwa pola adapter, retry, circuit breaker, dan webhook verification bekerja
2. **Testing tanpa risiko** — bisa simulasi timeout, error, dan skenario gagal tanpa kehilangan uang
3. **Demonstrasi** — reviewer bisa menjalankan seluruh sistem di laptop dengan satu perintah Docker

### 🔄 Analogi Sederhana

| Konsep | Analogi |
| --- | --- |
| **Payment Gateway** (Midtrans/Stripe) | Seperti **toko** yang menerima pembayaran dari pelanggan |
| **SPO Orchestrator** | Seperti **resepsionis** yang mengatur antrian dan memastikan tidak ada duplikasi |
| **Provider Simulator** | Seperti **role-play / latihan** di mana resepsionis berlatih tanpa toko sungguhan |

Jadi: **SPO bukan payment gateway, melainkan orchestrator yang mempermudah penggunaan payment gateway. Provider simulator adalah alat bantu untuk testing dan demonstrasi.**

---

## Fallback Antar-Payment Gateway

SPO dirancang agar dapat menggunakan payment gateway alternatif ketika gateway utama
bermasalah. Sebagai contoh, sebuah payment yang semula diarahkan ke Gateway A dapat
dialihkan ke Gateway B. Pengalihan ini disebut **failover** atau **provider fallback**.

Fallback bukan berarti setiap error dari Gateway A langsung dikirim ulang ke Gateway B.
Sistem harus memastikan lebih dahulu bahwa Gateway A belum membuat atau memproses
transaksi. Tanpa pemeriksaan tersebut, satu order dapat menghasilkan dua transaksi.

| Hasil dari Gateway A | Tindakan SPO | Boleh fallback ke B? |
| --- | --- | --- |
| Gateway tidak tersedia sebelum request dikirim, misalnya circuit breaker `OPEN` | Pilih provider sehat berikutnya | Ya |
| Koneksi gagal dan dapat dipastikan request belum sampai ke provider | Catat attempt gagal lalu pilih provider berikutnya | Ya |
| HTTP 429, 502, 503, atau 504 | Terapkan retry policy dan batas attempt | Ya, setelah kegagalan dipastikan aman |
| Validation error, authentication error, atau transaksi ditolak | Tandai sebagai kegagalan permanen | Tidak |
| Timeout setelah request mungkin sudah terkirim | Tandai `PENDING_RECONCILIATION` dan query status Gateway A | Belum |
| Rekonsiliasi memastikan transaksi tidak pernah tercipta | Buat attempt `FAILOVER` ke Gateway B | Ya |
| Rekonsiliasi menemukan transaksi sudah tercipta atau berhasil | Pertahankan transaksi pada Gateway A | Tidak |
| Status tetap tidak diketahui | Hentikan otomatisasi dan kirim ke manual review | Tidak |

Alur amannya adalah:

```text
Pilih Gateway A
      |
      +-- A tidak tersedia sebelum request dikirim --> pilih Gateway B
      |
      +-- request ditolak secara permanen ----------> FAILED
      |
      +-- timeout / hasil tidak diketahui
              |
              +--> PENDING_RECONCILIATION
                      |
                      +-- transaksi ada ------> tetap di Gateway A
                      +-- transaksi tidak ada -> failover ke Gateway B
                      +-- masih tidak pasti ---> manual review
```

Setiap perpindahan provider harus disimpan sebagai `payment_attempt` baru dengan
`attempt_type = FAILOVER`. Payment ID dan merchant reference di SPO tetap sama, sedangkan
provider, provider payment ID, dan payment URL dapat berubah. Gateway tujuan juga harus
mendukung currency, metode pembayaran, dan fitur yang dibutuhkan oleh transaksi tersebut.

> **Status implementasi POC:** struktur multi-provider, kontrak adapter, tipe attempt
> `FAILOVER`, dan state `PENDING_RECONCILIATION` sudah disiapkan. Provider selection,
> timeout handling, retry worker, reconciliation, dan circuit breaker belum selesai
> diimplementasikan secara end-to-end. Karena itu, fallback otomatis belum siap digunakan
> untuk transaksi production.

---

## Daftar Isi

- [Apa Itu Provider Simulator? Apakah Ini Payment Gateway?](#apa-itu-provider-simulator-apakah-ini-payment-gateway)
- [Fallback Antar-Payment Gateway](#fallback-antar-payment-gateway)
- [Untuk Siapa Aplikasi Ini?](#untuk-siapa-aplikasi-ini)
- [Masalah Apa yang Ingin Diselesaikan?](#masalah-apa-yang-ingin-diselesaikan)
- [Manfaat Aplikasi Ini](#manfaat-aplikasi-ini)
- [Fitur](#fitur)
- [Tech Stack](#tech-stack)
- [Arsitektur](#arsitektur)
- [Integrasi Midtrans (Provider Alpha)](#integrasi-midtrans-provider-alpha)
- [Integrasi Xendit (Provider Beta)](#integrasi-xendit-provider-beta)
- [Integrasi DOKU (Provider Gamma)](#integrasi-doku-provider-gamma)
- [Contoh Integrasi NICEPAY](#contoh-integrasi-nicepay)
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
- ✅ Adapter Midtrans Sandbox, Xendit test mode, dan DOKU Sandbox
- 🔶 Contoh adapter NICEPAY Sandbox untuk registration/create payment
- 🔶 Error mapping provider tersedia; orchestration classification belum terhubung
---

## Integrasi Midtrans (Provider Alpha)

Provider yang sebelumnya bernama Alpha sekarang mengidentifikasi diri sebagai `MIDTRANS`.
Implementasinya mengikuti [Midtrans API Quick Start](https://docs.midtrans.com/reference/quick-start-1):

- Create payment menggunakan `POST /snap/v1/transactions`.
- Status lookup/reconciliation menggunakan `GET /v2/{order_id}/status`.
- Autentikasi menggunakan HTTP Basic Auth dengan Server Key sebagai username dan password kosong.
- `merchant_reference` SPO dipakai sebagai Midtrans `order_id`.
- `gross_amount` dikirim sebagai integer; adapter saat ini hanya menerima `IDR`.
- `redirect_url` dari Snap menjadi `payment_url` SPO.

Tambahkan Server Key Sandbox ke environment lokal dan jangan commit nilainya:

```env
MIDTRANS_SERVER_KEY=your-sandbox-server-key
MIDTRANS_SNAP_BASE_URL=https://app.sandbox.midtrans.com
MIDTRANS_CORE_BASE_URL=https://api.sandbox.midtrans.com
MIDTRANS_TIMEOUT_SECONDS=10
```

| Status Midtrans | Status internal provider |
| --- | --- |
| `settlement`, atau `capture` + fraud `accept` | `COMPLETED` |
| `capture` + fraud `challenge` | `PENDING` |
| `capture` tanpa fraud status yang dikenal | `UNKNOWN` |
| `pending` | `PENDING` |
| `deny`, `cancel`, `expire`, `failure` | `FAILED` |
| `refund`, `partial_refund` | `REFUNDED` |
| Status lain | `UNKNOWN` dan perlu reconciliation/manual review |

> Integrasi ini baru tersedia pada layer adapter. Endpoint create payment SPO dan webhook
> processing belum diimplementasikan end-to-end, sehingga belum siap digunakan di production.
> Notifikasi Midtrans nantinya harus diverifikasi menggunakan signature Midtrans berbasis
> Server Key, bukan mekanisme HMAC simulator.

---

## Integrasi Xendit (Provider Beta)

Provider yang sebelumnya bernama Beta sekarang mengidentifikasi diri sebagai `XENDIT`.
Implementasinya menggunakan Payment Link/Invoice API dari
[Xendit API Reference](https://docs.xendit.co/apidocs):

- Create payment menggunakan `POST /v2/invoices`.
- Status lookup/reconciliation menggunakan `GET /v2/invoices/{invoice_id}`.
- Autentikasi menggunakan HTTP Basic Auth dengan Secret API Key sebagai username dan password kosong.
- `merchant_reference` SPO dipakai sebagai Xendit `external_id`.
- `invoice_url` dari Xendit menjadi `payment_url` SPO.
- Adapter saat ini dibatasi ke mata uang `IDR`.

Test mode ditentukan oleh API key Xendit, bukan base URL yang berbeda:

```env
XENDIT_SECRET_KEY=your-test-secret-key
XENDIT_CALLBACK_TOKEN=your-test-callback-token
XENDIT_BASE_URL=https://api.xendit.co
XENDIT_TIMEOUT_SECONDS=10
```

| Status invoice Xendit | Status internal provider |
| --- | --- |
| `PAID`, `SETTLED` | `COMPLETED` |
| `PENDING` | `PENDING` |
| `EXPIRED`, `FAILED` | `FAILED` |
| Status lain | `UNKNOWN` dan perlu reconciliation/manual review |

> Secret API Key tidak boleh dikirim ke frontend atau disimpan di repository. Callback
> Xendit nantinya harus diverifikasi menggunakan `X-CALLBACK-TOKEN`; webhook processing
> SPO masih belum diimplementasikan end-to-end.

---

## Integrasi DOKU (Provider Gamma)

Provider yang sebelumnya bernama Gamma sekarang mengidentifikasi diri sebagai `DOKU`.
Implementasinya menggunakan Checkout API dari
[DOKU Developer Documentation](https://developers.doku.com/):

- Create payment menggunakan `POST /checkout/v1/payment`.
- Status lookup/reconciliation menggunakan `GET /orders/v1/status/{invoice_number}`.
- `merchant_reference` SPO dipakai sebagai DOKU `invoice_number`.
- `payment.payment_url` dari DOKU menjadi `payment_url` SPO.
- Adapter saat ini dibatasi ke mata uang `IDR`.

Setiap request DOKU ditandatangani menggunakan `Client-Id`, `Request-Id`,
`Request-Timestamp`, `Request-Target`, SHA-256 `Digest` untuk request berbodi, dan signature
HMAC-SHA256 berbasis Secret Key.

```env
DOKU_CLIENT_ID=your-sandbox-client-id
DOKU_SECRET_KEY=your-sandbox-secret-key
DOKU_BASE_URL=https://api-sandbox.doku.com
DOKU_TIMEOUT_SECONDS=10
```

| Status DOKU | Status internal provider |
| --- | --- |
| `SUCCESS`, `PAID`, `SETTLED` | `COMPLETED` |
| `PENDING`, `PROCESSING` | `PENDING` |
| `FAILED`, `EXPIRED`, `CANCELLED`, `CANCELED` | `FAILED` |
| `REFUNDED`, `PARTIAL_REFUND` | `REFUNDED` |
| Status lain | `UNKNOWN` dan perlu reconciliation/manual review |

> Client ID dan Secret Key tidak boleh dikirim ke frontend atau disimpan di repository.
> Notification/webhook DOKU masih perlu diimplementasikan dan diverifikasi menggunakan
> komponen signature yang dikirim DOKU.

---

## Contoh Integrasi NICEPAY

NICEPAY ditambahkan sebagai provider keempat dengan nama `NICEPAY`. Contoh adapter ini
mengacu pada [NICEPAY API Documentation](https://docs.nicepay.co.id/nicepay-api) dan flow
Professional/Checkout v1.

- Registration menggunakan `POST /nicepay/api/v1.0/registration`.
- `merchant_reference` dipakai sebagai `referenceNo`.
- `amount` dipakai sebagai `amt` dan saat ini dibatasi ke `IDR`.
- `merchantToken` adalah SHA-256 dari `timeStamp + iMid + referenceNo + amt + merchantKey`.
- `paymentURL` dari response menjadi `payment_url` SPO.
- Payment method dapat dikonfigurasi; nilai contoh `01` digunakan untuk flow kartu/redirect.

```env
NICEPAY_IMID=your-sandbox-imid
NICEPAY_MERCHANT_KEY=your-sandbox-merchant-key
NICEPAY_BASE_URL=https://dev.nicepay.co.id
NICEPAY_PAY_METHOD=01
NICEPAY_TIMEOUT_SECONDS=10
```

> **Batasan contoh:** inquiry NICEPAY membutuhkan `tXid`, `referenceNo`, dan `amt` untuk
> membentuk request dan `merchantToken`. Kontrak `PaymentProvider::get_payment_status()`
> saat ini hanya menerima satu provider payment ID. Karena itu, create/registration sudah
> dicontohkan tetapi status inquiry belum diaktifkan dan akan mengembalikan
> `INQUIRY_CONTEXT_REQUIRED`. Application service perlu mengambil reference dan amount dari
> payment record atau kontrak status provider perlu diperluas sebelum reconciliation NICEPAY
> dapat digunakan.

Field customer, billing, serta payment-method-specific wajib disesuaikan dengan produk yang
diaktifkan pada akun merchant NICEPAY. Credential Sandbox tidak boleh disimpan di repository.

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
│   ├── 📁 planning/                 # BRD, PRD, WBS
│   ├── 📁 reports/                  # Progress report
│   ├── 📁 deployment/               # Production migration plan
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
    "provider": "MIDTRANS",
    "payment_url": "https://app.sandbox.midtrans.com/snap/v4/redirection/<token>",
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
| **BRD** | [documentation/planning/BRD-Secure-Payment-Orchestrator.md](./documentation/planning/BRD-Secure-Payment-Orchestrator.md) | Business Requirements Document |
| **PRD** | [documentation/planning/PRD-Secure-Payment-Orchestrator.md](./documentation/planning/PRD-Secure-Payment-Orchestrator.md) | Product Requirements Document |
| **WBS** | [documentation/planning/WBS-Secure-Payment-Orchestrator.md](./documentation/planning/WBS-Secure-Payment-Orchestrator.md) | Work Breakdown Structure |
| **API Contract** | [documentation/api/API-Contract-Secure-Payment-Orchestrator.md](./documentation/api/API-Contract-Secure-Payment-Orchestrator.md) | API endpoints detail |
| **OpenAPI Spec** | [documentation/api/openapi.yaml](./documentation/api/openapi.yaml) | OpenAPI 3.0.3 specification |
| **Architecture** | [documentation/architecture/Architecture-Secure-Payment-Orchestrator.md](./documentation/architecture/Architecture-Secure-Payment-Orchestrator.md) | Architecture & design decisions |
| **ERD** | [documentation/erd/ERD-Secure-Payment-Orchestrator.md](./documentation/erd/ERD-Secure-Payment-Orchestrator.md) | Entity Relationship Diagram |
| **Progress Report** | [documentation/reports/Progress-Report.md](./documentation/reports/Progress-Report.md) | Status audit proyek |
| **Migration Plan** | [documentation/deployment/Production-Migration-Plan.md](./documentation/deployment/Production-Migration-Plan.md) | Production migration plan |

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
| **M1 - Core Payment** | Dalam progress | Fondasi Axum, state machine, PostgreSQL, dan Midtrans adapter; handler payment belum selesai |
| **M2 - Security & Reliability** | 📅 Dalam Progress | HMAC webhook, duplicate protection, retry, timeout, reconciliation, structured logging |
| **M3 - Multi-Provider & Portfolio Ready** | Dalam progress | Adapter Midtrans/Xendit tersedia; circuit breaker, metrics, CI, Postman, threat model, dan demo belum selesai |

---

## License

Proyek ini dibuat untuk tujuan demonstrasi portfolio dan technical assessment.

---

*Dokumen diperbarui: 17 September 2026*
