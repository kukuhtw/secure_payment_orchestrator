# Architecture Document

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi | 1.0 |
| Status | Draft untuk Proof of Concept |
| Tanggal | 13 September 2026 |
| Tech Stack | Rust, Axum, Tokio, PostgreSQL, Redis |
| Architecture Style | Modular Monolith dengan domain-driven design |

---

## 1. Architecture Overview

Secure Payment Orchestrator (SPO) menggunakan arsitektur **modular monolith** yang terdiri dari beberapa layer dengan tanggung jawab terpisah. Arsitektur ini dipilih untuk POC karena:

1. **Sederhana** - Satu proses deployment tanpa kompleksitas microservices
2. **Domain isolation** - Layer domain tidak bergantung pada framework atau infrastruktur
3. **Testability** - Domain logic dapat diuji tanpa database atau HTTP
4. **Evolvability** - Di masa depan, setiap modul dapat dipisah menjadi service terpisah

### 1.1 High-Level Architecture Diagram

```mermaid
flowchart TB
    subgraph Client["External Clients"]
        Merchant["Merchant App"]
        Provider["Provider Simulators"]
    end

    subgraph API["API Layer (Axum)"]
        Auth["Authentication Middleware"]
        Idem["Idempotency Middleware"]
        Routes["Route Handlers"]
        Webhook["Webhook Handler"]
    end

    subgraph App["Application Layer"]
        PaymentSvc["Payment Service"]
        ProviderSvc["Provider Service"]
        WebhookSvc["Webhook Service"]
        ReconcileSvc["Reconciliation Service"]
        AuditSvc["Audit Service"]
    end

    subgraph Domain["Domain Layer"]
        StateMachine["Payment State Machine"]
        Entity["Payment / Attempt / Webhook Entities"]
        Rules["Transition Rules & Business Rules"]
    end

    subgraph Infra["Infrastructure Layer"]
        DBRepo["PostgreSQL Repository"]
        Cache["Redis Cache / Lock"]
        ProviderAdap["Provider Adapters"]
        EventBus["In-Memory Event Bus"]
    end

    subgraph Obs["Observability"]
        Logging["Structured Logging"]
        Metrics["Prometheus Metrics"]
        Health["Health / Readiness"]
    end

    Merchant --> Auth
    Merchant --> Routes
    Provider --> Webhook

    Auth --> App
    Idem --> Routes
    Routes --> PaymentSvc
    Webhook --> WebhookSvc

    PaymentSvc --> StateMachine
    PaymentSvc --> Entity
    PaymentSvc --> Rules
    ProviderSvc --> ProviderAdap
    WebhookSvc --> StateMachine

    PaymentSvc --> DBRepo
    PaymentSvc --> Cache
    ProviderSvc --> DBRepo
    WebhookSvc --> DBRepo
    ReconcileSvc --> DBRepo
    AuditSvc --> DBRepo

    PaymentSvc --> Logging
    PaymentSvc --> Metrics
```

### 1.2 Struktur Modul

```text
src/
├── api/               # Axum route handlers, middleware, DTO
│   ├── middleware/
│   ├── routes/
│   └── dto/
├── application/       # Service layer (orchestrates domain + infra)
│   ├── payment.rs
│   ├── provider.rs
│   ├── webhook.rs
│   ├── reconciliation.rs
│   └── audit.rs
├── domain/            # Business logic, entities, state machine
│   ├── payment.rs
│   ├── attempt.rs
│   ├── status.rs
│   ├── rules.rs
│   └── error.rs
├── infrastructure/    # Database, cache, external dependencies
│   ├── postgres/
│   ├── redis/
│   └── repositories/
├── providers/         # Provider adapter trait + implementations
│   ├── adapter.rs     (trait definition)
│   ├── alpha/
│   ├── beta/
│   └── gamma/
├── security/          # HMAC, API key hashing, constant-time comparison
│   ├── api_key.rs
│   ├── webhook_sig.rs
│   └── hash.rs
├── observability/     # Logging, metrics, health
│   ├── logging.rs
│   ├── metrics.rs
│   └── health.rs
└── config/            # Application configuration
    └── settings.rs
```

