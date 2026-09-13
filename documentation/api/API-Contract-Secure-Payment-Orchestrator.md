# API Contract

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi | 1.0 |
| Status | Draft untuk Proof of Concept |
| Tanggal | 13 September 2026 |
| Base URL | `http://localhost:8080` |
| API Version Prefix | `/api/v1` |
| Content Type | `application/json` |

---

## 1. Base Information

### 1.1 Base URL

```
http://localhost:8080/api/v1
```

Untuk environment POC, semua endpoint menggunakan base URL di atas. Provider simulator berjalan pada port terpisah (`:9091`, `:9092`, `:9093`).

### 1.2 Authentication

Semua endpoint (kecuali `/health`, `/ready`, `/metrics`, dan webhook) memerlukan autentikasi menggunakan **API Key** yang dikirim melalui header:

```
Authorization: Bearer <api_key>
Authorization: Bearer sk_live_abc123def456
```

**Aturan:**
- API key dikirim sebagai Bearer token
- Key divalidasi dengan hash yang tersimpan di database
- Merchant hanya dapat mengakses data miliknya sendiri
- Key yang expired atau revoked akan ditolak dengan HTTP 401

### 1.3 Idempotency

Endpoint mutasi (`POST`) memerlukan header idempotency:

```
Idempotency-Key: <unique_key_per_merchant>
```

**Aturan:**
- Key + merchant_id → UNIQUE constraint di database
- Same key + same payload → return existing response (idempotent)
- Same key + different payload → HTTP 409 Conflict
- Key TTL: 24 jam

### 1.4 Common Headers

| Header | Wajib | Deskripsi |
| --- | --- | --- |
| `Authorization` | Ya (kecuali public endpoints) | `Bearer <api_key>` |
| `Idempotency-Key` | Ya (POST mutasi) | Key unik untuk idempotency |
| `Content-Type` | Ya (POST/PUT) | `application/json` |
| `X-Request-ID` | Tidak | Untuk tracing, di-generate jika tidak ada |

### 1.5 Common Response Format

**Success Response:**

```json
{
  "data": { ... }
}
```

