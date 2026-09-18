# Task Progress Report

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi report | 11.0 |
| Tanggal audit | 18 September 2026 |
| Status produk | Proof of Concept, belum production-ready |
| Dasar penilaian | `cargo build`, `cargo test`, `cargo fmt -- --check` (registry Cargo dapat diakses), ditambah pemeriksaan source code, migration, konfigurasi, dan dokumentasi |
| Verifikasi build | **Lulus.** `cargo build` sukses (lib + bin + `gen_api_key`), `cargo test` sukses (57/57 test lulus), `cargo fmt -- --check` lulus tanpa isu |

## 1. Ringkasan Eksekutif

Fondasi proyek sudah tersedia: struktur aplikasi Rust, domain model, state machine,
PostgreSQL repositories, Redis lock helper, kontrak provider, empat provider adapter
(Midtrans/Alpha, Xendit/Beta, DOKU/Gamma, NICEPAY), konfigurasi, dokumentasi API,
container setup, authentication middleware, idempotency middleware, payment application
service, `POST /payments`/`GET /payments/{id}` yang tersambung ke `PaymentService`
(v8.0), request validation (v9.0), atomic transaction untuk `create_payment` (v10.0),
dan — baru pada pass ini — **`GET /payments` (search)** dan **`POST /payments/{id}/cancel`**
yang benar-benar berfungsi.

Pass v11.0 mengerjakan dua handler sekaligus:

1. **Search payment.** `PaymentService::search_payments` (wrapper tipis di atas
   `PaymentRepository::search` yang sudah ada) + `SearchPaymentParams::into_filter()` di
   DTO layer — apply default (`page=1`, `limit=20`), validasi bound (`1 ≤ limit ≤ 100`),
   dan parse `from_date`/`to_date` sebagai RFC 3339, semua error field dikumpulkan
   sekaligus ke satu `400 VALIDATION_ERROR` (pola yang sama seperti
   `CreatePaymentRequest::validate`). **Ditemukan dan diperbaiki sekalian:**
   `SearchCriteria.from_date`/`to_date` sudah ada di struct sejak awal tapi query SQL di
   `PgPaymentRepository::search` **tidak pernah memakainya** — filter tanggal diterima
   API tapi diam-diam diabaikan. Sekarang query SELECT dan COUNT keduanya memfilter
   `created_at` dengan benar.
2. **Cancel payment.** `PaymentService::cancel_payment` — fetch payment (merchant-scoped),
   `payment.transition_to(Cancelled)`, `payment_repo.update_status`, lalu audit log
   best-effort. Supaya error-nya presisi, `domain::rules::validate_transition` direfactor:
   payment yang sudah final (`SUCCESS`/`FAILED`/`CANCELLED`) sekarang ditolak duluan
   dengan `DomainError::AlreadyFinal` (varian yang sudah ada di kode tapi belum pernah
   dipakai) — bukan `InvalidTransition` generik — supaya handler bisa mengembalikan
   `422 PAYMENT_ALREADY_FINAL` yang persis sesuai
   `documentation/api/API-Contract-Secure-Payment-Orchestrator.md`, bukan cuma
   `422 INVALID_STATUS_TRANSITION`. Response cancel pakai DTO baru
   (`CancelPaymentResponse`/`CancelPaymentData`) karena bentuknya lebih ringkas daripada
   `PaymentResponse` biasa (`{payment_id, status, cancelled_at}`, sesuai contoh di API
   contract §3.4).

`PaymentService` kembali butuh `audit_repo` (untuk cancel) di samping `payment_tx`
(untuk create) — bukan regresi dari refactor atomic-transaction v10.0, ini kolaborator
tambahan yang genuinely baru.

`domain::rules::validate_transition` untuk pertama kalinya punya unit test (5 test) —
sebelumnya fungsi krusial ini sama sekali tidak tertest.

19 unit test baru: 4 di `application::payment` (search + cancel happy/already-final/
not-found), 8 di `api::dto::payment` (cancel validate/response, search params/response),
2 di `api::routes::payment` (error mapping `AlreadyFinal`/`InvalidTransition`), 5 di
`domain::rules`.

| Status | Jumlah task | Persentase |
| --- | ---: | ---: |
| Selesai | 30 | 50% |
| Parsial | 14 | 23% |
| Belum | 16 | 27% |
| **Total** | **60** | **100%** |

Dibanding v10.0 (28 selesai / 13 parsial / 19 belum), tiga task naik status: Search
payment dan Cancel payment (Belum → Selesai), dan Domain unit tests (Belum → Parsial,
karena `rules.rs` sekarang tertest tapi `payment.rs`/`status.rs`/`attempt.rs` masih
belum). Tidak ada task yang turun status.

Persentase di atas adalah hitungan task pada report ini, bukan estimasi LOC atau klaim
kesiapan production. Search dan cancel **belum ditest terhadap Postgres sungguhan**
(tidak ada Docker di environment ini) — sama seperti create/get. Manual retry dan
reconcile masih `NOT_IMPLEMENTED`, dan idempotency middleware masih belum bisa menyimpan
response untuk replay (gap dari v6.0/v10.0, tidak tersentuh pass ini).

## 2. Definisi Status

| Status | Arti |
| --- | --- |
| Selesai | Implementasi utama tersedia di source code dan tidak diketahui memiliki compile error terverifikasi |
| Parsial | Kontrak, model, atau helper tersedia, tetapi belum terhubung end-to-end, dan/atau memiliki compile error yang terverifikasi |
| Belum | Hanya komentar/skeleton, mengembalikan `NOT_IMPLEMENTED`, atau file/deliverable belum tersedia |

## 3. Progress per Workstream