---

## 2. Layer Architecture

### 2.1 API Layer

**Responsibility:** Menangani HTTP request/response, autentikasi, idempotency, validasi input.

**Technology:** Axum web framework dengan Tokio async runtime.

**Key Components:**

- **Authentication Middleware** - Memvalidasi API key merchant dari header `X-API-Key`
- **Idempotency Middleware** - Mengecek dan menyimpan idempotency key untuk mencegah duplikasi
- **Request ID Middleware** - Memberikan `X-Request-ID` untuk setiap request
- **Route Handlers** - Endpoint REST untuk payment CRUD, webhook, dan operasional
- **DTO Layer** - Request/response struct dengan validasi Serde

**Endpoints:**

| Method | Path | Deskripsi |
| --- | --- | --- |
| `POST` | `/api/v1/payments` | Membuat pembayaran baru |
| `GET` | `/api/v1/payments/{id}` | Mendapatkan detail payment |
| `GET` | `/api/v1/payments` | Mencari payment (filter, pagination) |
| `POST` | `/api/v1/payments/{id}/reconcile` | Rekonsiliasi payment status uncertain |
| `POST` | `/api/v1/webhooks/{provider}` | Menerima webhook dari provider |
| `GET` | `/health` | Health check |
| `GET` | `/ready` | Readiness check |

### 2.2 Application Layer

**Responsibility:** Orchestrasi antara domain logic dan infrastruktur. Service layer memanggil domain untuk validasi, state machine, dan business rules, lalu menggunakan repository untuk persistensi.

**Key Services:**

- **PaymentService** - Create, get, search payment dengan idempotency
- **ProviderService** - Memanggil provider melalui adapter, mencatat attempt
- **WebhookService** - Verifikasi signature, duplicate detection, update status
- **ReconciliationService** - Query status ke provider untuk payment uncertain
- **AuditService** - Mencatat perubahan ke audit log

### 2.3 Domain Layer (Core)

**Responsibility:** Pure business logic tanpa ketergantungan pada framework atau infrastruktur. Layer ini adalah inti dari sistem.

**Key Components:**

- **Payment Entity** - Data model dengan validasi invariant
- **Status Enum** - State machine dengan transition rules
- **Money & Currency** - Value objects dengan validasi
- **Business Rules** - Idempotency rules, retry classification, validation

**Prinsip:**

1. Domain layer tidak memiliki dependensi ke `axum`, `sqlx`, atau `redis`
2. Semua domain logic dapat diuji dengan unit test tanpa database
3. State transition hanya diizinkan jika sesuai transition matrix

### 2.4 Infrastructure Layer

**Responsibility:** Implementasi konkret dari repository interface, cache, dan komunikasi eksternal.

**Components:**

- **PostgreSQL Repository** - SQLx dengan parameterized queries
- **Redis Cache** - Distributed lock untuk idempotency, caching configuration
- **Provider Adapters** - HTTP client ke provider simulator

### 2.5 Provider Layer

**Responsibility:** Adapter pattern untuk menormalkan perbedaan API provider.

**Key Design:**

```rust
#[async_trait]
pub trait PaymentProvider: Send + Sync {
    async fn create_payment(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError>;
    async fn get_payment_status(&self, payment_id: &str) -> Result<ProviderStatus, ProviderError>;
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
}
```

Setiap provider mengimplementasikan trait di atas. Provider internal Alpha dipetakan ke
Midtrans Sandbox melalui Snap API untuk create payment dan Core API untuk status lookup;
Beta dipetakan ke Xendit Payment Link/Invoice API, Gamma dipetakan ke DOKU Checkout API,
dan NICEPAY ditambahkan sebagai contoh adapter Checkout/Professional v1. Inquiry NICEPAY
masih memerlukan perluasan konteks status provider.

---

