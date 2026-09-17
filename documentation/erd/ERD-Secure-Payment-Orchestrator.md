# Entity Relationship Diagram (ERD)

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi | 1.0 |
| Status | Draft untuk Proof of Concept |
| Tanggal | 13 September 2026 |
| Database | PostgreSQL |
| Tools Referensi | Mermaid.js, DBML |

---

## 1. Diagram ERD (Mermaid)

Berikut adalah ERD yang merepresentasikan hubungan antar entitas utama dalam sistem Secure Payment Orchestrator:

```mermaid
erDiagram
    MERCHANTS {
        uuid id PK
        string name
        string merchant_code UK
        string contact_email
        string status
        jsonb supported_currencies
        jsonb config
        timestamptz created_at
        timestamptz updated_at
    }

    API_KEYS {
        uuid id PK
        uuid merchant_id FK
        string key_hash
        string key_prefix UK
        string label
        string permissions
        timestamptz expires_at
        timestamptz created_at
        timestamptz revoked_at
    }

    PAYMENTS {
        uuid id PK
        uuid merchant_id FK
        string idempotency_key UK
        string merchant_reference
        string currency
        bigint amount
        string description
        string status
        string provider
        string failure_reason
        string payment_url
        uuid created_by_api_key_id FK
        timestamptz created_at
        timestamptz updated_at
        timestamptz completed_at
    }

    PAYMENT_ATTEMPTS {
        uuid id PK
        uuid payment_id FK
        string provider
        string provider_payment_id
        string provider_status
        string status
        string attempt_type
        int attempt_number
        jsonb request_snapshot
        jsonb response_snapshot
        int http_status_code
        string error_code
        string error_message
        bigint duration_ms
        timestamptz started_at
        timestamptz completed_at
        timestamptz created_at
    }

    WEBHOOK_EVENTS {
        uuid id PK
        uuid payment_id FK
        string provider
        string event_id UK
        string event_type
        string signature
        string raw_body
        string verification_status
        string processing_status
        text verification_error
        timestamptz received_at
        timestamptz verified_at
        timestamptz processed_at
    }

    AUDIT_LOGS {
        uuid id PK
        uuid merchant_id FK
        uuid payment_id FK
        uuid entity_id
        string entity_type
        string action
        string actor
        string field_name
        text old_value
---

## 2. Deskripsi Entitas

### 2.1 `merchants`

Merupakan entitas utama yang merepresentasikan merchant / pengguna sistem.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `id` | `UUID` | `PK` | Identitas unik merchant |
| `name` | `VARCHAR(255)` | `NOT NULL` | Nama merchant |
| `merchant_code` | `VARCHAR(50)` | `UNIQUE, NOT NULL` | Kode unik merchant untuk identifikasi |
| `contact_email` | `VARCHAR(255)` | `NOT NULL` | Email kontak merchant |
| `status` | `VARCHAR(20)` | `NOT NULL, DEFAULT 'ACTIVE'` | Status merchant (ACTIVE, SUSPENDED, INACTIVE) |
| `supported_currencies` | `JSONB` | `NOT NULL` | Daftar currency yang didukung |
| `config` | `JSONB` |  | Konfigurasi khusus merchant (webhook_url, dll) |
| `created_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu pembuatan |
| `updated_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu terakhir diperbarui |

### 2.2 `api_keys`

API key untuk autentikasi merchant saat mengakses API.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `id` | `UUID` | `PK` | Identitas unik API key |
| `merchant_id` | `UUID` | `FK -> merchants.id, NOT NULL` | Merchant pemilik key |
| `key_hash` | `VARCHAR(255)` | `NOT NULL` | Hash dari API key (tidak disimpan plaintext) |
| `key_prefix` | `VARCHAR(10)` | `UNIQUE, NOT NULL` | Prefix key untuk identifikasi (misal: `sk_live_abc...`) |
| `label` | `VARCHAR(100)` |  | Label untuk membedakan multiple key |
| `permissions` | `VARCHAR(50)` | `NOT NULL, DEFAULT 'standard'` | Level permission key |
| `expires_at` | `TIMESTAMPTZ` |  | Waktu kadaluwarsa key |
| `created_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu pembuatan |
| `revoked_at` | `TIMESTAMPTZ` |  | Waktu pencabutan key |