### 3.1 Inisiasi dan desain — 5 selesai, 1 parsial

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Business requirements | Selesai | `documentation/planning/BRD-Secure-Payment-Orchestrator.md` |
| Product requirements | Selesai | `documentation/planning/PRD-Secure-Payment-Orchestrator.md` |
| Work breakdown structure | Selesai | `documentation/planning/WBS-Secure-Payment-Orchestrator.md` |
| Architecture document | Selesai | `documentation/architecture/Architecture-Secure-Payment-Orchestrator.md` |
| ERD dan relational schema design | Selesai | ERD dan migration awal tersedia |
| Threat model | Parsial | Risiko tersebar di BRD/production plan; belum ada threat-model khusus |

### 3.2 Project foundation — 5 selesai, 1 parsial

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Cargo project dan module structure | Selesai | `cargo build` (lib + bin) dan `cargo fmt -- --check` lulus tanpa error; hanya warning kosmetik tersisa (`unused variable`, `dead_code`) |
| Configuration loading | Selesai | `src/config/settings.rs` — seluruh env var provider (Midtrans, Xendit, DOKU, NICEPAY) dan retry/circuit-breaker termuat |
| PostgreSQL pool dan startup migration | Selesai | Diperbaiki: `main.rs` sekarang memanggil `infrastructure::create_pool` (path yang benar, sebelumnya salah memanggil `infrastructure::postgres::create_pool`) |
| Redis client setup | Selesai | Diperbaiki: root cause-nya adalah name shadowing — `src/infrastructure/mod.rs` mendeklarasikan `pub mod redis;` (module lokal) di file yang sama dengan `use redis::aio::ConnectionManager;`, sehingga `use` merujuk ke module lokal alih-alih crate eksternal. Fix: qualifikasi eksplisit `::redis::aio::ConnectionManager` dan `::redis::Client::open` |
| Dockerfile dan Docker Compose | Selesai | Container definition tersedia |
| Health/readiness endpoint | Parsial | Diperbaiki: `AppState` sekarang punya field `db_pool` (ditambahkan di `src/lib.rs`), dan Redis ping dipanggil via `redis::Cmd::new().arg("PING").query_async::<String>(...)` (bukan method `.ping()` yang memang tidak ada di API `ConnectionManager`). Endpoint compile dan berjalan; uptime masih hard-coded `0` (belum ada tracking start time) |

### 3.3 Domain dan persistence — 6 selesai, 1 parsial

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Payment aggregate dan Money | Selesai | `src/domain/payment.rs` — entity, validasi, transition wrapper |
| Payment status/state machine | Selesai | `src/domain/status.rs`, `src/domain/rules.rs` — enum dan transition matrix lengkap. Sejak v11.0, `validate_transition` menolak payment yang sudah final dengan `DomainError::AlreadyFinal` (bukan `InvalidTransition` generik) — dipakai `cancel_payment` untuk `422 PAYMENT_ALREADY_FINAL` yang presisi. 5 unit test baru (lihat §3.9) |
| Retry classification/backoff rules | Selesai | `is_retryable_http_status`, `is_retryable_error`, `retry_delay_seconds` di `src/domain/rules.rs` |
| Repository contracts | Selesai | 7 trait (payment, `PaymentTransactionRepository` baru v10.0, attempt, API key, idempotency, audit, webhook) di `src/domain/repositories.rs` |
| PostgreSQL repository implementation | Selesai | Diperbaiki: `PaymentAttemptRow` ditambahkan ke import di `repositories.rs`, dan `domain/attempt.rs` memakai `row.status.as_str()` / `row.attempt_type.as_str()` (bukan `&row.status`) agar cocok dengan `impl From<&str>`. Query CRUD/search/count untuk semua 6 repository lengkap dan compile bersih. Sejak v10.0, INSERT payment/attempt/audit diekstrak jadi fungsi executor-generic (`insert_payment`/`insert_attempt`/`insert_audit_log`) dipakai bersama oleh repo biasa (`&PgPool`) dan `PgPaymentTransactionRepository` (`&mut *tx`). Sejak v11.0, query `search` (SELECT dan COUNT) memfilter `created_at` berdasar `from_date`/`to_date` — sebelumnya kedua field itu ada di `SearchCriteria` tapi diam-diam diabaikan di SQL |
| Atomic business transaction | Selesai (naik dari Parsial) | `PgPaymentTransactionRepository::create_with_attempt_and_audit` memakai `pool.begin()`/`tx.commit()` sungguhan — payment, attempt awal, dan audit log tersimpan dalam satu transaksi. `idempotency_keys` **belum** ikut transaksi ini (butuh request hash dari middleware + response body dari handler, tidak tersedia di `PaymentService`) — lihat §1 dan §7. **Belum ditest terhadap Postgres sungguhan** |
| Concurrency protection | Parsial | Redis lock helper (§3.6) kini dipakai nyata oleh idempotency middleware (§3.4) untuk mencegah request konkuren dengan `Idempotency-Key` sama diproses bersamaan; belum dipakai oleh webhook/reconciliation flow — `application/*.rs` masih skeleton satu baris |