## 3. Data Flow

### 3.1 Payment Creation Flow

```mermaid
sequenceDiagram
    participant M as Merchant
    participant API as API Layer
    participant SVC as PaymentService
    participant DOM as Domain Layer
    participant DB as PostgreSQL
    participant CACHE as Redis
    participant PROV as Provider Adapter

    M->>API: POST /payments (Idempotency-Key, X-API-Key)
    API->>API: Validate API Key
    API->>API: Check idempotency (Redis lock)
    alt Idempotent request found
        API->>M: Return previous payment
    else New request
        API->>SVC: Create payment command
        SVC->>DOM: Validate & create Payment entity
        SVC->>DB: Save payment (PENDING)
        SVC->>CACHE: Release idempotency lock
        SVC->>PROV: Select & call provider
        PROV->>SVC: Provider response
        SVC->>DOM: Transition to PROCESSING
        SVC->>DB: Save attempt & update payment
        SVC-->>API: Payment response
        API-->>M: 201 Created (payment_url, status)
    end
```

### 3.2 Webhook Processing Flow

```mermaid
sequenceDiagram
    participant P as Provider
    participant API as Webhook Handler
    participant SVC as WebhookService
    participant DOM as Domain Layer
    participant SEC as Security
    participant DB as PostgreSQL

    P->>API: POST /webhooks/alpha (HMAC signature)
    API->>SEC: Verify HMAC signature
    alt Invalid signature
        API->>P: 401 Unauthorized
    else Valid signature
        API->>SVC: Process webhook
        SVC->>DB: Check duplicate event_id
        alt Duplicate event
            SVC->>DB: Mark as DUPLICATE
            SVC-->>API: 200 OK (already processed)
        else New event
            SVC->>SVC: Save webhook_event (RECEIVED)
            SVC->>DOM: Transition payment status
            SVC->>DB: Update payment & audit log
            SVC->>DB: Update webhook (PROCESSED)
            SVC-->>API: 200 OK
        end
        API-->>P: 200 OK
    end
```

### 3.3 Retry & Reconciliation Flow

```mermaid
flowchart TD
    A["Payment PROCESSING"] --> B{Provider response?}
    B -->|"Success"| C["SUCCESS"]
    B -->|"Rejection"| D["FAILED"]
    B -->|"Timeout / 5xx"| E{Retry eligible?}
    E -->|"Yes, retry count < max"| F["PENDING_RETRY"]
    F --> G["Wait (exponential backoff)"]
    G --> A
    E -->|"No, max retry reached"| H["PENDING_RECONCILIATION"]
    H --> I["Query provider status"]
    I -->|"Success"| C
    I -->|"Failed"| D
    I -->|"Uncertain"| J["MANUAL_REVIEW"]
```

---

### 3.4 Provider Fallback Flow

Provider fallback adalah pengalihan payment dari provider awal ke provider alternatif.
Fallback hanya dilakukan jika sistem mempunyai bukti yang cukup bahwa provider awal
belum membuat transaksi. Prinsip ini mencegah duplicate payment atau double charge ketika
response provider hilang setelah request berhasil diterima.

```mermaid
flowchart TD
    A["Select Gateway A"] --> B{"A available?"}
    B -->|"No, request not sent"| H["Select compatible Gateway B"]
    B -->|"Yes"| C["Send create-payment request"]
    C -->|"Success"| D["Continue with Gateway A"]
    C -->|"Permanent rejection"| E["FAILED - no fallback"]
    C -->|"Confirmed safe failure"| H
    C -->|"Timeout / ambiguous result"| F["PENDING_RECONCILIATION"]
    F --> G["Query status at Gateway A"]
    G -->|"Transaction exists"| D
    G -->|"Confirmed not created"| H
    G -->|"Still uncertain"| I["MANUAL_REVIEW"]
    H --> J["Create FAILOVER attempt"]
    J --> K["Send request to Gateway B"]
```

#### Fallback decision rules