### 2.3 `payments`

Entitas inti yang mencatat setiap transaksi pembayaran.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `id` | `UUID` | `PK` | Identitas unik payment |
| `merchant_id` | `UUID` | `FK -> merchants.id, NOT NULL` | Merchant pemilik payment |
| `idempotency_key` | `VARCHAR(255)` | `UNIQUE, NOT NULL` | Key untuk mencegah duplikasi request |
| `merchant_reference` | `VARCHAR(255)` | `NOT NULL` | Referensi dari merchant (order ID, dll) |
| `currency` | `VARCHAR(3)` | `NOT NULL` | Kode currency ISO 4217 (IDR, SGD) |
| `amount` | `BIGINT` | `NOT NULL, CHECK (amount > 0)` | Jumlah pembayaran dalam satuan terkecil (sen) |
| `description` | `TEXT` |  | Deskripsi pembayaran |
| `status` | `VARCHAR(30)` | `NOT NULL` | Status terkini (lihat state machine) |
| `provider` | `VARCHAR(50)` |  | Provider yang menangani payment |
| `failure_reason` | `TEXT` |  | Alasan kegagalan |
| `payment_url` | `VARCHAR(500)` |  | URL redirect ke provider |
| `created_by_api_key_id` | `UUID` | `FK -> api_keys.id` | API key yang digunakan |
| `created_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu pembuatan |
| `updated_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu terakhir diperbarui |
| `completed_at` | `TIMESTAMPTZ` |  | Waktu penyelesaian (SUCCESS/FAILED) |
        text new_value
        jsonb metadata
        inet ip_address
        string correlation_id
        timestamptz created_at
    }
**Status Transitions:**

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

### 2.4 `payment_attempts`