### 3.4 Core Payment API — 7 selesai, 0 parsial, 1 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Request/response DTO | Selesai | Struct lengkap; `impl From<Payment> for PaymentResponse` (v8.0) dipakai handler sungguhan. `CreatePaymentRequest::validate()` (v9.0) dan `CancelPaymentRequest::validate()` (baru v11.0, cek panjang `reason` ≤1000 char) dipanggil sebelum service. `SearchPaymentParams::into_filter()` (baru v11.0) apply default page/limit dan parse tanggal RFC 3339 |
| Authentication middleware | Selesai | `src/api/middleware/authentication.rs::require_api_key` — validasi `Authorization: Bearer`, lookup `key_prefix`, verifikasi Argon2 constant-time (`security::hash::verify_secret`), cek merchant `ACTIVE`, attach `MerchantContext` ke request extensions. Dipasang hanya pada payment routes via `axum::middleware::from_fn_with_state` di `api/mod.rs` (bukan global — health/ready/metrics/webhook tidak terpengaruh). **Belum ditest end-to-end** (tidak ada Postgres/Docker di environment ini); hanya unit test pure-logic yang lulus (lihat §3.7, §3.9) |
| Idempotency middleware | Selesai | `src/api/middleware/idempotency.rs::require_idempotency_key` — wajibkan header pada POST (maks 255 karakter sejak v9.0, cocok kolom DB), hash body (SHA-256), cek duplikat/conflict ke `IdempotencyRepository` (replay cached response atau `409 IDEMPOTENCY_MISMATCH`), acquire Redis lock untuk cegah request konkuren (`409 IDEMPOTENCY_IN_PROGRESS`). Dipasang setelah auth middleware (butuh `MerchantContext`). Sejak v8.0 juga attach `IdempotencyKey` ke request extensions supaya `create_payment` tidak perlu parse ulang header. Sisi tulis (`save` ke `idempotency_keys`) sengaja belum ada — menunggu perluasan atomic transaction (lihat §5 P0). **Belum ditest end-to-end** (alasan sama seperti authentication middleware) |
| Create payment | Selesai | `src/api/routes/payment.rs::create_payment` memanggil `PaymentService::create_payment` dan mengembalikan `201 Created` dengan `PaymentResponse` sungguhan. Error dipetakan ke kode API yang sesuai (`map_application_error`). Persistensi atomic (§3.3, sejak v10.0). **Belum ditest terhadap Postgres/Redis sungguhan** (tidak ada Docker di environment ini) |
| Get payment | Selesai | `get_payment` memanggil `PaymentService::get_payment`, balas `200 OK` atau `404 PAYMENT_NOT_FOUND` (kode+detail sesuai API contract §3.2). **Belum ditest terhadap Postgres sungguhan** |
| Search payment | Selesai (naik dari Belum) | `search_payments` memanggil `PaymentService::search_payments` (wrapper atas `PaymentRepository::search`). Ditemukan+diperbaiki sekalian: SQL `search` sebelumnya menerima `from_date`/`to_date` di `SearchCriteria` tapi **tidak pernah memfilternya** — sekarang query SELECT dan COUNT keduanya memfilter `created_at` dengan benar (lihat §3.3). **Belum ditest terhadap Postgres sungguhan** |
| Cancel payment | Selesai (naik dari Belum) | `cancel_payment` memanggil `PaymentService::cancel_payment` (fetch → `transition_to(Cancelled)` → `update_status` → audit log best-effort), balas `200 OK` dengan `CancelPaymentResponse` baru (`{payment_id, status, cancelled_at}`, bentuk lebih ringkas sesuai API contract §3.4) atau `404`/`422 PAYMENT_ALREADY_FINAL`. **Belum ditest terhadap Postgres sungguhan** |
| Manual retry endpoint | Belum | Handler mengembalikan `NOT_IMPLEMENTED`; butuh retry orchestration (P1), bukan sekadar wiring |

### 3.5 Provider integration dan fallback — 2 selesai, 4 parsial, 3 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Canonical `PaymentProvider` contract | Selesai | `src/providers/adapter.rs` — trait, request, response, error types |
| Midtrans/Xendit/DOKU provider adapters | Selesai | Alpha (277 baris), Beta (232 baris), Gamma (346 baris) — Midtrans, Xendit, DOKU Sandbox |
| NICEPAY example adapter | Parsial | Registration/create tersedia (269 baris); inquiry memerlukan referenceNo dan amt yang belum dibawa kontrak status provider |
| Provider availability contract | Parsial | `is_available()` di trait; Midtrans unavailable jika Server Key kosong; health/circuit breaker runtime belum tersedia |
| Failover data model | Parsial | `AttemptType::Failover` tersedia; flow belum diimplementasikan |
| Provider selection by availability/priority | Parsial (naik dari Belum) | `PaymentService::create_payment` (§3.4) memilih provider pertama yang `is_available()` — placeholder naif, bukan seleksi berbasis prioritas/circuit breaker sungguhan. `src/application/provider.rs` sendiri masih skeleton satu baris |
| Timeout wrapper dan response classification | Belum | Belum ada orchestration implementation |
| Circuit breaker | Belum | Hanya config (`circuit_breaker_threshold`, `circuit_breaker_timeout_seconds` di `settings.rs`); tidak ada state machine/runtime |
| Automatic fallback A ke B | Belum | Belum ada routing, safe-failure decision, atau failover execution |

### 3.6 Reliability dan reconciliation — 1 selesai, 1 parsial, 4 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Redis distributed lock helper | Selesai | Diperbaiki: `release_lock` di `src/infrastructure/redis/lock.rs` sebelumnya gagal compile karena never-type-fallback ambiguity pada `invoke_async(...).await?`; fix dengan anotasi eksplisit `let _: () = script...invoke_async(redis).await?;`. Logic acquire (`SET NX EX`) dan release (ownership-check Lua) sudah benar secara desain dan sekarang compile bersih |
| Retry rules | Parsial | Klasifikasi dan backoff tersedia di domain layer; worker/orchestrator belum ada |
| Retry worker dan max-attempt execution | Belum | Belum ada worker implementation |
| Reconciliation service | Belum | `src/application/reconciliation.rs` hanya doc comment satu baris |
| Reconciliation endpoint | Belum | Handler mengembalikan `NOT_IMPLEMENTED` |
| Late-webhook conflict handling | Belum | Belum ada processing implementation |