| Condition | Decision | Reason |
| --- | --- | --- |
| Circuit breaker provider awal `OPEN` sebelum request dikirim | Fallback | Tidak ada transaksi yang mungkin tercipta di provider awal |
| Koneksi gagal sebelum request meninggalkan SPO | Fallback | Kegagalan dapat dipastikan aman |
| Provider rejection atau validation/authentication error | Tidak fallback | Error bersifat permanen atau request merchant harus diperbaiki |
| Timeout setelah request dikirim | Reconciliation dahulu | Provider mungkin sudah membuat transaksi |
| Reconciliation menyatakan transaksi tidak ditemukan | Fallback | Provider awal dipastikan tidak memiliki transaksi |
| Reconciliation menemukan transaksi | Tidak fallback | Melanjutkan di provider awal mencegah duplikasi |
| Reconciliation tetap ambigu | Manual review | Tidak ada dasar aman untuk membuat transaksi kedua |

#### Data and consistency requirements

1. Satu payment mempertahankan `payment_id`, `merchant_id`, `merchant_reference`, amount,
   dan currency yang sama selama failover.
2. Setiap provider call menghasilkan record `payment_attempts` tersendiri. Perpindahan
   provider menggunakan `attempt_type = FAILOVER` dan nomor attempt berikutnya.
3. Provider payment ID dan payment URL berasal dari attempt/provider yang aktif dan dapat
   berubah setelah failover.
4. Pemilihan provider alternatif harus memeriksa availability, priority, currency, metode
   pembayaran, dan capability yang dibutuhkan.
5. Lock per payment diperlukan agar retry, webhook, reconciliation, dan failover tidak
   berjalan bersamaan.
6. Late webhook dari provider lama tetap disimpan dan diverifikasi. Jika bertentangan
   dengan attempt aktif, payment ditahan untuk reconciliation/manual review, bukan langsung
   ditimpa.

#### Current implementation status

Dokumen ini mendeskripsikan target architecture. Pada POC saat ini, provider adapter,
multi-provider registration, `FAILOVER` attempt type, dan status
`PENDING_RECONCILIATION` sudah tersedia sebagai fondasi. Provider routing, timeout/retry
worker, reconciliation end-to-end, dan circuit breaker belum selesai. Oleh karena itu,
automatic fallback belum dianggap production-ready.

---

## 4. State Machine Design

### 4.1 Payment State Machine

Domain state machine adalah inti dari sistem yang memastikan transisi status selalu valid.

```mermaid
stateDiagram-v2
    [*] --> PENDING
    PENDING --> PROCESSING
    PROCESSING --> SUCCESS
    PROCESSING --> FAILED
    PROCESSING --> PENDING_RETRY
    PROCESSING --> PENDING_RECONCILIATION
    PENDING_RETRY --> PROCESSING
    PENDING_RECONCILIATION --> PROCESSING
    PENDING_RECONCILIATION --> SUCCESS
    PENDING_RECONCILIATION --> FAILED
    SUCCESS --> [*]
    FAILED --> [*]
```

### 4.2 Status Definitions

| Status | Deskripsi | Final |
| --- | --- | --- |
| `PENDING` | Payment dibuat, belum diproses | Tidak |
| `PROCESSING` | Sedang dikirim ke provider | Tidak |
| `SUCCESS` | Pembayaran berhasil | Ya |
| `FAILED` | Pembayaran gagal (provider reject / error permanen) | Ya |
| `PENDING_RETRY` | Akan di-retry dengan exponential backoff | Tidak |
| `PENDING_RECONCILIATION` | Status tidak pasti, perlu rekonsiliasi | Tidak |

### 4.3 Transition Rules Matrix