**Error Response:**

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable description",
    "details": {}
  }
}
```

### 1.6 HTTP Status Codes

| Kode | Deskripsi | Penggunaan |
| --- | --- | --- |
| `200 OK` | Request berhasil | GET, POST sukses |
| `201 Created` | Resource berhasil dibuat | POST /payments |
| `400 Bad Request` | Validasi gagal | Payload tidak valid |
| `401 Unauthorized` | Autentikasi gagal | API key salah/expired |
| `404 Not Found` | Resource tidak ditemukan | Payment ID tidak dikenal |
| `409 Conflict` | Konflik | Idempotency key + payload mismatch |
| `422 Unprocessable Entity` | Business rule violation | Transisi status tidak valid |
| `429 Too Many Requests` | Rate limit | Terlalu banyak request |
| `500 Internal Server Error` | Error server | Database down, dll |
| `502 Bad Gateway` | Provider error | Provider tidak merespon |
| `503 Service Unavailable` | Service unavailable | Maintenance |
## 3. Endpoint Detail

### 3.1 Create Payment

Membuat pembayaran baru dan memilih provider untuk diproses.

**Endpoint:**

```
POST /api/v1/payments
```

**Headers:**

| Header | Nilai |
| --- | --- |
| `Authorization` | `Bearer <merchant_api_key>` |
| `Idempotency-Key` | `checkout-order-10001` |
| `Content-Type` | `application/json` |

**Request Body:**

```json
{
  "merchant_reference": "ORDER-10001",
  "amount": 250000,
  "currency": "IDR",
  "customer": {
    "name": "Budi Santoso",
    "email": "budi@example.com"
  },
  "description": "Pembayaran ORDER-10001",
  "metadata": {
    "source": "web",
    "channel": "direct"
  }
}
```

**Field Descriptions:**

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `merchant_reference` | `string` | Ya | Referensi unik dari merchant (order ID) |
| `amount` | `integer` | Ya | Jumlah dalam satuan terkecil (sen). Contoh: Rp 2.500 = 250000 |
| `currency` | `string` | Ya | Kode ISO 4217. Didukung: `IDR`, `SGD` |
| `customer.name` | `string` | Tidak | Nama customer |
| `customer.email` | `string` | Tidak | Email customer |
| `description` | `string` | Tidak | Deskripsi pembayaran |
| `metadata` | `object` | Tidak | Data tambahan untuk audit |

**Response 201 Created:**

```json
{
  "data": {
    "payment_id": "pay_01J8ZVX8B8",
    "merchant_reference": "ORDER-10001",
    "status": "PENDING",
    "amount": 250000,
    "currency": "IDR",
    "provider": "ALPHA",
    "payment_url": "http://localhost:9091/pay/pay_01J8ZVX8B8",
    "created_at": "2026-09-13T10:00:00Z"
  }
}
```

**Response Fields:**

| Field | Type | Description |
| --- | --- | --- |
| `payment_id` | `string` | ID unik pembayaran (format: `pay_` prefix) |
| `merchant_reference` | `string` | Referensi dari merchant |
| `status` | `string` | Status awal: `PENDING` |
| `amount` | `integer` | Jumlah dalam satuan terkecil |
| `currency` | `string` | Kode ISO 4217 |
| `provider` | `string` | Provider terpilih (`ALPHA`, `BETA`, `GAMMA`) |
| `payment_url` | `string` | URL redirect ke provider |
| `created_at` | `string (ISO 8601)` | Waktu pembuatan |

**Response 200 OK (Idempotent):**

Jika idempotency key dan request body sama dengan request sebelumnya:

```json
{
  "data": {
    "payment_id": "pay_01J8ZVX8B8",
    "merchant_reference": "ORDER-10001",
    "status": "PROCESSING",
    "amount": 250000,
    "currency": "IDR",
    "provider": "ALPHA",
    "payment_url": "http://localhost:9091/pay/pay_01J8ZVX8B8",
    "created_at": "2026-09-13T10:00:00Z"
  }
}
```

**Response 409 Conflict (Idempotency Mismatch):**

```json
{
  "error": {
    "code": "IDEMPOTENCY_MISMATCH",
    "message": "Idempotency key already used with different request body",
    "details": {
      "existing_payment_id": "pay_01J8ZVX8B8",
      "conflict_field": "amount"
    }
  }
}
```

---

---

## 2. Endpoint Summary

| Method | Endpoint | Fungsi | Auth |
| --- | --- | --- | --- |
| `POST` | `/api/v1/payments` | Membuat pembayaran baru | Merchant API Key |
### 3.2 Get Payment

Mendapatkan detail pembayaran berdasarkan ID.

**Endpoint:**

```
GET /api/v1/payments/{payment_id}
```

**Headers:**

| Header | Nilai |
| --- | --- |
| `Authorization` | `Bearer <merchant_api_key>` |

**Path Parameters:**

| Parameter | Type | Description |
| --- | --- | --- |
| `payment_id` | `string` | ID pembayaran yang diperoleh dari create payment |

**Response 200 OK:**

```json
{
  "data": {
    "payment_id": "pay_01J8ZVX8B8",
    "merchant_reference": "ORDER-10001",
    "status": "SUCCESS",
    "amount": 250000,
    "currency": "IDR",
    "provider": "ALPHA",
    "payment_url": "http://localhost:9091/pay/pay_01J8ZVX8B8",
    "attempts": [
      {
        "attempt_number": 1,
        "provider": "ALPHA",
        "status": "SUCCESS",
        "duration_ms": 1250,
        "attempt_type": "INITIAL",
        "started_at": "2026-09-13T10:00:01Z",
        "completed_at": "2026-09-13T10:00:02Z"
      }
    ],
    "webhook_events": [
      {
        "event_id": "evt_alpha_001",
        "event_type": "payment.success",
        "verification_status": "VALID",
        "processing_status": "PROCESSED",
        "received_at": "2026-09-13T10:00:05Z"
      }
    ],
    "created_at": "2026-09-13T10:00:00Z",
    "updated_at": "2026-09-13T10:00:06Z",
    "completed_at": "2026-09-13T10:00:06Z"
  }
}
```

**Response 404 Not Found:**

```json
{
  "error": {
    "code": "PAYMENT_NOT_FOUND",
    "message": "Payment not found with the given ID",
    "details": {
      "payment_id": "pay_unknown_123"
    }
  }
### 3.4 Cancel Payment

Membatalkan pembayaran yang belum mencapai status final.

**Endpoint:**

```
POST /api/v1/payments/{payment_id}/cancel
```

**Headers:**

| Header | Nilai |
| --- | --- |
| `Authorization` | `Bearer <merchant_api_key>` |
| `Idempotency-Key` | `cancel-order-10001` |
| `Content-Type` | `application/json` |

**Path Parameters:**

| Parameter | Type | Description |
| --- | --- | --- |
| `payment_id` | `string` | ID pembayaran yang akan dibatalkan |

**Request Body:**

```json
{
  "reason": "Customer changed mind"
}
```

**Response 200 OK:**

```json
{
  "data": {
    "payment_id": "pay_01J8ZVX8B8",
    "status": "CANCELLED",
    "cancelled_at": "2026-09-13T11:00:00Z"
  }
}
```

**Error Conditions:**
- `422 Unprocessable Entity` jika payment sudah dalam status final
- `404 Not Found` jika payment_id tidak dikenal
- `400 Bad Request` jika reason melebihi batas karakter

---

### 3.5 Manual Retry

Melakukan retry manual untuk pembayaran yang gagal. Hanya untuk Operations.

**Endpoint:**

```
POST /api/v1/payments/{payment_id}/retry
```

**Headers:**

| Header | Nilai |
| --- | --- |
| `Authorization` | `Bearer <operations_api_key>` |
| `Idempotency-Key` | `retry-order-10001` |
| `Content-Type` | `application/json` |

**Path Parameters:**

| Parameter | Type | Description |
| --- | --- | --- |
| `payment_id` | `string` | ID pembayaran yang akan di-retry |

**Response 200 OK:**

```json
{
  "data": {
    "payment_id": "pay_01J8ZVX8B8",
    "status": "PROCESSING",
    "attempt_number": 2,
    "provider": "BETA",
    "message": "Retry initiated"
  }
}
```

### 3.7 Webhook Receiver

Menerima notifikasi webhook dari provider untuk memperbarui status pembayaran.

**Endpoint:**

```
POST /api/v1/webhooks/{provider}
```

**Headers:**

| Header | Nilai | Keterangan |
| --- | --- | --- |
| `X-Signature` | `t=1726200000,v1=abc123...` | HMAC SHA-256 signature |
| `Content-Type` | `application/json` | |

**Path Parameters:**

| Parameter | Type | Description |
| --- | --- | --- |
| `provider` | `string` | Nama provider (`alpha`, `beta`, `gamma`) |

**Request Body (Payment Success):**

```json
{
  "event_id": "evt_alpha_001",
  "event_type": "payment.success",
  "payment_id": "pay_01J8ZVX8B8",
  "status": "COMPLETED",
  "amount": 250000,
  "currency": "IDR",
  "timestamp": "2026-09-13T10:00:05Z",
  "metadata": {
    "provider_reference": "PROV-001"
  }
}
```

**Request Body (Payment Failed):**

```json
{
  "event_id": "evt_alpha_002",
  "event_type": "payment.failed",
  "payment_id": "pay_01J8ZVX8B8",
  "status": "FAILED",
  "failure_reason": "insufficient_funds",
  "timestamp": "2026-09-13T10:00:05Z"
}
```

**Response 200 OK:**

```json
{
  "status": "received",
  "event_id": "evt_alpha_001",
  "processing_status": "PROCESSED"
}
```

**Response 401 Unauthorized (Invalid Signature):**

```json
{
  "error": {
    "code": "INVALID_WEBHOOK_SIGNATURE",
    "message": "Webhook signature verification failed",
    "details": {
      "provider": "alpha",
      "event_id": "evt_alpha_001"
    }
  }
}
```

**Signature Verification:**

```
Format: t=<unix_timestamp>,v1=<hmac_hex>
Algoritma: HMAC SHA-256
Payload: timestamp + '.' + raw_body
Secret: Didapat dari environment per provider
```

---

### 3.8 Health Check

Liveness probe untuk Kubernetes / Docker.

**Endpoint:**

```
GET /health
```

**Headers:** Tidak ada (public endpoint)

**Response 200 OK:**

```json
{
  "status": "ok",
  "version": "1.0.0",
  "uptime_seconds": 3600
}
```

---

### 3.9 Readiness Check

Readiness probe untuk Kubernetes / Docker. Gagal jika database tidak tersedia.

**Endpoint:**

```
GET /ready
```

**Headers:** Tidak ada (public endpoint)

**Response 200 OK:**

```json
{
  "status": "ready",
  "database": "connected",
  "redis": "connected"
}
```

**Response 503 Service Unavailable:**

```json
{
  "status": "not_ready",
  "database": "disconnected",
  "redis": "connected"
}
```

---

### 3.10 Metrics

Menyajikan metrics Prometheus untuk observability.

**Endpoint:**

```
GET /metrics
```

**Headers:** Internal network only

**Response 200 OK:**

```text
# HELP spo_payments_total Total number of payments
# TYPE spo_payments_total counter
spo_payments_total{status="SUCCESS",provider="ALPHA"} 125
spo_payments_total{status="FAILED",provider="ALPHA"} 3

# HELP spo_payment_duration_ms Payment processing duration in ms
# TYPE spo_payment_duration_ms histogram
spo_payment_duration_ms_bucket{le="100",provider="ALPHA"} 50
spo_payment_duration_ms_bucket{le="500",provider="ALPHA"} 100
spo_payment_duration_ms_sum{provider="ALPHA"} 45000
spo_payment_duration_ms_count{provider="ALPHA"} 128
```

---

## 4. Error Codes

| Code | HTTP Status | Description |
| --- | --- | --- |
| `AUTHENTICATION_FAILED` | 401 | API key tidak valid atau expired |
| `AUTHORIZATION_FAILED` | 403 | API key tidak memiliki izin untuk resource ini |
| `VALIDATION_ERROR` | 400 | Request body tidak valid |
| `IDEMPOTENCY_MISMATCH` | 409 | Idempotency key sama, payload berbeda |
| `PAYMENT_NOT_FOUND` | 404 | Payment ID tidak ditemukan |
| `INVALID_STATUS_TRANSITION` | 422 | Transisi status tidak diperbolehkan |
| `PAYMENT_ALREADY_FINAL` | 422 | Payment sudah dalam status final |
| `MAX_RETRY_REACHED` | 400 | Retry maksimum sudah tercapai |
| `INVALID_WEBHOOK_SIGNATURE` | 401 | Signature webhook tidak valid |
| `WEBHOOK_EXPIRED` | 401 | Webhook timestamp di luar tolerance |
| `DUPLICATE_WEBHOOK_EVENT` | 200 | Event ID sudah diproses (tidak error) |
| `PROVIDER_UNAVAILABLE` | 502 | Provider sedang tidak tersedia |
| `INTERNAL_ERROR` | 500 | Error server internal |

---

## 5. Referensi

| Dokumen | Deskripsi |
| --- | --- |
| [BRD-Secure-Payment-Orchestrator.md](../../BRD-Secure-Payment-Orchestrator.md) | Business Requirements Document |
| [PRD-Secure-Payment-Orchestrator.md](../../PRD-Secure-Payment-Orchestrator.md) | Product Requirements Document |
| [ERD-Secure-Payment-Orchestrator.md](../erd/ERD-Secure-Payment-Orchestrator.md) | Entity Relationship Diagram |
| [Architecture-Secure-Payment-Orchestrator.md](../architecture/Architecture-Secure-Payment-Orchestrator.md) | Architecture Document |

## 6. Changelog

| Versi | Tanggal | Perubahan | Penulis |
| --- | --- | --- | --- |
| 1.0 | 13 September 2026 | Draft awal API Contract | Engineering Team |