### 3.7 Security dan webhook — 1 selesai, 2 parsial, 3 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Security algorithm/design | Parsial | Bagian API key (Argon2 + constant-time) sudah diimplementasikan; bagian HMAC-SHA256 webhook (`security/webhook_sig.rs`) masih doc comment intent saja |
| API-key hashing dan verification | Selesai | `src/security/hash.rs::hash_secret`/`verify_secret` (Argon2id, `PasswordVerifier` yang inheren constant-time) dan `src/security/api_key.rs::parse_bearer_token`/`key_prefix`; 8 unit test lulus (roundtrip, wrong-secret, malformed hash, bearer parsing, key prefix) |
| Merchant authentication/authorization | Parsial (naik dari Belum) | Merchant diidentifikasi dan discoping via `MerchantContext` (§3.4) setelah API key tervalidasi; otorisasi granular per-permission (`standard`/`operations`/`admin`) belum diterapkan di level route karena handler yang dilindungi (retry/reconcile) sendiri masih `NOT_IMPLEMENTED` |
| Webhook signature verification | Belum | `src/security/webhook_sig.rs` hanya doc comment |
| Webhook replay/duplicate processing | Belum | DB unique constraint tersedia (`uq_webhook_events_provider_event`), tetapi service belum ada |
| Webhook route processing | Belum | Handler mengembalikan `NOT_IMPLEMENTED` |

### 3.8 Observability dan operations — 1 selesai, 2 parsial, 1 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Logging initialization | Selesai | Diimplementasikan: `src/observability/logging.rs::init()` sekarang membangun `tracing_subscriber` dengan format JSON dan `EnvFilter` dari `settings.log_level` (sebelumnya file ini hanya doc comment tanpa fungsi apa pun, menyebabkan `main.rs` gagal compile) |
| Request/correlation ID | Parsial | Diperbaiki hanya path pemanggilan: `api/mod.rs` sekarang memanggil `middleware::request_id_layer()` yang benar-benar ada (bukan `middleware::request_id::request_id_layer()` yang tidak ada), tetap berupa `Identity` pass-through — belum ada generate/propagate `X-Request-ID` sungguhan |
| Prometheus instrumentation/export | Parsial (naik dari Belum) | Diimplementasikan: `src/observability/metrics.rs::init()` memasang `PrometheusBuilder` recorder global, dan `GET /metrics` (`api/routes/metrics.rs`) merender snapshot asli via `render()`. Belum ada metric (`spo_payments_total`, `spo_payment_duration_ms`) yang benar-benar direkam karena application layer belum memanggilnya — endpoint akan menampilkan output kosong sampai instrumentasi ditambahkan di P1/P2 |
| Operational payment actions | Belum | Cancel, retry, dan reconcile belum bekerja |

### 3.9 Quality assurance dan delivery — 2 selesai, 2 parsial, 4 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| API contract | Selesai | `documentation/api/API-Contract-Secure-Payment-Orchestrator.md` |
| OpenAPI specification | Selesai | `documentation/api/openapi.yaml` |
| Domain unit tests | Parsial (naik dari Belum) | `src/domain/rules.rs` sekarang punya 5 unit test (`validate_transition`: transisi valid, non-final→Cancelled, final→AlreadyFinal untuk ketiga status final, transisi terlarang→InvalidTransition). `payment.rs`, `status.rs`, `attempt.rs`, `error.rs` masih belum punya test langsung — `Payment::try_from(PaymentRow)` cuma tercakup tidak langsung lewat test `application::payment` |
| API integration tests | Belum | `tests/api/mod.rs` masih TODO |
| Provider tests | Parsial | 5 unit test di provider layer (Midtrans/Xendit/DOKU/NICEPAY status mapping) — semuanya **lulus** via `cargo test`; `tests/providers/mod.rs` (integration) masih TODO. Di luar provider, ada 52 unit test lain (8 security, 3 idempotency middleware, 11 `application::payment` termasuk search/cancel, 10 DTO mapping termasuk cancel/search, 7 error mapping handler termasuk AlreadyFinal/InvalidTransition, 8 request validation, 5 `domain::rules`) — total 57/57 lulus |
| CI/CD workflow | Belum | Folder `.github/` tidak ditemukan di repository |
| Postman collection | Belum | File tidak tersedia |
| Demo/end-to-end script | Belum | File tidak tersedia |

## 4. Daftar yang Sudah Selesai