| From | To | Condition |
| --- | --- | --- |
| `PENDING` | `PROCESSING` | Payment siap dikirim ke provider |
| `PROCESSING` | `SUCCESS` | Provider mengonfirmasi success (sync atau webhook) |
| `PROCESSING` | `FAILED` | Provider reject, validation error, atau permanent error |
| `PROCESSING` | `PENDING_RETRY` | Timeout, 429, 502, 503, dan retry count < max |
| `PROCESSING` | `PENDING_RECONCILIATION` | Timeout dan retry count = max, status tidak pasti |
| `PENDING_RETRY` | `PROCESSING` | Waktu backoff terlewati, siap retry |
| `PENDING_RECONCILIATION` | `PROCESSING` | Rekonsiliasi menemukan payment masih pending |
| `PENDING_RECONCILIATION` | `SUCCESS` | Rekonsiliasi menemukan payment success |
| `PENDING_RECONCILIATION` | `FAILED` | Rekonsiliasi menemukan payment failed |

---

## 5. Security Architecture

### 5.1 Authentication & Authorization

```mermaid
flowchart LR
    A[Merchant Request] --> B{X-API-Key present?}
    B -->|"No"| C[401 Unauthorized]
    B -->|"Yes"| D[Hash API Key]
    D --> E{Match in DB?}
    E -->|"No"| C
    E -->|"Yes"| F{Key expired / revoked?}
    F -->|"Yes"| C
    F -->|"No"| G[Merchant identified]
    G --> H[Process Request]
```

**Key Security Decisions:**

1. **API Key Storage** - Hanya hash yang disimpan di database, tidak ada plaintext
2. **Constant-time Comparison** - Hash key dibandingkan menggunakan constant-time algorithm untuk mencegah timing attack
3. **Key Prefix** - Prefix disimpan untuk identifikasi key tanpa membandingkan hash penuh
4. **Per-Merchant Scoping** - Merchant hanya dapat mengakses data payment miliknya sendiri

### 5.2 Webhook Security (HMAC)

```mermaid
flowchart TB
    A[Webhook from Provider] --> B[Extract timestamp, signature]
    B --> C{Timestamp within tolerance?}
    C -->|"No (expired / future)"| D[401 Rejected]
    C -->|"Yes"| E[Compute HMAC SHA-256]
    E --> F{Signature match?}
    F -->|"No"| D
    F -->|"Yes"| G{Event ID already processed?}
    G -->|"Yes - duplicate"| H[200 OK - skip]
    G -->|"No - new event"| I[Process webhook]
```

**Webhook Security Features:**

1. **HMAC SHA-256** - Payload diverifikasi dengan secret key per provider
2. **Timestamp Tolerance** - Webhook diterima hanya dalam window waktu (default ±5 menit)
3. **Replay Protection** - Event ID unik dicek duplicate sebelum diproses
4. **Constant-time Comparison** - Signature comparison menggunakan constant-time

### 5.3 Secrets Management

| Secret | Storage | Notes |
| --- | --- | --- |
| API Key Hash | PostgreSQL | bcrypt/argon2 hash |
| Webhook Secret | Environment / Secret Store | Tidak di hardcode |
| Database URL | Environment | Via Docker Compose |
| Redis URL | Environment | Via Docker Compose |

---

## 6. Reliability Architecture

### 6.1 Idempotency Strategy

```mermaid
flowchart TD
    A[Request with Idempotency-Key] --> B[Acquire Redis Lock]
    B --> C{Idempotency Key exists?}
    C -->|"Yes - same hash"| D[Return existing payment]
    C -->|"Yes - different hash"| E[409 Conflict]
    C -->|"No"| F[Create payment in transaction]
    F --> G[Release Redis Lock]
    G --> H[Process with provider]
    H --> I[Store idempotency mapping]
```

**Idempotency Rules:**
- Key + merchant_id → UNIQUE constraint di database
- Same key + same payload → return existing (idempotent)
- Same key + different payload → 409 Conflict
- Keys memiliki masa berlaku (TTL) untuk garbage collection

### 6.2 Retry Policy