Mencatat setiap percobaan pengiriman payment ke provider.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `id` | `UUID` | `PK` | Identitas unik attempt |
| `payment_id` | `UUID` | `FK -> payments.id, NOT NULL` | Payment yang dicoba |
| `provider` | `VARCHAR(50)` | `NOT NULL` | Provider yang dipanggil |
| `provider_payment_id` | `VARCHAR(255)` |  | ID payment dari provider |
| `provider_status` | `VARCHAR(50)` |  | Status dari response provider |
| `status` | `VARCHAR(30)` | `NOT NULL` | Status attempt (SUCCESS, FAILED, TIMEOUT) |
| `attempt_type` | `VARCHAR(20)` | `NOT NULL` | Tipe attempt (INITIAL, RETRY, RECONCILIATION, FAILOVER) |
| `attempt_number` | `INTEGER` | `NOT NULL` | Urutan attempt (1, 2, 3, ...) |
| `request_snapshot` | `JSONB` |  | Snapshot request yang dikirim |
| `response_snapshot` | `JSONB` |  | Snapshot response yang diterima |
| `http_status_code` | `INTEGER` |  | HTTP status code dari provider |
| `error_code` | `VARCHAR(100)` |  | Kode error dari provider |
| `error_message` | `TEXT` |  | Pesan error |
| `duration_ms` | `BIGINT` |  | Durasi request dalam milidetik |
| `started_at` | `TIMESTAMPTZ` |  | Waktu mulai request |
| `completed_at` | `TIMESTAMPTZ` |  | Waktu selesai request |
| `created_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu pencatatan |

### 2.5 `webhook_events`

Mencatat setiap webhook yang diterima dari provider.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `id` | `UUID` | `PK` | Identitas unik webhook event |
| `payment_id` | `UUID` | `FK -> payments.id, NOT NULL` | Payment terkait |
| `provider` | `VARCHAR(50)` | `NOT NULL` | Provider pengirim |
| `event_id` | `VARCHAR(255)` | `UNIQUE, NOT NULL` | ID unik event dari provider |
| `event_type` | `VARCHAR(50)` | `NOT NULL` | Tipe event (payment.success, payment.failed) |
| `signature` | `VARCHAR(500)` | `NOT NULL` | Signature HMAC untuk verifikasi |
| `raw_body` | `TEXT` | `NOT NULL` | Raw body webhook |
| `verification_status` | `VARCHAR(20)` | `NOT NULL, DEFAULT 'PENDING'` | Status verifikasi (PENDING, VALID, INVALID) |
| `processing_status` | `VARCHAR(20)` | `NOT NULL, DEFAULT 'PENDING'` | Status pemrosesan (PENDING, PROCESSED, DUPLICATE, FAILED) |
| `verification_error` | `TEXT` |  | Detail error verifikasi jika gagal |
| `received_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu diterima |
| `verified_at` | `TIMESTAMPTZ` |  | Waktu diverifikasi |
| `processed_at` | `TIMESTAMPTZ` |  | Waktu diproses |

    IDEMPOTENCY_KEYS {
        string idempotency_key PK
        uuid merchant_id FK
        string request_hash
        uuid payment_id FK
        string response_status_code
### 2.6 `audit_logs`

Menyediakan trail audit untuk setiap perubahan data penting.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `id` | `UUID` | `PK` | Identitas unik audit log |
| `merchant_id` | `UUID` | `FK -> merchants.id` | Merchant terkait (nullable untuk system action) |
| `payment_id` | `UUID` | `FK -> payments.id` | Payment terkait (nullable) |
| `entity_id` | `UUID` |  | ID entitas yang berubah |
| `entity_type` | `VARCHAR(50)` | `NOT NULL` | Tipe entitas (payment, attempt, api_key) |
| `action` | `VARCHAR(50)` | `NOT NULL` | Aksi yang dilakukan (CREATED, STATUS_CHANGED, RETRIED) |
| `actor` | `VARCHAR(100)` | `NOT NULL` | Pelaku (merchant_id, system, provider_name) |
| `field_name` | `VARCHAR(100)` |  | Field yang berubah |
| `old_value` | `TEXT` |  | Nilai sebelumnya |
| `new_value` | `TEXT` |  | Nilai baru |
| `metadata` | `JSONB` |  | Metadata tambahan (request_id, dll) |
| `ip_address` | `INET` |  | Alamat IP asal request |
| `correlation_id` | `VARCHAR(100)` |  | ID untuk tracing lintas service |
| `created_at` | `TIMESTAMPTZ` | `NOT NULL, DEFAULT NOW()` | Waktu kejadian |

### 2.7 `idempotency_keys`

Menyimpan mapping idempotency key untuk mencegah duplikasi.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `idempotency_key` | `VARCHAR(255)` | `PK` | Idempotency key dari request |
| `merchant_id` | `UUID` | `FK -> merchants.id, NOT NULL` | Merchant pengirim |
| `request_hash` | `VARCHAR(64)` | `NOT NULL` | SHA-256 hash dari request body |
| `payment_id` | `UUID` | `FK -> payments.id, NOT NULL` | Payment yang dihasilkan |
| `response_status_code` | `VARCHAR(3)` |  | HTTP status code dari response asli |
| `response_body` | `JSONB` |  | Response body asli |
| `created_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu pembuatan |
| `expires_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu kadaluwarsa key |

**Business Rules:**
- `idempotency_key + merchant_id` harus unik (UNIQUE constraint)
- Jika idempotency_key dan request_hash sama → return payment yang sama (idempotent)
- Jika idempotency_key sama tetapi request_hash berbeda → HTTP 409 Conflict

### 2.8 `reconciliation_records`

Mencatat hasil rekonsiliasi untuk payment yang statusnya tidak pasti.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `id` | `UUID` | `PK` | Identitas unik record |
| `payment_id` | `UUID` | `FK -> payments.id, NOT NULL` | Payment yang direkonsiliasi |
| `payment_attempt_id` | `UUID` | `FK -> payment_attempts.id` | Attempt yang memicu rekonsiliasi |
| `reconciliation_type` | `VARCHAR(30)` | `NOT NULL` | Tipe (TIMEOUT, FAILOVER, MANUAL) |
| `previous_status` | `VARCHAR(30)` | `NOT NULL` | Status sebelum rekonsiliasi |
| `current_status` | `VARCHAR(30)` | `NOT NULL` | Status setelah rekonsiliasi |
| `provider_status` | `VARCHAR(50)` |  | Status dari provider |
| `resolution` | `VARCHAR(30)` | `NOT NULL` | Hasil (COMPLETED, FAILED, UNCERTAIN) |
| `details` | `JSONB` |  | Detail tambahan |
| `created_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu rekonsiliasi |

### 2.9 `circuit_breakers`

State circuit breaker per provider untuk mencegah request ke provider yang bermasalah.

| Kolom | Tipe | Constraint | Deskripsi |
| --- | --- | --- | --- |
| `provider_key` | `VARCHAR(50)` | `PK` | Identitas provider |
| `state` | `VARCHAR(15)` | `NOT NULL, DEFAULT 'CLOSED'` | State (CLOSED, OPEN, HALF_OPEN) |
| `failure_count` | `INTEGER` | `NOT NULL, DEFAULT 0` | Hitungan kegagalan berturut-turut |
| `success_count` | `INTEGER` | `NOT NULL, DEFAULT 0` | Hitungan keberhasilan di HALF_OPEN |
| `last_failure_at` | `TIMESTAMPTZ` |  | Waktu kegagalan terakhir |
| `last_success_at` | `TIMESTAMPTZ` |  | Waktu keberhasilan terakhir |
| `opened_at` | `TIMESTAMPTZ` |  | Waktu circuit terbuka |
| `half_open_at` | `TIMESTAMPTZ` |  | Waktu circuit setengah terbuka |
| `next_retry_at` | `TIMESTAMPTZ` |  | Waktu boleh retry berikutnya |
| `updated_at` | `TIMESTAMPTZ` | `NOT NULL` | Waktu terakhir diperbarui |
        jsonb response_body
        timestamptz created_at
---

## 3. Relasi Antar Entitas

| Relasi | Dari | Ke | Tipe | Deskripsi |
| --- | --- | --- | --- | --- |
| R1 | `merchants` | `api_keys` | One-to-Many | Satu merchant memiliki banyak API key |
| R2 | `merchants` | `payments` | One-to-Many | Satu merchant membuat banyak payment |
| R3 | `api_keys` | `payments` | One-to-Many | Satu API key digunakan untuk banyak payment |
| R4 | `payments` | `payment_attempts` | One-to-Many | Satu payment memiliki banyak attempt |
| R5 | `payments` | `webhook_events` | One-to-Many | Satu payment menerima banyak webhook |
| R6 | `payments` | `audit_logs` | One-to-Many | Satu payment memiliki banyak audit log |
| R7 | `merchants` | `audit_logs` | One-to-Many | Satu merchant memiliki banyak audit log |
| R8 | `payments` | `idempotency_keys` | One-to-One | Satu payment direferensi oleh satu idempotency key |
| R9 | `merchants` | `idempotency_keys` | One-to-Many | Satu merchant memiliki banyak idempotency key |
| R10 | `payments` | `reconciliation_records` | One-to-Many | Satu payment memiliki banyak reconciliation record |
| R11 | `payment_attempts` | `reconciliation_records` | One-to-Many | Satu attempt memiliki banyak reconciliation record |

---

## 4. Indexes

### 4.1 Performance Indexes

```sql
-- Merchant lookups
CREATE INDEX idx_merchants_merchant_code ON merchants (merchant_code);
CREATE INDEX idx_merchants_status ON merchants (status);

-- API key lookups
CREATE INDEX idx_api_keys_merchant_id ON api_keys (merchant_id);
CREATE INDEX idx_api_keys_key_prefix ON api_keys (key_prefix);

-- Payment queries
CREATE INDEX idx_payments_merchant_id ON payments (merchant_id);
CREATE INDEX idx_payments_status ON payments (status);
CREATE INDEX idx_payments_created_at ON payments (created_at DESC);
CREATE INDEX idx_payments_merchant_reference ON payments (merchant_id, merchant_reference);
CREATE INDEX idx_payments_provider ON payments (provider);

-- Payment attempts
CREATE INDEX idx_payment_attempts_payment_id ON payment_attempts (payment_id);
CREATE INDEX idx_payment_attempts_provider ON payment_attempts (provider);
CREATE INDEX idx_payment_attempts_status ON payment_attempts (status);
CREATE INDEX idx_payment_attempts_payment_attempt ON payment_attempts (payment_id, attempt_number);

-- Webhook events
CREATE INDEX idx_webhook_events_payment_id ON webhook_events (payment_id);
CREATE INDEX idx_webhook_events_event_id ON webhook_events (event_id);
CREATE INDEX idx_webhook_events_processing_status ON webhook_events (processing_status);
CREATE UNIQUE INDEX idx_webhook_events_provider_event ON webhook_events (provider, event_id);

-- Audit logs
CREATE INDEX idx_audit_logs_merchant_id ON audit_logs (merchant_id);
CREATE INDEX idx_audit_logs_payment_id ON audit_logs (payment_id);
CREATE INDEX idx_audit_logs_created_at ON audit_logs (created_at DESC);
CREATE INDEX idx_audit_logs_entity ON audit_logs (entity_type, entity_id);
CREATE INDEX idx_audit_logs_correlation_id ON audit_logs (correlation_id);

-- Idempotency keys
CREATE INDEX idx_idempotency_keys_merchant_key ON idempotency_keys (merchant_id, idempotency_key);
CREATE INDEX idx_idempotency_keys_expires_at ON idempotency_keys (expires_at) WHERE expires_at > NOW();

-- Reconciliation
CREATE INDEX idx_reconciliation_payment_id ON reconciliation_records (payment_id);

-- Circuit breakers
CREATE INDEX idx_circuit_breakers_state ON circuit_breakers (state);
---

## 5. DBML Schema

Berikut adalah skema database dalam format DBML (Database Markup Language):

```dbml
Table merchants {
  id uuid [pk]
  name varchar(255) [not null]
  merchant_code varchar(50) [not null, unique]
  contact_email varchar(255) [not null]
  status varchar(20) [not null, default: 'ACTIVE']
  supported_currencies jsonb [not null]
  config jsonb
  created_at timestamptz [not null]
  updated_at timestamptz [not null]
}

Table api_keys {
  id uuid [pk]
  merchant_id uuid [not null, ref: > merchants.id]
  key_hash varchar(255) [not null]
  key_prefix varchar(10) [not null, unique]
  label varchar(100)
  permissions varchar(50) [not null, default: 'standard']
  expires_at timestamptz
  created_at timestamptz [not null]
  revoked_at timestamptz
}

Table payments {
  id uuid [pk]
  merchant_id uuid [not null, ref: > merchants.id]
  idempotency_key varchar(255) [not null, unique]
  merchant_reference varchar(255) [not null]
  currency varchar(3) [not null]
  amount bigint [not null]
  description text
  status varchar(30) [not null]
  provider varchar(50)
  failure_reason text
  payment_url varchar(500)
  created_by_api_key_id uuid [ref: > api_keys.id]
  created_at timestamptz [not null]
  updated_at timestamptz [not null]
  completed_at timestamptz
}

Table payment_attempts {
  id uuid [pk]
  payment_id uuid [not null, ref: > payments.id]
  provider varchar(50) [not null]
  provider_payment_id varchar(255)
  provider_status varchar(50)
  status varchar(30) [not null]
  attempt_type varchar(20) [not null]
  attempt_number integer [not null]
  request_snapshot jsonb
  response_snapshot jsonb
  http_status_code integer
  error_code varchar(100)
  error_message text
  duration_ms bigint
  started_at timestamptz
  completed_at timestamptz
  created_at timestamptz [not null]
}

Table webhook_events {
  id uuid [pk]
  payment_id uuid [not null, ref: > payments.id]
  provider varchar(50) [not null]
  event_id varchar(255) [not null, unique]
  event_type varchar(50) [not null]
  signature varchar(500) [not null]
  raw_body text [not null]
  verification_status varchar(20) [not null, default: 'PENDING']
  processing_status varchar(20) [not null, default: 'PENDING']
  verification_error text
  received_at timestamptz [not null]
  verified_at timestamptz
  processed_at timestamptz
}

Table audit_logs {
  id uuid [pk]
  merchant_id uuid [ref: > merchants.id]
  payment_id uuid [ref: > payments.id]
  entity_id uuid
  entity_type varchar(50) [not null]
  action varchar(50) [not null]
  actor varchar(100) [not null]
  field_name varchar(100)
  old_value text
  new_value text
  metadata jsonb
  ip_address inet
  correlation_id varchar(100)
  created_at timestamptz [not null, default: `now()`]
---

## 6. Referensi Data Model

ERD ini mengacu pada dokumentasi proyek berikut:

| Dokumen | Deskripsi |
| --- | --- |
| [BRD-Secure-Payment-Orchestrator.md](../planning/BRD-Secure-Payment-Orchestrator.md) | Business Requirements Document |
| [PRD-Secure-Payment-Orchestrator.md](../planning/PRD-Secure-Payment-Orchestrator.md) | Product Requirements Document |
| [WBS-Secure-Payment-Orchestrator.md](../planning/WBS-Secure-Payment-Orchestrator.md) | Work Breakdown Structure |

---

## 7. Changelog

| Versi | Tanggal | Perubahan | Penulis |
| --- | --- | --- | --- |
| 1.0 | 13 September 2026 | Draft awal ERD untuk POC | Engineering Team |
}

Table idempotency_keys {
  idempotency_key varchar(255) [pk]
  merchant_id uuid [not null, ref: > merchants.id]
  request_hash varchar(64) [not null]
  payment_id uuid [not null, ref: > payments.id]
  response_status_code varchar(3)
  response_body jsonb
  created_at timestamptz [not null]
  expires_at timestamptz [not null]
}

Table reconciliation_records {
  id uuid [pk]
  payment_id uuid [not null, ref: > payments.id]
  payment_attempt_id uuid [ref: > payment_attempts.id]
  reconciliation_type varchar(30) [not null]
  previous_status varchar(30) [not null]
  current_status varchar(30) [not null]
  provider_status varchar(50)
  resolution varchar(30) [not null]
  details jsonb
  created_at timestamptz [not null]
}

Table circuit_breakers {
  provider_key varchar(50) [pk]
  state varchar(15) [not null, default: 'CLOSED']
  failure_count integer [not null, default: 0]
  success_count integer [not null, default: 0]
  last_failure_at timestamptz
  last_success_at timestamptz
  opened_at timestamptz
  half_open_at timestamptz
  next_retry_at timestamptz
  updated_at timestamptz [not null]
}
```
```

### 4.2 Unique Constraints

```sql
ALTER TABLE merchants ADD CONSTRAINT uq_merchants_merchant_code UNIQUE (merchant_code);

ALTER TABLE api_keys ADD CONSTRAINT uq_api_keys_key_prefix UNIQUE (key_prefix);

ALTER TABLE payments ADD CONSTRAINT uq_payments_idempotency_key UNIQUE (idempotency_key);
ALTER TABLE payments ADD CONSTRAINT uq_payments_merchant_reference UNIQUE (merchant_id, merchant_reference);

ALTER TABLE webhook_events ADD CONSTRAINT uq_webhook_events_provider_event UNIQUE (provider, event_id);
```
        timestamptz expires_at
    }

    RECONCILIATION_RECORDS {
        uuid id PK
        uuid payment_id FK
        uuid payment_attempt_id FK
        string reconciliation_type
        string previous_status
        string current_status
        string provider_status
        string resolution
        jsonb details
        timestamptz created_at
    }

    CIRCUIT_BREAKERS {
        string provider_key PK
        string state
        int failure_count
        int success_count
        timestamptz last_failure_at
        timestamptz last_success_at
        timestamptz opened_at
        timestamptz half_open_at
        timestamptz next_retry_at
        timestamptz updated_at
    }

    MERCHANTS ||--o{ API_KEYS : has
    MERCHANTS ||--o{ PAYMENTS : creates
    MERCHANTS ||--o{ AUDIT_LOGS : "audited for"
    MERCHANTS ||--o{ IDEMPOTENCY_KEYS : owns

    API_KEYS ||--o{ PAYMENTS : "used by"

    PAYMENTS ||--o{ PAYMENT_ATTEMPTS : "has many"
    PAYMENTS ||--o{ WEBHOOK_EVENTS : "receives"
    PAYMENTS ||--o{ AUDIT_LOGS : "audited"
    PAYMENTS ||--o{ IDEMPOTENCY_KEYS : "referenced by"
    PAYMENTS ||--o{ RECONCILIATION_RECORDS : "reconciled"

    PAYMENT_ATTEMPTS ||--o{ RECONCILIATION_RECORDS : "reconciled"
```