1. BRD, PRD, WBS, architecture, dan ERD.
2. Configuration loader (seluruh provider + retry/circuit-breaker settings).
3. Migration schema, indexes, dan seed data (merchants + 2 demo API key sejak v7.0).
4. Payment aggregate, Money, payment status, dan transition rules.
5. Retry classification dan exponential-backoff calculation.
6. Repository traits (7 kontrak sejak v10.0) dan seluruh implementasi PostgreSQL-nya.
7. Provider adapter contract serta adapter create/status untuk Midtrans, Xendit, dan DOKU.
8. Contoh registration/create payment NICEPAY; inquiry masih parsial.
9. Dockerfile dan Docker Compose.
10. API contract dan OpenAPI specification.
11. PostgreSQL pool creation dan Redis client setup (path/naming bug diperbaiki).
12. Redis distributed lock helper (compile bug diperbaiki).
13. Structured JSON logging initialization.
14. `spo-api` compile bersih: `cargo build`, `cargo test` (57/57 lulus), dan `cargo fmt -- --check` semuanya lulus.
15. Authentication middleware — Argon2 API-key verification, merchant context, dipasang hanya pada payment routes (belum ditest end-to-end, lihat §3.4 dan §7).
16. Idempotency middleware (sisi baca) — duplicate/conflict detection dan Redis lock concurrency guard, sekarang juga membatasi panjang `Idempotency-Key` (≤255 karakter).
17. `src/bin/gen_api_key.rs` — tool generate/hash API key, dipakai untuk membuat 2 seed key di migration `20260917_002_seed_demo_api_keys.sql` (cocok contoh `sk_live_demo_key_001`/`sk_live_ops_key_001` di README.md).
18. `application::payment::PaymentService` (`create_payment`, `get_payment`) — lengkap dan tertest dengan fake repository/provider.
19. `POST /payments` dan `GET /payments/{id}` benar-benar tersambung ke `PaymentService` — bukan lagi `NOT_IMPLEMENTED` (lihat §3.4). Belum ditest terhadap Postgres/Redis sungguhan.
20. `CreatePaymentRequest::validate()` — required-field, length, dan format checks (merchant_reference, amount, currency, description, customer.email), dipanggil di `create_payment` sebelum service; error field terkumpul sekaligus dalam satu `400 VALIDATION_ERROR`.
21. Atomic transaction untuk `create_payment` — `PgPaymentTransactionRepository` membungkus insert payment + attempt + audit log dalam satu `sqlx::Transaction` (lihat §3.3). Belum ditest terhadap Postgres sungguhan.
22. `GET /payments` (search) dan `POST /payments/{id}/cancel` tersambung ke `PaymentService` — bukan lagi `NOT_IMPLEMENTED` (lihat §3.4). Termasuk perbaikan bug lama: filter `from_date`/`to_date` di search sekarang benar-benar dipakai SQL. Belum ditest terhadap Postgres sungguhan.
23. `domain::rules::validate_transition` — payment final ditolak dengan `DomainError::AlreadyFinal` (bukan `InvalidTransition` generik), sekarang punya 5 unit test (sebelumnya nihil).

## 5. Daftar yang Belum Selesai

Seluruh compile blocker dari v3.0 (12 error + 3 temuan static-review) sudah diperbaiki —
lihat §9 Changelog untuk daftar lengkap dan §3 untuk detail per task. Backlog di bawah ini
murni tentang fitur/business logic yang belum dikerjakan, bukan lagi tentang kode yang
tidak bisa dikompilasi.

### Prioritas P0 — agar core payment flow dapat berjalan