| Condition | Retryable | Notes |
| --- | --- | --- |
| HTTP 408 Timeout | Ya | Network timeout |
| HTTP 429 Too Many Requests | Ya | Rate limited |
| HTTP 502 Bad Gateway | Ya | Provider upstream error |
| HTTP 503 Service Unavailable | Ya | Provider temporary down |
| HTTP 504 Gateway Timeout | Ya | Provider upstream timeout |
| HTTP 4xx (other) | Tidak | Client / validation error |
| Network timeout (no response) | Tidak langsung | → Rekonsiliasi dulu |

**Backoff Strategy:**

```text
Attempt 1: initial request
Attempt 2: wait 1 second
Attempt 3: wait 2 seconds
Attempt 4: wait 4 seconds
Attempt 5: wait 8 seconds
Max attempts: 5
```

### 6.3 Circuit Breaker

State machine per provider:

```mermaid
stateDiagram-v2
    CLOSED --> OPEN : "failure threshold reached"
    OPEN --> HALF_OPEN : "timeout elapsed"
    HALF_OPEN --> CLOSED : "probe success"
    HALF_OPEN --> OPEN : "probe failure"
```

**Configuration:**
- Failure threshold: 5 consecutive failures
- Open state duration: 30 seconds
- Half-open probe: allow 1 request

### 6.4 Distributed Lock (Redis)

Digunakan untuk mencegah race condition pada operasi concurrent:

| Operasi | Lock Key | TTL | Notes |
| --- | --- | --- | --- |
| Create payment | `idempotency:{merchant_id}:{key}` | 30s | Mencegah duplicate create |
| Process webhook | `webhook:{event_id}` | 10s | Mencegah duplicate processing |
| Reconcile payment | `reconcile:{payment_id}` | 10s | Mencegah concurrent reconciliation |

---

## 7. Technology Stack

### 7.1 Runtime & Framework

| Komponen | Teknologi | Alasan |
| --- | --- | --- |
| Language | Rust 1.80+ | Performance, memory safety, type system |
| HTTP Framework | Axum 0.8 | Ergonomis, integrasi Tokio, tower middleware |
| Async Runtime | Tokio 1.x | Industri standar untuk async Rust |
| Serialization | Serde + serde_json | Performant, zero-copy JSON parsing |

### 7.2 Data & Cache

| Komponen | Teknologi | Alasan |
| --- | --- | --- |
| Database | PostgreSQL 16 | ACID compliance, JSONB, mature ecosystem |
| Database Driver | SQLx 0.8 | Async, compile-time query checking |
| Cache | Redis 7 | Distributed lock, config caching |
| Migration | SQLx migrate | Versioned, embeddable migrations |

### 7.3 Observability

| Komponen | Teknologi | Alasan |
| --- | --- | --- |
| Logging | Tracing + tracing-subscriber | Structured logging, span propagation |
| Metrics | Prometheus (via `metrics` crate) | OpenMetrics standard |
| Health | Axum route | Kubernetes liveness/readiness probe |

### 7.4 Security

| Komponen | Teknologi | Alasan |
| --- | --- | --- |
| HMAC | `hmac` + `sha2` crate | Webhook signature verification |
| Hashing | `argon2` or `bcrypt` crate | API key hashing |
| Constant-time | `subtle` crate | Timing-safe comparison |
| Secrets | Environment + Docker secrets | POC tidak menggunakan vault |

---

## 8. Infrastructure Diagram

```mermaid
flowchart TB
    subgraph Docker_Network["Docker Network"]
        API["SPO API\n(Rust / Axum)\nPort 8080"]
        PG[("PostgreSQL\nPort 5432")]
        RD[("Redis\nPort 6379")]
    end
    MIDTRANS["Midtrans Sandbox\nSnap + Core API"]
    XENDIT["Xendit Test Mode\nInvoice API"]
    DOKU["DOKU Sandbox\nCheckout API"]
    NICEPAY["NICEPAY Sandbox\nCheckout v1 Example"]

    Merchant["Merchant App"] --> API
    Provider["Provider Webhooks"] --> API

    API --> MIDTRANS
    API --> XENDIT
    API --> DOKU
    API --> NICEPAY
    API --> PG
    API --> RD
```

### 8.1 Deployment (Docker Compose)

```yaml
services:
  spo-api:
    build: .
    ports:
      - "8080:8080"
    environment:
      - DATABASE_URL=postgres://spo:spo@postgres:5432/spo
      - REDIS_URL=redis://redis:6379
      - MIDTRANS_SERVER_KEY=${MIDTRANS_SERVER_KEY}
      - MIDTRANS_SNAP_BASE_URL=https://app.sandbox.midtrans.com
      - MIDTRANS_CORE_BASE_URL=https://api.sandbox.midtrans.com
      - XENDIT_SECRET_KEY=${XENDIT_SECRET_KEY}
      - XENDIT_CALLBACK_TOKEN=${XENDIT_CALLBACK_TOKEN}
      - XENDIT_BASE_URL=https://api.xendit.co
      - DOKU_CLIENT_ID=${DOKU_CLIENT_ID}
      - DOKU_SECRET_KEY=${DOKU_SECRET_KEY}
      - DOKU_BASE_URL=https://api-sandbox.doku.com
      - NICEPAY_IMID=${NICEPAY_IMID}
      - NICEPAY_MERCHANT_KEY=${NICEPAY_MERCHANT_KEY}
      - NICEPAY_BASE_URL=https://dev.nicepay.co.id
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_started


  postgres:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: spo
      POSTGRES_USER: spo
      POSTGRES_PASSWORD: spo
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U spo"]
      interval: 5s

  redis:
    image: redis:7-alpine
```

---

## 9. Design Decisions (ADR)

### ADR-01: Modular Monolith vs Microservices

**Keputusan:** Modular Monolith

**Konteks:** POC dengan satu engineer dan target demonstrasi dalam 30 hari.

**Konsekuensi:**
- Positif: Deployment sederhana, testing mudah, latency minimal
- Negatif: Tidak bisa scale individual component, perlu refactor untuk production multi-service

### ADR-02: PostgreSQL vs SQLite

**Keputusan:** PostgreSQL

**Konteks:** Membutuhkan ACID transaction, row locking, JSONB, dan concurrent access.

**Konsekuensi:**
- Positif: Mature, fitur lengkap, production-ready
- Negatif: Lebih berat dari SQLite, perlu Docker

### ADR-03: Axum vs Actix-web

**Keputusan:** Axum

**Konteks:** Framework Rust modern dengan integrasi Tokio ecosystem.

**Konsekuensi:**
- Positif: Integrasi mulus dengan Tower middleware, ergonomis, type-safe
- Negatif: Ekosistem lebih kecil dari Actix, meskipun berkembang cepat

### ADR-04: Provider Adapter Pattern

**Keputusan:** Trait-based adapter dengan masing-masing provider sebagai modul terpisah

**Konteks:** Setiap provider memiliki API contract, authentication, dan error format berbeda.

**Konsekuensi:**
- Positif: Isolasi perubahan, mudah test dengan mock, mudah tambah provider baru
- Negatif: Overhead mapping per provider

---

## 10. Referensi

Dokumen ini mengacu pada:

| Dokumen | Deskripsi |
| --- | --- |
| [BRD-Secure-Payment-Orchestrator.md](../../BRD-Secure-Payment-Orchestrator.md) | Business Requirements Document |
| [PRD-Secure-Payment-Orchestrator.md](../../PRD-Secure-Payment-Orchestrator.md) | Product Requirements Document |
| [WBS-Secure-Payment-Orchestrator.md](../../WBS-Secure-Payment-Orchestrator.md) | Work Breakdown Structure |
| [ERD-Secure-Payment-Orchestrator.md](../erd/ERD-Secure-Payment-Orchestrator.md) | Entity Relationship Diagram |

## 11. Changelog

| Versi | Tanggal | Perubahan | Penulis |
| --- | --- | --- | --- |
| 1.0 | 13 September 2026 | Draft awal dokumen arsitektur | Engineering Team |