1. ~~Authentication middleware dan merchant context.~~ **Selesai pada v5.0** — lihat §3.4, §3.7. Belum ditest terhadap database sungguhan.
2. ~~API-key hashing/verification.~~ **Selesai pada v5.0.**
3. ~~Idempotency flow dengan Redis lock dan DB constraint.~~ **Sisi baca selesai pada v6.0** (duplicate/conflict detection, concurrency lock) — lihat §3.4. Sisi tulis (persist `IdempotencyRow`) **masih terbuka** meski atomic transaction (item #11) sudah selesai — `PaymentService` belum punya akses ke request hash (di middleware) dan response body (di handler) yang dibutuhkan untuk mengisi baris itu.
4. ~~Seed/tooling untuk membuat API key.~~ **Selesai pada v7.0** — `src/bin/gen_api_key.rs` + migration `20260917_002_seed_demo_api_keys.sql`. Belum dijalankan terhadap Postgres sungguhan (tidak ada Docker di environment ini).
5. ~~Request validation.~~ **Selesai** untuk `CreatePaymentRequest` (v9.0), `CancelPaymentRequest`, dan `SearchPaymentParams` (v11.0) — lihat §3.4.
6. ~~Payment application service.~~ **Selesai** — `PaymentService::create_payment`/`get_payment` (v7.0), `search_payments`/`cancel_payment` (v11.0) lengkap+tertest (lihat §3.4, §3.9); `retry`/`reconcile` masih belum punya method service (lihat item #10).
7. Provider selection dan invocation. **Seleksi naif ("first available") sudah ada** di dalam `PaymentService::create_payment` sejak v7.0 (lihat §3.5) — seleksi berbasis prioritas/circuit breaker sungguhan masih belum.
8. ~~Create dan Get payment handlers.~~ **Selesai pada v8.0** — `src/api/routes/payment.rs::create_payment`/`get_payment` tersambung ke `PaymentService` (lihat §3.4). Belum ditest terhadap Postgres/Redis sungguhan.
9. ~~Search dan Cancel payment handlers.~~ **Selesai pada v11.0** (lihat §3.4). Catatan: `cancel_payment` cuma mengubah status lokal, **tidak** memberi tahu provider (mis. void transaksi di gateway) — sesuai bentuk response yang didokumentasikan API contract §3.4 (tidak ada field provider), tapi worth diperiksa ulang kalau requirement sebenarnya butuh provider notification.
10. Manual retry dan reconciliation endpoint — belum ada method service, belum ada retry/reconciliation orchestration (lihat P1).
11. ~~Atomic transaction untuk payment, attempt, dan audit log.~~ **Selesai pada v10.0** — `PgPaymentTransactionRepository::create_with_attempt_and_audit` (lihat §3.3). Idempotency record **belum** ikut transaksi ini (lihat item #3) — perluasan itu jadi item tersendiri, bukan otomatis selesai bersama ini.
12. Sambungkan `IdempotencyRepository::save()` ke transaksi `create_payment` — perlu request hash dari idempotency middleware dan response body dari handler dialirkan ke `PaymentService`/`PgPaymentTransactionRepository` (kemungkinan perlu menambah parameter atau method baru di trait `PaymentTransactionRepository`).

### Prioritas P1 — reliability dan fallback

1. Provider timeout handling dan error classification.
2. Retry orchestration/worker dengan bounded attempts.
3. Reconciliation service dan endpoint.
4. Circuit breaker per provider.
5. Safe automatic fallback dari Gateway A ke Gateway B.
6. Locking antara retry, webhook, reconciliation, dan failover (lock helper sudah siap dipakai, tinggal diintegrasikan).
7. Late webhook dan conflicting-status handling.

### Prioritas P2 — security, observability, dan readiness

1. HMAC webhook verification dan timestamp tolerance.
2. Duplicate/replay webhook processing.
3. Instrumentasi metric aktual (`spo_payments_total`, `spo_payment_duration_ms`) — exporter-nya sudah siap.
4. Request ID/correlation ID generation & propagation sungguhan (layer placeholder sudah terpasang).
5. Uptime tracking pada health response.
6. Threat model khusus.

### Prioritas P3 — quality dan delivery

1. Domain unit tests.
2. Repository/provider integration tests (unit test provider sudah ada dan lulus).
3. API integration, idempotency, concurrency, webhook, retry, dan failover tests.
4. CI workflow untuk format, lint, build, dan test (`cargo build`/`test`/`fmt` semuanya sudah lulus lokal, tinggal disambungkan ke CI; `cargo clippy` belum pernah dijalankan).
5. Postman collection.
6. Demo/end-to-end script.
7. Load, security, dan failure-injection testing.

## 6. Acceptance Criteria Milestone Berikutnya

Milestone berikutnya dapat dianggap selesai jika:

1. ~~`cargo build` dan `cargo test` lulus tanpa error.~~ **Tercapai pada v4.0.**
2. `POST /payments` membuat tepat satu payment untuk request idempotent yang sama.
3. Payment, attempt, idempotency record, dan audit event tersimpan konsisten.
4. `GET /payments/{id}` dan search hanya menampilkan data merchant yang terautentikasi.
5. Provider dipilih melalui service, bukan dipanggil langsung dari route.
6. Provider rejection dan transient/ambiguous errors dipetakan secara berbeda.
7. Automated test membuktikan happy path, duplicate request, dan concurrent request.
8. `cargo fmt`, `cargo clippy`, dan `cargo test` lulus di CI (fmt dan test sudah lulus lokal; clippy belum dijalankan; CI belum ada).

## 7. Risiko dan Blocker Saat Ini

| Risiko / blocker | Dampak | Tindakan |
| --- | --- | --- |
| Manual retry/reconcile handlers masih `NOT_IMPLEMENTED` | Operations tidak bisa retry atau rekonsiliasi payment lewat HTTP — create/get/search/cancel sudah jalan | Kerjakan P0 §5 item 10 (retry/reconciliation orchestration, P1) |
| `cancel_payment` tidak memberi tahu provider | Kalau provider masih memproses pembayaran di sisi mereka, status lokal jadi CANCELLED tapi provider tidak tahu — berpotensi payment tetap sukses di provider padahal sudah "dibatalkan" di sistem kita | Verifikasi requirement sebenarnya; kalau perlu, tambah `PaymentProvider::cancel_payment` dan panggil dari `PaymentService::cancel_payment` |
| Create/Get/Search/Cancel payment, authentication & idempotency middleware, migration seed belum ditest terhadap database sungguhan | Bug logic (mis. salah tangani expired/revoked key, race condition pada lock, migration SQL yang tidak sesuai skema, atau DTO mapping yang tidak cocok skema DB) berpotensi belum terdeteksi meski unit test pure-logic/fake-repo lulus | Jalankan `sqlx migrate run` + hit endpoint sungguhan begitu Postgres/Redis tersedia |
| Idempotency middleware masih belum bisa menyimpan response (sisi tulis) | Duplicate request kedua **tidak akan** di-replay dari cache — atomic transaction (v10.0) sudah menyelesaikan setengah masalah (payment+attempt+audit atomic), tapi `idempotency_keys` masih di luar transaksi itu | Kerjakan P0 §5 item 12 — perlu mengalirkan request hash + response body ke `PaymentService`/`PgPaymentTransactionRepository` |
| Atomic transaction belum ditest terhadap Postgres sungguhan | `BEGIN`/`COMMIT`/`ROLLBACK` di `PgPaymentTransactionRepository` baru diverifikasi lewat fake trait di unit test, belum lewat Postgres nyata | Jalankan integration test begitu Postgres tersedia |
| README lama menandai beberapa fitur runtime sebagai selesai | Ekspektasi pengguna tidak sesuai kondisi kode | Gunakan report ini sebagai sumber status; sinkronkan README berikutnya |
| Automated test masih terbatas (57 unit test — lihat §8) | Regression dan correctness belum terukur untuk domain/API/webhook/middleware end-to-end | Tambahkan test bersamaan dengan setiap use case di P0-P2 |
| `cargo clippy` belum pernah dijalankan | Lint issue/anti-pattern berpotensi belum terdeteksi | Jalankan `cargo clippy` sebelum CI dibuat |
| Timeout tanpa reconciliation | Risiko duplicate transaction saat fallback | Larang fallback otomatis sampai reconciliation tersedia |
| Security layer masih skeleton | Endpoint belum aman diekspos | Jangan deploy ke production |

## 8. Hasil Verifikasi

| Pemeriksaan | Hasil |
| --- | --- |
| `cargo build` (lib + bin + `gen_api_key`) | **Lulus** — 0 error, hanya warning kosmetik (`unused variable`, `dead_code` pada fungsi yang memang belum dipakai) |
| `cargo test` | **Lulus** — 57/57 test passed (5 provider status-mapping + 8 security + 3 idempotency middleware + 11 `application::payment` termasuk search/cancel + 10 DTO mapping termasuk cancel/search + 7 error mapping handler termasuk AlreadyFinal/InvalidTransition + 8 request validation + 5 `domain::rules`); 0 failed |
| `cargo fmt -- --check` | **Lulus**, tidak ada isu format |
| `cargo clippy` | Belum dijalankan pada pass ini — masuk backlog P3 |
| `gen_api_key` tool | Dijalankan manual 2× untuk generate hash yang di-embed di migration seed; output diverifikasi cocok format `key_prefix`/Argon2 PHC yang diharapkan repository |
| Authentication middleware, idempotency middleware, migration seed, create/get/search/cancel payment, atomic transaction end-to-end | **Belum diverifikasi** — tidak ada Postgres/Docker di environment ini. `PaymentService` (termasuk `PaymentTransactionRepository`), request validation, dan DTO/error-mapping tertest lewat fake repository/provider dan pure function (bukan DB/HTTP sungguhan) |
| Source scan untuk TODO/stub | Ditemukan pada retry/reconcile payment routes, webhook route, application services (audit/provider/reconciliation/webhook), dan test file (`tests/api`, `tests/providers`) |
| Payment API runtime implementation | `POST /payments`, `GET /payments/{id}`, `GET /payments` (search), `POST /payments/{id}/cancel` tersambung ke `PaymentService` (§3.4); retry/reconcile masih `NOT_IMPLEMENTED` |
| Automated test implementation | 57 test: provider (5) + security (8) + idempotency middleware (3) + payment application service (11) + DTO response mapping (10) + handler error-code mapping (7) + request validation (8) + domain rules (5); domain unit test masih parsial (cuma `rules.rs`), API integration/webhook/concurrency test belum ada |
| Production readiness | Tidak siap |

## 9. Changelog

| Versi | Tanggal | Perubahan |
| --- | --- | --- |
| 1.0 | 13 September 2026 | Report awal foundation |
| 1.1 | 13 September 2026 | Update repository layer |
| 2.0 | 17 September 2026 | Audit ulang berdasarkan implementasi aktual, klasifikasi selesai/parsial/belum, dan penambahan backlog fallback |
| 3.0 | 17 September 2026 | Audit ulang dengan `cargo build`/`cargo test --no-run`/`cargo fmt -- --check` sungguhan (registry tersedia); ditemukan root cause konkret untuk 12 compile error; status Redis client setup dan Redis lock helper diturunkan dari Selesai ke Parsial; ditambahkan daftar P0-blocker |
| 4.0 | 17 September 2026 | Seluruh 12 compile error v3.0 diperbaiki, ditambah 3 bug baru yang baru terlihat setelah lib compile: (1) `main.rs` memanggil `api::routes::build_router` padahal fungsinya di `api::build_router`; (2) `settings` dipakai setelah di-*move* ke `AppState::new` (borrow checker error); (3) koreksi diagnosis v3.0 — `auth_layer`/`idempotency_layer`/`request_id_layer` ternyata **sudah** didefinisikan (di `middleware/mod.rs` sebagai `Identity` placeholder), bug sebenarnya adalah `api/mod.rs` memanggil path submodule yang salah, bukan fungsi yang "tidak pernah didefinisikan" seperti klaim v3.0. Juga mengimplementasikan `logging::init()` dan `metrics::init()`/`render()` secara nyata (bukan sekadar stub) karena `main.rs` sudah memanggilnya. `cargo build`, `cargo test` (5/5), dan `cargo fmt -- --check` semuanya lulus untuk pertama kalinya |
| 5.0 | 17 September 2026 | Implementasi P0 pertama: authentication middleware. Menambahkan `security::hash::{hash_secret,verify_secret}` (Argon2id, constant-time), `security::api_key::{parse_bearer_token,key_prefix,MerchantContext}`, dan `api::middleware::authentication::require_api_key` (validasi Bearer token, lookup + verifikasi hash, cek merchant aktif, attach `MerchantContext`). Dipasang hanya pada payment routes via `from_fn_with_state`, bukan global. `auth_layer()` placeholder dihapus dari `middleware/mod.rs`. 8 unit test baru (13/13 total lulus). Belum ditest end-to-end karena tidak ada Postgres/Docker di environment ini dan migration tidak seed `api_keys` |
| 6.0 | 17 September 2026 | Implementasi P0 kedua: idempotency middleware (sisi baca). Menambahkan `api::middleware::idempotency::require_idempotency_key` — wajibkan `Idempotency-Key` pada POST, hash body (SHA-256), replay cached response untuk duplikat identik, `409 IDEMPOTENCY_MISMATCH` untuk payload berbeda, `409 IDEMPOTENCY_IN_PROGRESS` via Redis lock (`infrastructure::redis::lock`, sekarang benar-benar dipakai) untuk request konkuren. Dipasang di `api/mod.rs` setelah auth middleware (urutan layer penting: layer yang ditambahkan terakhir jadi terluar dan jalan duluan). `idempotency_layer()` placeholder dihapus dari `middleware/mod.rs`. Sisi tulis (`IdempotencyRepository::save`) sengaja belum diimplementasikan — butuh `payment_id` yang baru ada setelah atomic transaction (P0 lain) selesai. 3 unit test baru (16/16 total lulus). Belum ditest end-to-end karena tidak ada Postgres/Docker di environment ini |
| 7.0 | 17 September 2026 | Dua item P0: (1) Seed/tooling API key — `src/bin/gen_api_key.rs` (CLI generate+hash key pakai fungsi Argon2 yang sama dengan auth middleware) dan migration `20260917_002_seed_demo_api_keys.sql` yang men-seed `sk_live_demo_key_001`/`sk_live_ops_key_001` (cocok contoh di README.md) ke dua merchant yang sudah ada. (2) `application::payment::PaymentService` — `create_payment` (validasi, pilih provider naif "first available", panggil provider, simpan payment+attempt+audit) dan `get_payment` (fetch + `Payment::try_from(PaymentRow)` baru di `domain/payment.rs`), didesain decoupled dari `AppState` supaya testable dengan fake repository/provider. Belum disambungkan ke `src/api/routes/payment.rs` (item P0 terpisah), dan 3 langkah persist belum dibungkus transaksi (item P0 terpisah lain). 6 unit test baru (22/22 total lulus) |
| 8.0 | 17 September 2026 | Sambungkan `src/api/routes/payment.rs::create_payment`/`get_payment` ke `PaymentService`: extract `MerchantContext` (auth) + `IdempotencyKey` (idempotency middleware, baru attach extension ini) dari request extensions, panggil service, map `Payment`→`PaymentResponse` via `impl From<Payment>` baru di `api/dto/payment.rs`, balas `201`/`200`/`404 PAYMENT_NOT_FOUND` (kode sesuai API contract). `ApplicationError` dipetakan ke kode error API (`map_application_error`/`map_get_payment_error`) — `Validation`→400, `Conflict`→409, `Provider`→502, `NoProviderAvailable`→503. `AppState` sekarang membangun `PaymentService` sekali di `AppState::new`. Search/cancel/retry/reconcile TIDAK disentuh (`PaymentService` belum punya method untuk itu). 7 unit test baru — 2 DTO mapping, 5 error mapping (29/29 total lulus). Belum ditest terhadap Postgres/Redis sungguhan |
| 9.0 | 17 September 2026 | Implementasi P0 request validation. Menambahkan `CreatePaymentRequest::validate()` di `api/dto/payment.rs` — `merchant_reference` (non-kosong, ≤255 char), `amount` (>0, field-specific selain cek `Money::new`), `currency` (3 huruf besar), `description` (≤1000 char), `customer.email` (bentuk longgar) — dipanggil di `create_payment` sebelum memanggil `PaymentService`; semua error field dikumpulkan sekaligus ke satu `400 VALIDATION_ERROR` dengan `details` per-field. `Idempotency-Key` di `api/middleware/idempotency.rs` sekarang juga dibatasi ≤255 karakter (cocok kolom DB). `CancelPaymentRequest` sengaja belum divalidasi (handler cancel belum aktif, validasi di sana akan inert). 8 unit test baru (37/37 total lulus). Belum ditest terhadap Postgres/Redis sungguhan |
| 10.0 | 17 September 2026 | Implementasi P0 atomic transaction. Trait baru `domain::repositories::PaymentTransactionRepository::create_with_attempt_and_audit`, diimplementasikan di `PgPaymentTransactionRepository` (`infrastructure/postgres/repositories.rs`) memakai `pool.begin()`/`tx.commit()` sungguhan. Query INSERT payment/attempt/audit diekstrak jadi fungsi executor-generic (`insert_payment`/`insert_attempt`/`insert_audit_log`, bertipe `E: sqlx::Executor<'e, Database = Postgres>`) supaya dipakai bersama oleh repo non-transactional (`&PgPool`) dan repo transactional baru (`&mut *tx`) tanpa duplikasi SQL. `PaymentService` diganti field-nya: `attempt_repo`/`audit_repo` dihapus, diganti `payment_tx: Arc<dyn PaymentTransactionRepository>` — `create_payment` sekarang panggil satu method atomic, bukan 3 panggilan repo terpisah. `idempotency_keys` sengaja **belum** ikut transaksi ini (butuh request hash dari middleware + response body dari handler, keduanya tidak tersedia di `PaymentService`) — dicatat sebagai P0 item terpisah. 6 test lama diganti test terhadap fake `PaymentTransactionRepository`, ditambah 1 test baru untuk kegagalan transaksi (38/38 total lulus). Belum ditest terhadap Postgres sungguhan |
| 11.0 | 18 September 2026 | Sambungkan `GET /payments` (search) dan `POST /payments/{id}/cancel` ke `PaymentService`. Search: `PaymentService::search_payments` + `SearchPaymentParams::into_filter()` (default page/limit, bound `1..=100`, parse RFC 3339 date); ditemukan+diperbaiki bug lama — `PgPaymentRepository::search` menerima `from_date`/`to_date` di `SearchCriteria` tapi tidak pernah memfilternya, sekarang query SELECT+COUNT memfilter `created_at` dengan benar. Cancel: `PaymentService::cancel_payment` (fetch merchant-scoped → `transition_to(Cancelled)` → `update_status` → audit log best-effort); `domain::rules::validate_transition` direfactor supaya payment final ditolak dengan `DomainError::AlreadyFinal` (varian yang sudah ada tapi belum pernah dipakai) alih-alih `InvalidTransition` generik, sehingga handler bisa balas `422 PAYMENT_ALREADY_FINAL` yang presisi sesuai API contract; `PaymentService` re-menambahkan `audit_repo` sebagai kolaborator (dipakai cancel, bukan create). DTO baru: `CancelPaymentRequest::validate()`, `CancelPaymentResponse`/`CancelPaymentData` (bentuk ringkas sesuai contract), `SearchPaymentParams::into_filter()`, `PaymentSummary`/`SearchPaymentsResponse` conversion. `domain::rules::validate_transition` untuk pertama kalinya punya unit test (5 test). Manual retry dan reconcile TIDAK disentuh. 19 unit test baru (57/57 total lulus). Belum ditest terhadap Postgres sungguhan |
