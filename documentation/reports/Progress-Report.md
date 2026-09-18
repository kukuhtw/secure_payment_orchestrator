# Task Progress Report

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi report | 16.0 |
| Tanggal audit | 18 September 2026 |
| Status produk | Proof of Concept, belum production-ready |
| Dasar penilaian | `cargo build`, `cargo test`, `cargo fmt -- --check` (registry Cargo dapat diakses), ditambah pemeriksaan source code, migration, konfigurasi, dan dokumentasi |
| Verifikasi build | **Lulus.** `cargo build` sukses (lib + bin + `gen_api_key`), `cargo test` sukses (84/84 test lulus), `cargo fmt -- --check` lulus tanpa isu |

## 1. Ringkasan Eksekutif

Fondasi proyek sudah tersedia: struktur aplikasi Rust, domain model, state machine,
PostgreSQL repositories, Redis lock helper, kontrak provider, empat provider adapter
(Midtrans/Alpha, Xendit/Beta, DOKU/Gamma, NICEPAY), konfigurasi, dokumentasi API,
container setup, authentication middleware, idempotency middleware, payment application
service, `POST /payments`/`GET /payments/{id}` (v8.0), request validation (v9.0), atomic
transaction untuk `create_payment` (v10.0), `GET /payments` (search) dan
`POST /payments/{id}/cancel` (v11.0), `POST /payments/{id}/retry` (v12.0), dan — baru
pada pass ini — **`POST /payments/{id}/reconcile`** yang benar-benar berfungsi. Dengan
ini, **seluruh 6 route payment di `src/api/routes/payment.rs` sudah tersambung ke
`PaymentService`** — tidak ada lagi handler `NOT_IMPLEMENTED`.

Pass v13.0 mengimplementasikan manual reconciliation, endpoint operations-only per API
contract dan architecture doc §3.3 ("Query status ke provider untuk payment uncertain"):

- **Eligibility ketat lewat `PaymentStatus::needs_reconciliation()`.** Sama seperti
  pelajaran dari retry (v12.0): route lewat `validate_transition` akan salah, jadi
  `reconcile_payment` langsung memakai helper domain yang sempit (`needs_reconciliation()`
  → status harus persis `PENDING_RECONCILIATION`) alih-alih `validate_transition`. Selain
  konsisten, ini juga jujur soal limitasi: **belum ada apa pun di codebase ini yang
  mentransisikan payment KE status itu** (tidak ada worker timeout/retry-exhaustion) —
  jadi endpoint ini sudah benar secara implementasi tapi belum bisa dipicu lewat flow
  lain mana pun. Dicatat sebagai gap yang disengaja, bukan bug.
- **Query provider, bukan re-create.** `reconcile_payment` ambil attempt terakhir
  (`AttemptRepository::get_by_payment_id`, diambil yang `attempt_number` terbesar),
  lalu panggil `PaymentProvider::get_payment_status(provider_payment_id)` ke provider
  yang sama dipakai attempt itu. Hasil dipetakan: `COMPLETED`→`Success`,
  `FAILED`→`Failed`, selain itu (termasuk `PENDING`)→tetap `PENDING_RECONCILIATION`
  dengan resolution `UNCERTAIN`.
- **Provider error TIDAK propagate seperti create/retry.** Berbeda dari
  `create_payment`/`retry_payment` (yang meneruskan error provider ke caller dan tidak
  menyimpan apa pun), kegagalan provider saat reconcile direkam sebagai resolution
  `UNCERTAIN` dengan detail error di `reconciliation_records.details` — tujuan
  reconciliation adalah mencatat apa yang berhasil dipelajari (atau bahwa tidak ada
  yang berhasil dipelajari), bukan kehilangan jejak bahwa upaya sudah dilakukan.
- **Atomic, tabel baru `reconciliation_records`.** Method baru
  `PaymentTransactionRepository::reconcile` (satu `sqlx::Transaction`) selalu insert
  baris `reconciliation_records`, dan — hanya kalau status provider berhasil di-resolve
  (`Some`) — ikut update `payments.status` dalam transaksi yang sama. Kalau masih
  `UNCERTAIN`, hanya record yang tersimpan, payment tetap `PENDING_RECONCILIATION`.
- **Sengaja TIDAK diimplementasikan:** insert baris `payment_attempts` baru untuk query
  reconciliation (ini query status, bukan payment attempt baru, meski
  `AttemptType::Reconciliation` sudah ada di domain model — dicatat sebagai keputusan
  scope, worth ditinjau ulang kalau kontrak sebenarnya butuh jejak attempt terpisah);
  insert `audit_logs` untuk reconcile (`reconciliation_records` sendiri berfungsi
  sebagai audit trail yang lebih tepat untuk aksi ini); dan Redis distributed lock
  `reconcile:{payment_id}` yang disebut architecture doc (`PaymentService` belum punya
  dependency Redis sama sekali — menambahkannya hanya untuk endpoint ini adalah
  perubahan struktural yang lebih besar dari yang dibutuhkan pass ini).

7 unit test baru untuk `reconcile_payment` (resolve ke Success, resolve ke Failed, tetap
UNCERTAIN saat provider PENDING, tetap UNCERTAIN saat provider error tanpa propagate,
status tidak reconcilable, not-found) ditambah 1 test untuk error-code mapping
(`NotReconcilable` → `409 PAYMENT_NOT_RECONCILABLE`).

Pass v14.0 menutup gap P0 lama: idempotency write-side. Sejak v6.0, middleware idempotency
hanya menangani sisi baca (replay/conflict detection); sisi tulis — insert baris
`idempotency_keys` — belum pernah diimplementasikan sama sekali:

- **`PaymentTransactionRepository::create_with_attempt_and_audit`** dapat parameter baru
  `idempotency: Option<&IdempotencyRow>` — kalau `Some`, baris `idempotency_keys` ikut
  di-insert dalam transaksi atomic yang sama dengan payment/attempt/audit (satu
  `sqlx::Transaction`, konsisten dengan pola create/retry/reconcile sebelumnya).
- **`request_hash` mengalir dari middleware ke handler.** `IdempotencyKey` (extension
  request) berubah dari tuple struct 1-field jadi struct dengan `key` dan `request_hash`
  — middleware sudah menghitung SHA-256 hash itu untuk deteksi duplikat, sekarang juga
  diteruskan ke `create_payment` handler lewat `CreatePaymentInput::request_hash`, alih-alih
  hash dihitung ulang atau tidak pernah dipakai.
- **Response body lewat closure, bukan `application` bergantung ke `api::dto`.**
  `PaymentService::create_payment` dapat parameter baru `response_snapshot: impl FnOnce(&Payment) -> serde_json::Value`
  — handler HTTP menyediakan closure yang membangun `PaymentResponse` JSON dari `Payment`
  yang sudah lengkap (id, provider, payment_url) tapi belum di-insert, dipanggil pas
  sebelum commit untuk mengisi `idempotency_keys.response_body`. Ini menjaga layering
  (`application` tidak boleh import tipe `api`) tanpa duplikasi shape JSON di dua tempat.
- **`expires_at` di-set eksplisit (24 jam)** dari Rust, bukan mengandalkan DB `DEFAULT`,
  konsisten dengan pola insert_* helper lain yang selalu bind semua kolom.
- **Helper baru `insert_idempotency_row`** (executor-generic) diekstrak dari
  `PgIdempotencyRepository::save` yang sudah ada, dipakai bersama oleh path non-transactional
  (`&PgPool`) dan path atomic baru (`&mut *tx`) — pola sama seperti `insert_payment`/
  `insert_attempt`/`insert_audit_log`/`insert_reconciliation_record`.
- **Sengaja TIDAK diimplementasikan:** `ON CONFLICT` handling untuk kasus langka idempotency
  key yang sama dipakai ulang setelah baris lama expired tapi belum dibersihkan (constraint
  PK `(idempotency_key, merchant_id)` bisa bentrok) — insert tetap plain `INSERT`, sama
  seperti `PgIdempotencyRepository::save` sebelumnya; dicatat sebagai edge case yang sangat
  sempit (24 jam TTL, tidak ada cleanup job), bukan diperbaiki pass ini.

1 test yang sudah ada (`create_payment_persists_payment_attempt_and_audit_atomically`)
diperluas dengan assertion baru untuk memverifikasi baris `idempotency_keys` yang
di-generate — tidak ada test baru ditambahkan di pass itu, jadi total tetap 73/73
sampai v14.0.

Pass v15.0 menutup item P0 terakhir yang masih open: provider selection berbasis
prioritas. **Dengan ini, seluruh 13 item P0 di §5 sudah Selesai** — tidak ada lagi
core payment flow item yang belum dikerjakan sama sekali (masih banyak yang belum
ditest terhadap database sungguhan, tapi itu limitasi lingkungan, bukan pekerjaan
yang belum dimulai):

- **`PaymentProvider` trait dapat method baru `priority() -> i32`** (`providers/
  adapter.rs`) — wajib diimplementasikan semua 4 adapter (Midtrans/Xendit/DOKU/
  NICEPAY). Nilai lebih kecil menang.
- **Priority datang dari config, bukan hardcoded.** `Settings` dapat 4 field baru
  (`midtrans_priority`, `xendit_priority`, `doku_priority`, `nicepay_priority`,
  env var `*_PRIORITY`) dengan default yang menjaga urutan lama (10/20/30/40 —
  Midtrans < Xendit < DOKU < NICEPAY) supaya perilaku tidak berubah kalau tidak
  dikonfigurasi ulang.
- **`select_best_provider` baru** (`application/payment.rs`) — di antara provider
  yang `is_available()`, pilih yang `priority()`-nya terkecil
  (`.filter(...).min_by_key(...)`), dipakai bersama oleh `create_payment` dan
  `retry_payment` (sebelumnya masing-masing punya `.find(|p| p.is_available())`
  sendiri-sendiri, yang secara implisit berarti "provider pertama di `Vec`").
- **Sengaja TIDAK termasuk: circuit breaker awareness.** `select_best_provider`
  murni priority + `is_available()` (yang sendiri cuma cek konfigurasi, bukan
  circuit breaker sungguhan) — provider yang sedang gagal berulang kali di
  runtime tapi masih terkonfigurasi tetap dianggap available dan bisa terpilih.
  Circuit breaker (state machine CLOSED/OPEN/HALF_OPEN, §6.3 architecture doc)
  tetap P1 item #4, item terpisah dari seleksi prioritas ini.

2 unit test baru: memverifikasi provider dengan priority value lebih kecil menang
walau didaftarkan belakangan (bukan "first in Vec"), dan provider unavailable
di-skip meski priority-nya lebih baik daripada provider available lain.

Pass v16.0 memulai backlog P1: circuit breaker per provider (§5 P1 item 4), state
machine yang confignya sudah ada sejak awal proyek (`circuit_breaker_threshold`,
`circuit_breaker_timeout_seconds`) tapi tidak pernah dipakai runtime mana pun:

- **`CircuitBreakerProvider` baru** (`providers/circuit_breaker.rs`) — dekorator yang
  membungkus `Box<dyn PaymentProvider>` mana pun, bukan menyentuh 4 adapter yang ada.
  State CLOSED (normal) / OPEN (>= `failure_threshold` kegagalan berturut-turut,
  langsung dianggap unavailable) / HALF_OPEN (setelah `open_duration` lewat, izinkan
  1 probe — sukses → CLOSED, gagal → OPEN lagi), sesuai §6.3 architecture doc persis
  (default 5 kegagalan, 30 detik).
- **`providers::build_providers` membungkus keempat adapter** dengan ini — jadi
  `PaymentService::select_best_provider` (v15.0) otomatis jadi circuit-breaker-aware
  tanpa menyentuh `application/payment.rs` sama sekali (dekorator transparan di
  belakang trait `PaymentProvider` yang sama).
- **HALF_OPEN disederhanakan** — direpresentasikan sebagai "OPEN yang durasinya sudah
  lewat" (dihitung `Instant::elapsed()` on-the-fly di `is_available()`, bukan state
  tersimpan terpisah), bukan 3 variant literal. Ini artinya beberapa request konkuren
  yang datang tepat setelah durasi OPEN lewat semua bisa lolos sebagai probe — bukan
  ditegakkan ketat "1 request saja". Trade-off yang disengaja: opsi yang benar butuh
  flag "probe sedang berlangsung" tambahan untuk single-process concurrency, dianggap
  di luar scope untuk breaker in-memory sederhana ini.
- **In-memory per-proses saja** — bukan Redis/DB. Deployment multi-instance perlu
  state bersama untuk koordinasi circuit breaker lintas proses; di luar scope pass ini.

9 unit test baru untuk `CircuitBreakerProvider` (closed by default, delegasi
name/priority, tetap unavailable kalau inner unavailable, tetap closed di bawah
threshold, terbuka setelah threshold tercapai, counter reset saat sukses, probe
terbuka lagi setelah durasi lewat, probe gagal membuka ulang, probe sukses menutup).

| Status | Jumlah task | Persentase |
| --- | ---: | ---: |
| Selesai | 35 | 58% |
| Parsial | 13 | 22% |
| Belum | 12 | 20% |
| **Total** | **60** | **100%** |

Dibanding v15.0 (34 selesai / 13 parsial / 13 belum), satu task naik dari Belum ke
Selesai: "Circuit breaker" (§3.5). Tidak ada task yang turun status.

Persentase di atas adalah hitungan task pada report ini, bukan estimasi LOC atau klaim
kesiapan production. Circuit breaker baru diverifikasi lewat unit test dengan fake
provider dan real (non-mocked) wall-clock time — belum pernah diuji terhadap provider
sungguhan yang benar-benar gagal berulang kali. Idempotency write-side (v14.0) dan
provider selection (v15.0) juga masih **belum ditest terhadap Postgres/environment
sungguhan**. Reconcile (v13.0) juga masih belum bisa dipicu lewat flow lain mana pun.

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
| Repository contracts | Selesai | 7 trait (payment, `PaymentTransactionRepository` baru v10.0, attempt, API key, idempotency, audit, webhook) di `src/domain/repositories.rs`. Sejak v13.0, `PaymentTransactionRepository` dapat method baru `reconcile`, dan struct baru `ReconciliationRecordRow` (mapping tabel `reconciliation_records`) |
| PostgreSQL repository implementation | Selesai | Diperbaiki: `PaymentAttemptRow` ditambahkan ke import di `repositories.rs`, dan `domain/attempt.rs` memakai `row.status.as_str()` / `row.attempt_type.as_str()` (bukan `&row.status`) agar cocok dengan `impl From<&str>`. Query CRUD/search/count untuk semua 6 repository lengkap dan compile bersih. Sejak v10.0, INSERT payment/attempt/audit diekstrak jadi fungsi executor-generic (`insert_payment`/`insert_attempt`/`insert_audit_log`) dipakai bersama oleh repo biasa (`&PgPool`) dan `PgPaymentTransactionRepository` (`&mut *tx`). Sejak v11.0, query `search` (SELECT dan COUNT) memfilter `created_at` berdasar `from_date`/`to_date` — sebelumnya kedua field itu ada di `SearchCriteria` tapi diam-diam diabaikan di SQL. Sejak v13.0, `insert_reconciliation_record` (executor-generic, pola sama seperti helper insert_* lain) dipakai `PgPaymentTransactionRepository::reconcile` untuk insert `reconciliation_records` dan, kalau status ter-resolve, update `payments.status` — satu transaksi |
| Atomic business transaction | Selesai (naik dari Parsial) | `PgPaymentTransactionRepository::create_with_attempt_and_audit` memakai `pool.begin()`/`tx.commit()` sungguhan — payment, attempt awal, audit log, dan (sejak v14.0) `idempotency_keys` tersimpan dalam satu transaksi yang sama, lihat baris "Idempotency middleware" di §3.4. **Belum ditest terhadap Postgres sungguhan** |
| Concurrency protection | Parsial | Redis lock helper (§3.6) kini dipakai nyata oleh idempotency middleware (§3.4) untuk mencegah request konkuren dengan `Idempotency-Key` sama diproses bersamaan; belum dipakai oleh webhook/reconciliation flow — `application/*.rs` masih skeleton satu baris |

### 3.4 Core Payment API — 9 selesai, 0 parsial, 0 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Request/response DTO | Selesai | Struct lengkap; `impl From<Payment> for PaymentResponse` (v8.0) dipakai handler sungguhan. `CreatePaymentRequest::validate()` (v9.0) dan `CancelPaymentRequest::validate()` (baru v11.0, cek panjang `reason` ≤1000 char) dipanggil sebelum service. `SearchPaymentParams::into_filter()` (baru v11.0) apply default page/limit dan parse tanggal RFC 3339 |
| Authentication middleware | Selesai | `src/api/middleware/authentication.rs::require_api_key` — validasi `Authorization: Bearer`, lookup `key_prefix`, verifikasi Argon2 constant-time (`security::hash::verify_secret`), cek merchant `ACTIVE`, attach `MerchantContext` ke request extensions. Dipasang hanya pada payment routes via `axum::middleware::from_fn_with_state` di `api/mod.rs` (bukan global — health/ready/metrics/webhook tidak terpengaruh). **Belum ditest end-to-end** (tidak ada Postgres/Docker di environment ini); hanya unit test pure-logic yang lulus (lihat §3.7, §3.9) |
| Idempotency middleware | Selesai | `src/api/middleware/idempotency.rs::require_idempotency_key` — wajibkan header pada POST (maks 255 karakter sejak v9.0, cocok kolom DB), hash body (SHA-256), cek duplikat/conflict ke `IdempotencyRepository` (replay cached response atau `409 IDEMPOTENCY_MISMATCH`), acquire Redis lock untuk cegah request konkuren (`409 IDEMPOTENCY_IN_PROGRESS`). Dipasang setelah auth middleware (butuh `MerchantContext`). `IdempotencyKey` extension (dipakai `create_payment`) sejak v14.0 membawa `request_hash` (bukan cuma `key`), supaya handler tidak perlu hash ulang body. Sisi tulis kini **selesai**: `PaymentService::create_payment` insert baris `idempotency_keys` (key, request_hash, payment_id, response snapshot) dalam transaksi atomic yang sama dengan payment/attempt/audit — lihat §3.3, §3.9. **Belum ditest end-to-end** (alasan sama seperti authentication middleware) |
| Create payment | Selesai | `src/api/routes/payment.rs::create_payment` memanggil `PaymentService::create_payment` dan mengembalikan `201 Created` dengan `PaymentResponse` sungguhan. Error dipetakan ke kode API yang sesuai (`map_application_error`). Persistensi atomic termasuk idempotency record (§3.3, sejak v10.0/v14.0). Handler memberi `PaymentService` sebuah closure (`response_snapshot`) yang membangun JSON body identik dengan yang dikembalikan — dipakai untuk mengisi `idempotency_keys.response_body`, supaya `application` layer tidak perlu bergantung pada tipe DTO `api` layer. **Belum ditest terhadap Postgres/Redis sungguhan** (tidak ada Docker di environment ini) |
| Get payment | Selesai | `get_payment` memanggil `PaymentService::get_payment`, balas `200 OK` atau `404 PAYMENT_NOT_FOUND` (kode+detail sesuai API contract §3.2). **Belum ditest terhadap Postgres sungguhan** |
| Search payment | Selesai (naik dari Belum) | `search_payments` memanggil `PaymentService::search_payments` (wrapper atas `PaymentRepository::search`). Ditemukan+diperbaiki sekalian: SQL `search` sebelumnya menerima `from_date`/`to_date` di `SearchCriteria` tapi **tidak pernah memfilternya** — sekarang query SELECT dan COUNT keduanya memfilter `created_at` dengan benar (lihat §3.3). **Belum ditest terhadap Postgres sungguhan** |
| Cancel payment | Selesai (naik dari Belum) | `cancel_payment` memanggil `PaymentService::cancel_payment` (fetch → `transition_to(Cancelled)` → `update_status` → audit log best-effort), balas `200 OK` dengan `CancelPaymentResponse` baru (`{payment_id, status, cancelled_at}`, bentuk lebih ringkas sesuai API contract §3.4) atau `404`/`422 PAYMENT_ALREADY_FINAL`. **Belum ditest terhadap Postgres sungguhan** |
| Manual retry endpoint | Selesai (naik dari Belum) | `src/api/routes/payment.rs::retry_payment` — cek `MerchantContext::is_operations()` (403 kalau bukan), panggil `PaymentService::retry_payment`, balas `200 OK` dengan `RetryPaymentResponse` (`{payment_id, status, attempt_number, provider, message}` sesuai API contract §3.5). Eligibility pakai `PaymentStatus::is_retryable()`, dibatasi `MAX_RETRY_ATTEMPTS`, atomic via `update_status_with_attempt_and_audit`. Seleksi provider masih naif (sama seperti create). **Belum ditest terhadap Postgres sungguhan** |
| Reconcile payment endpoint | Selesai (naik dari Belum) | `src/api/routes/payment.rs::reconcile_payment` — cek `MerchantContext::is_operations()` (403 kalau bukan), panggil `PaymentService::reconcile_payment`, balas `200 OK` dengan `ReconcileResponse` (`{payment_id, previous_status, current_status, provider_status, resolution, reconciled_at}`). Eligibility pakai `PaymentStatus::needs_reconciliation()` (status harus `PENDING_RECONCILIATION`), query provider dari attempt terakhir, atomic via `PaymentTransactionRepository::reconcile` (lihat §3.3, §3.6). **Belum ditest terhadap Postgres sungguhan, dan belum bisa dipicu lewat flow lain mana pun** (tidak ada worker yang mentransisikan payment ke `PENDING_RECONCILIATION` — lihat §3.6, §7) |

### 3.5 Provider integration dan fallback — 4 selesai, 3 parsial, 2 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Canonical `PaymentProvider` contract | Selesai | `src/providers/adapter.rs` — trait, request, response, error types. Sejak v15.0, trait dapat method baru `priority() -> i32` (wajib diimplementasikan semua adapter) |
| Midtrans/Xendit/DOKU provider adapters | Selesai | Alpha (277 baris), Beta (232 baris), Gamma (346 baris) — Midtrans, Xendit, DOKU Sandbox |
| NICEPAY example adapter | Parsial | Registration/create tersedia (269 baris); inquiry memerlukan referenceNo dan amt yang belum dibawa kontrak status provider |
| Provider availability contract | Parsial | `is_available()` di trait; tiap adapter mentah (Alpha/Beta/Gamma/Nicepay) sendiri masih cuma cek konfigurasi (mis. Server Key kosong) — TIDAK tahu soal circuit breaker. Yang benar-benar dipakai `PaymentService` adalah versi yang dibungkus `CircuitBreakerProvider` (lihat baris "Circuit breaker" di bawah), bukan adapter mentah — makanya baris ini tetap Parsial, bukan Selesai |
| Failover data model | Parsial | `AttemptType::Failover` tersedia; flow belum diimplementasikan |
| Provider selection by availability/priority | Selesai (naik dari Parsial) | `select_best_provider` (`application/payment.rs`, dipakai `create_payment` dan `retry_payment`) sekarang memilih provider dengan `priority()` terkecil di antara yang `is_available()` — bukan lagi "provider pertama di `Vec`". Priority per-provider datang dari config (`Settings::midtrans_priority`/`xendit_priority`/`doku_priority`/`nicepay_priority`, env var `*_PRIORITY`), bukan hardcoded. Sejak v16.0, `is_available()` yang dipanggil di sini genuinely circuit-breaker-aware (lihat baris "Circuit breaker") |
| Timeout wrapper dan response classification | Belum | Belum ada orchestration implementation |
| Circuit breaker | Selesai (naik dari Belum) | `providers::circuit_breaker::CircuitBreakerProvider` (baru v16.0) — dekorator yang membungkus `Box<dyn PaymentProvider>`, state machine CLOSED/OPEN/HALF_OPEN sesuai §6.3 architecture doc (5 kegagalan berturut-turut → OPEN, 30 detik → HALF_OPEN, probe sukses → CLOSED / probe gagal → OPEN lagi). Konsumsi `circuit_breaker_threshold`/`circuit_breaker_timeout_seconds` dari `Settings` — config yang sejak awal ada tapi tidak pernah dipakai runtime mana pun. `providers::build_providers` membungkus keempat adapter dengan ini. State disimpan in-memory per-proses saja (bukan Redis/DB) — deployment multi-instance butuh state bersama, di luar scope. HALF_OPEN direpresentasikan sebagai "OPEN yang durasinya sudah lewat" (dihitung dari `Instant::elapsed()`, bukan state tersimpan terpisah) — simplifikasi yang disengaja: circuit breaker literal 3-state butuh flag "probe sedang berlangsung" tambahan untuk menegakkan "izinkan tepat 1 request" di bawah pemanggil konkuren, yang tidak dicoba diimplementasikan di sini. 9 unit test baru |
| Automatic fallback A ke B | Belum | Belum ada routing, safe-failure decision, atau failover execution |

### 3.6 Reliability dan reconciliation — 1 selesai, 1 parsial, 3 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Redis distributed lock helper | Selesai | Diperbaiki: `release_lock` di `src/infrastructure/redis/lock.rs` sebelumnya gagal compile karena never-type-fallback ambiguity pada `invoke_async(...).await?`; fix dengan anotasi eksplisit `let _: () = script...invoke_async(redis).await?;`. Logic acquire (`SET NX EX`) dan release (ownership-check Lua) sudah benar secara desain dan sekarang compile bersih |
| Retry rules | Parsial | Klasifikasi dan backoff tersedia di domain layer. `MAX_RETRY_ATTEMPTS` sekarang benar-benar ditegakkan oleh `PaymentService::retry_payment` (v12.0, manual/synchronous) — sebelumnya konstanta ini tidak dipakai di mana pun. `retry_delay_seconds` (exponential backoff) masih belum dipakai — itu untuk worker otomatis (P1) yang belum ada |
| Retry worker dan max-attempt execution | Belum | Belum ada worker implementation |
| Reconciliation service (otomatis/worker) | Belum | `src/application/reconciliation.rs` masih hanya doc comment satu baris — tidak ada worker/trigger otomatis yang mendeteksi payment uncertain dan mentransisikannya ke `PENDING_RECONCILIATION`. Manual reconciliation lewat HTTP endpoint sudah selesai (lihat §3.4 "Reconcile payment endpoint") dan logic-nya ada langsung di `PaymentService::reconcile_payment` (`application/payment.rs`, pola sama seperti cancel/retry) — tapi karena tidak ada apa pun yang men-set status `PENDING_RECONCILIATION`, endpoint manual itu belum reachable lewat flow lain mana pun |
| Late-webhook conflict handling | Belum | Belum ada processing implementation |

### 3.7 Security dan webhook — 1 selesai, 2 parsial, 3 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Security algorithm/design | Parsial | Bagian API key (Argon2 + constant-time) sudah diimplementasikan; bagian HMAC-SHA256 webhook (`security/webhook_sig.rs`) masih doc comment intent saja |
| API-key hashing dan verification | Selesai | `src/security/hash.rs::hash_secret`/`verify_secret` (Argon2id, `PasswordVerifier` yang inheren constant-time) dan `src/security/api_key.rs::parse_bearer_token`/`key_prefix`; 8 unit test lulus (roundtrip, wrong-secret, malformed hash, bearer parsing, key prefix) |
| Merchant authentication/authorization | Parsial | Merchant diidentifikasi dan discoping via `MerchantContext` setelah API key tervalidasi. Sejak v12.0, otorisasi granular per-permission **sudah ada implementasinya**: `MerchantContext::is_operations()` (`operations`/`admin`) dicek di `retry_payment`, dan sejak v13.0 juga di `reconcile_payment` → `403 AUTHORIZATION_FAILED` kalau bukan operations key. Dua endpoint sudah pakai pengecekan ini, tapi masing-masing masih inline di handler-nya sendiri — belum ada pengecekan permission generik (mis. middleware/extractor) yang dipakai bersama di semua route operations |
| Webhook signature verification | Belum | `src/security/webhook_sig.rs` hanya doc comment |
| Webhook replay/duplicate processing | Belum | DB unique constraint tersedia (`uq_webhook_events_provider_event`), tetapi service belum ada |
| Webhook route processing | Belum | Handler mengembalikan `NOT_IMPLEMENTED` |

### 3.8 Observability dan operations — 2 selesai, 2 parsial, 0 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Logging initialization | Selesai | Diimplementasikan: `src/observability/logging.rs::init()` sekarang membangun `tracing_subscriber` dengan format JSON dan `EnvFilter` dari `settings.log_level` (sebelumnya file ini hanya doc comment tanpa fungsi apa pun, menyebabkan `main.rs` gagal compile) |
| Request/correlation ID | Parsial | Diperbaiki hanya path pemanggilan: `api/mod.rs` sekarang memanggil `middleware::request_id_layer()` yang benar-benar ada (bukan `middleware::request_id::request_id_layer()` yang tidak ada), tetap berupa `Identity` pass-through — belum ada generate/propagate `X-Request-ID` sungguhan |
| Prometheus instrumentation/export | Parsial (naik dari Belum) | Diimplementasikan: `src/observability/metrics.rs::init()` memasang `PrometheusBuilder` recorder global, dan `GET /metrics` (`api/routes/metrics.rs`) merender snapshot asli via `render()`. Belum ada metric (`spo_payments_total`, `spo_payment_duration_ms`) yang benar-benar direkam karena application layer belum memanggilnya — endpoint akan menampilkan output kosong sampai instrumentasi ditambahkan di P1/P2 |
| Operational payment actions | Selesai (koreksi retroaktif) | Cancel (v11.0), retry (v12.0), dan reconcile (v13.0) semuanya sudah bekerja lewat `PaymentService` (lihat §3.4) — baris ini masih salah menyatakan "Belum" di report v11.0/v12.0 meski cancel dan retry sudah selesai saat itu; diperbaiki sekarang saat reconcile melengkapi ketiganya |

### 3.9 Quality assurance dan delivery — 2 selesai, 2 parsial, 4 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| API contract | Selesai | `documentation/api/API-Contract-Secure-Payment-Orchestrator.md` |
| OpenAPI specification | Selesai | `documentation/api/openapi.yaml` |
| Domain unit tests | Parsial (naik dari Belum) | `src/domain/rules.rs` sekarang punya 5 unit test (`validate_transition`: transisi valid, non-final→Cancelled, final→AlreadyFinal untuk ketiga status final, transisi terlarang→InvalidTransition). `payment.rs`, `status.rs`, `attempt.rs`, `error.rs` masih belum punya test langsung — `Payment::try_from(PaymentRow)` cuma tercakup tidak langsung lewat test `application::payment` |
| API integration tests | Belum | `tests/api/mod.rs` masih TODO |
| Provider tests | Parsial | 5 unit test di provider layer (Midtrans/Xendit/DOKU/NICEPAY status mapping) — semuanya **lulus** via `cargo test`; `tests/providers/mod.rs` (integration) masih TODO. Di luar provider, ada 68 unit test lain (10 security termasuk `is_operations`, 3 idempotency middleware, 22 `application::payment` termasuk retry dan reconcile, 11 DTO mapping termasuk retry response, 9 error mapping handler termasuk MAX_RETRY_REACHED dan PAYMENT_NOT_RECONCILABLE, 8 request validation, 5 `domain::rules`) — total 73/73 lulus |
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
14. `spo-api` compile bersih: `cargo build`, `cargo test` (73/73 lulus), dan `cargo fmt -- --check` semuanya lulus.
15. Authentication middleware — Argon2 API-key verification, merchant context, dipasang hanya pada payment routes (belum ditest end-to-end, lihat §3.4 dan §7).
16. Idempotency middleware (sisi baca) — duplicate/conflict detection dan Redis lock concurrency guard, sekarang juga membatasi panjang `Idempotency-Key` (≤255 karakter).
17. `src/bin/gen_api_key.rs` — tool generate/hash API key, dipakai untuk membuat 2 seed key di migration `20260917_002_seed_demo_api_keys.sql` (cocok contoh `sk_live_demo_key_001`/`sk_live_ops_key_001` di README.md).
18. `application::payment::PaymentService` (`create_payment`, `get_payment`) — lengkap dan tertest dengan fake repository/provider.
19. `POST /payments` dan `GET /payments/{id}` benar-benar tersambung ke `PaymentService` — bukan lagi `NOT_IMPLEMENTED` (lihat §3.4). Belum ditest terhadap Postgres/Redis sungguhan.
20. `CreatePaymentRequest::validate()` — required-field, length, dan format checks (merchant_reference, amount, currency, description, customer.email), dipanggil di `create_payment` sebelum service; error field terkumpul sekaligus dalam satu `400 VALIDATION_ERROR`.
21. Atomic transaction untuk `create_payment` — `PgPaymentTransactionRepository` membungkus insert payment + attempt + audit log dalam satu `sqlx::Transaction` (lihat §3.3). Belum ditest terhadap Postgres sungguhan.
22. `GET /payments` (search) dan `POST /payments/{id}/cancel` tersambung ke `PaymentService` — bukan lagi `NOT_IMPLEMENTED` (lihat §3.4). Termasuk perbaikan bug lama: filter `from_date`/`to_date` di search sekarang benar-benar dipakai SQL. Belum ditest terhadap Postgres sungguhan.
23. `domain::rules::validate_transition` — payment final ditolak dengan `DomainError::AlreadyFinal` (bukan `InvalidTransition` generik), sekarang punya 5 unit test (sebelumnya nihil).
24. `POST /payments/{id}/retry` tersambung ke `PaymentService::retry_payment` — bukan lagi `NOT_IMPLEMENTED` (lihat §3.4). Termasuk implementasi pertama authorization operations-only (`MerchantContext::is_operations()`, §3.7) dan `MAX_RETRY_ATTEMPTS` yang benar-benar ditegakkan (§3.6). Belum ditest terhadap Postgres sungguhan.
25. `POST /payments/{id}/reconcile` tersambung ke `PaymentService::reconcile_payment` — bukan lagi `NOT_IMPLEMENTED` (lihat §3.4). **Seluruh 6 route payment kini tersambung ke `PaymentService`, tidak ada lagi handler `NOT_IMPLEMENTED`.** Operations-only (`403` kalau bukan), eligibility via `PaymentStatus::needs_reconciliation()`, atomic via `PaymentTransactionRepository::reconcile` (tabel baru `reconciliation_records`, §3.3). Provider error direkam sebagai `UNCERTAIN` alih-alih di-propagate. Belum ditest terhadap Postgres sungguhan, dan belum reachable lewat flow lain mana pun karena belum ada worker yang men-set status `PENDING_RECONCILIATION` (§3.6, §7).

## 5. Daftar yang Belum Selesai

Seluruh compile blocker dari v3.0 (12 error + 3 temuan static-review) sudah diperbaiki —
lihat §9 Changelog untuk daftar lengkap dan §3 untuk detail per task. Backlog di bawah ini
murni tentang fitur/business logic yang belum dikerjakan, bukan lagi tentang kode yang
tidak bisa dikompilasi.

### Prioritas P0 — agar core payment flow dapat berjalan

**Seluruh 13 item P0 di bawah ini sudah Selesai per v15.0.** Ini tidak berarti core
payment flow production-ready — lihat catatan "Belum ditest terhadap Postgres/Redis
sungguhan" yang masih menempel di hampir semua item, plus seluruh P1/P2/P3 di bawah —
tapi tidak ada lagi item P0 yang open. Backlog berikutnya ada di P1.

1. ~~Authentication middleware dan merchant context.~~ **Selesai pada v5.0** — lihat §3.4, §3.7. Belum ditest terhadap database sungguhan.
2. ~~API-key hashing/verification.~~ **Selesai pada v5.0.**
3. ~~Idempotency flow dengan Redis lock dan DB constraint.~~ **Sisi baca selesai pada v6.0** (duplicate/conflict detection, concurrency lock), **sisi tulis selesai pada v14.0** — lihat §3.4. `PaymentService::create_payment` sekarang insert `idempotency_keys` atomic bersama payment/attempt/audit, dengan `request_hash` mengalir dari middleware dan response body dari closure yang disediakan handler.
4. ~~Seed/tooling untuk membuat API key.~~ **Selesai pada v7.0** — `src/bin/gen_api_key.rs` + migration `20260917_002_seed_demo_api_keys.sql`. Belum dijalankan terhadap Postgres sungguhan (tidak ada Docker di environment ini).
5. ~~Request validation.~~ **Selesai** untuk `CreatePaymentRequest` (v9.0), `CancelPaymentRequest`, dan `SearchPaymentParams` (v11.0) — lihat §3.4.
6. ~~Payment application service.~~ **Selesai** — `PaymentService::create_payment`/`get_payment` (v7.0), `search_payments`/`cancel_payment` (v11.0), `retry_payment` (v12.0), `reconcile_payment` (v13.0) lengkap+tertest (lihat §3.4, §3.9).
7. ~~Provider selection dan invocation.~~ **Selesai pada v15.0** (lihat §3.5) — `select_best_provider` memilih provider ber-`priority()` terkecil di antara yang `is_available()`, priority datang dari config (`Settings::*_priority`), dipakai `create_payment` (v7.0) dan `retry_payment` (v12.0). Sejak v16.0, `is_available()` yang dikonsultasikan di sini genuinely circuit-breaker-aware lewat `CircuitBreakerProvider` (lihat P1 item #4 di bawah, sudah Selesai juga).
8. ~~Create dan Get payment handlers.~~ **Selesai pada v8.0** — `src/api/routes/payment.rs::create_payment`/`get_payment` tersambung ke `PaymentService` (lihat §3.4). Belum ditest terhadap Postgres/Redis sungguhan.
9. ~~Search dan Cancel payment handlers.~~ **Selesai pada v11.0** (lihat §3.4). Catatan: `cancel_payment` cuma mengubah status lokal, **tidak** memberi tahu provider (mis. void transaksi di gateway) — sesuai bentuk response yang didokumentasikan API contract §3.4 (tidak ada field provider), tapi worth diperiksa ulang kalau requirement sebenarnya butuh provider notification.
10. ~~Manual retry endpoint.~~ **Selesai pada v12.0** (lihat §3.4) — operations-only (`403` kalau bukan), dibatasi `MAX_RETRY_ATTEMPTS`, atomic.
11. ~~Atomic transaction untuk payment, attempt, dan audit log.~~ **Selesai pada v10.0**, diperluas v12.0 untuk retry (`update_status_with_attempt_and_audit`), v13.0 untuk reconcile (`reconcile`), dan v14.0 untuk idempotency record (lihat item #3) — `PgPaymentTransactionRepository` (lihat §3.3).
12. ~~Sambungkan `IdempotencyRepository::save()` ke transaksi `create_payment`.~~ **Selesai pada v14.0** (lihat §3.3, §3.4) — `PaymentTransactionRepository::create_with_attempt_and_audit` dapat parameter `idempotency: Option<&IdempotencyRow>`, request hash mengalir dari middleware lewat `IdempotencyKey`/`CreatePaymentInput`, response body dibangun via closure `response_snapshot` yang disuplai handler.
13. ~~Reconciliation endpoint.~~ **Selesai pada v13.0** (lihat §3.4) — operations-only (`403` kalau bukan), eligibility via `PaymentStatus::needs_reconciliation()`, atomic via `PaymentTransactionRepository::reconcile`. Belum reachable lewat flow lain mana pun (belum ada worker yang men-set status `PENDING_RECONCILIATION`) dan belum pakai distributed lock (`reconcile:{payment_id}`) — keduanya masuk P1 (lihat di bawah).

### Prioritas P1 — reliability dan fallback

1. Provider timeout handling dan error classification.
2. Retry orchestration/worker dengan bounded attempts.
3. Reconciliation service otomatis/worker — deteksi payment uncertain dan transisi ke `PENDING_RECONCILIATION` (endpoint manual-nya sudah selesai di v13.0, lihat §3.4/§3.6/§5 P0 item 13).
4. ~~Circuit breaker per provider.~~ **Selesai pada v16.0** (lihat §3.5) — `providers::circuit_breaker::CircuitBreakerProvider`, dekorator `PaymentProvider` dengan state CLOSED/OPEN/HALF_OPEN sesuai §6.3 architecture doc, membungkus keempat adapter di `build_providers`. In-memory per-proses saja (bukan lintas-instance); HALF_OPEN disederhanakan jadi "OPEN yang durasinya sudah lewat" alih-alih flag "1 probe" yang ditegakkan ketat di bawah concurrent caller — dicatat sebagai trade-off yang disengaja, lihat §1 v16.0 dan komentar di `circuit_breaker.rs`.
5. Safe automatic fallback dari Gateway A ke Gateway B.
6. Locking antara retry, webhook, reconciliation, dan failover (lock helper sudah siap dipakai, tinggal diintegrasikan — reconcile endpoint v13.0 belum pakai `reconcile:{payment_id}` lock).
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
| Reconcile endpoint belum reachable lewat flow lain mana pun | Endpoint `POST /payments/{id}/reconcile` sudah berfungsi, tapi tidak ada worker/trigger yang pernah men-set payment ke `PENDING_RECONCILIATION` — jadi secara praktis payment tidak akan pernah "butuh" direkonsiliasi sampai worker itu ada | Kerjakan P1 (reconciliation service otomatis/worker) |
| Reconcile endpoint belum pakai distributed lock | Kalau operations memanggil reconcile untuk payment yang sama dua kali bersamaan, tidak ada `reconcile:{payment_id}` lock (per architecture doc) yang mencegah race — `PaymentService` belum punya dependency Redis | Tambahkan Redis lock ke `reconcile_payment` kalau concurrent-call jadi masalah nyata (lihat P1 item 6) |
| `cancel_payment` tidak memberi tahu provider | Kalau provider masih memproses pembayaran di sisi mereka, status lokal jadi CANCELLED tapi provider tidak tahu — berpotensi payment tetap sukses di provider padahal sudah "dibatalkan" di sistem kita | Verifikasi requirement sebenarnya; kalau perlu, tambah `PaymentProvider::cancel_payment` dan panggil dari `PaymentService::cancel_payment` |
| Otorisasi operations-only masih inline per-handler | `MerchantContext::is_operations()` sekarang dicek terpisah di `retry_payment` dan `reconcile_payment` (v13.0) — bukan lewat middleware/extractor bersama, jadi endpoint operations-only baru berikutnya bisa lupa menambahkan cek ini | Pertimbangkan generic operations-only middleware/extractor kalau jumlah endpoint operations terus bertambah |
| Create/Get/Search/Cancel/Retry/Reconcile payment, authentication & idempotency middleware, migration seed belum ditest terhadap database sungguhan | Bug logic (mis. salah tangani expired/revoked key, race condition pada lock, migration SQL yang tidak sesuai skema, atau DTO mapping yang tidak cocok skema DB) berpotensi belum terdeteksi meski unit test pure-logic/fake-repo lulus | Jalankan `sqlx migrate run` + hit endpoint sungguhan begitu Postgres/Redis tersedia |
| Idempotency write-side belum ditest terhadap Postgres sungguhan | `INSERT INTO idempotency_keys` dalam transaksi atomic (v14.0) baru diverifikasi lewat fake trait di unit test — constraint PK `(idempotency_key, merchant_id)`, tipe kolom (`response_status_code VARCHAR(3)`, `response_body JSONB`), dan replay end-to-end belum pernah dijalankan lewat Postgres nyata | Jalankan integration test begitu Postgres tersedia — termasuk skenario replay duplicate request sungguhan |
| Idempotency key reuse setelah expired bisa bentrok PK | `insert_idempotency_row` plain `INSERT` (bukan `ON CONFLICT`) — kalau idempotency key yang sama dipakai lagi setelah 24 jam TTL tapi baris lama belum dibersihkan (tidak ada cleanup job), insert akan gagal dan seluruh transaksi `create_payment` rollback, padahal seharusnya boleh dipakai ulang | Edge case sempit, dicatat sebagai known limitation; tambahkan `ON CONFLICT DO UPDATE` atau cleanup job kalau observasi produksi menunjukkan ini benar-benar terjadi |
| Atomic transaction belum ditest terhadap Postgres sungguhan | `BEGIN`/`COMMIT`/`ROLLBACK` di `PgPaymentTransactionRepository` (create + retry + reconcile) baru diverifikasi lewat fake trait di unit test, belum lewat Postgres nyata | Jalankan integration test begitu Postgres tersedia |
| Circuit breaker state in-memory per-proses saja | Kalau `spo-api` dijalankan multi-instance (mis. beberapa pod di belakang load balancer), tiap instance punya state circuit breaker sendiri-sendiri — satu instance bisa menganggap provider OPEN (gagal terus) sementara instance lain masih CLOSED, jadi proteksi tidak konsisten lintas instance | Butuh state bersama (mis. Redis) kalau deployment multi-instance jadi kebutuhan nyata — di luar scope P1 saat ini |
| Circuit breaker HALF_OPEN tidak menegakkan ketat "1 probe" | `is_available()` menghitung `Instant::elapsed()` on-the-fly tanpa mutasi state, jadi beberapa request konkuren yang datang tepat setelah `open_duration` lewat semua bisa lolos sebagai probe, bukan cuma satu seperti spesifikasi §6.3 | Trade-off yang disengaja untuk kesederhanaan; tambahkan flag "probe in-flight" kalau observasi produksi menunjukkan over-probing jadi masalah nyata |
| README lama menandai beberapa fitur runtime sebagai selesai | Ekspektasi pengguna tidak sesuai kondisi kode | Gunakan report ini sebagai sumber status; sinkronkan README berikutnya |
| Automated test masih terbatas (84 unit test — lihat §8) | Regression dan correctness belum terukur untuk domain/API/webhook/middleware end-to-end | Tambahkan test bersamaan dengan setiap use case di P0-P2 |
| `cargo clippy` belum pernah dijalankan | Lint issue/anti-pattern berpotensi belum terdeteksi | Jalankan `cargo clippy` sebelum CI dibuat |
| Timeout tanpa reconciliation | Risiko duplicate transaction saat fallback | Larang fallback otomatis sampai reconciliation tersedia |
| Security layer masih skeleton | Endpoint belum aman diekspos | Jangan deploy ke production |

## 8. Hasil Verifikasi

| Pemeriksaan | Hasil |
| --- | --- |
| `cargo build` (lib + bin + `gen_api_key`) | **Lulus** — 0 error, hanya warning kosmetik (`unused variable`, `dead_code` pada fungsi yang memang belum dipakai) |
| `cargo test` | **Lulus** — 84/84 test passed (5 provider status-mapping + 9 `circuit_breaker` + 10 security termasuk `is_operations` + 3 idempotency middleware + 24 `application::payment` termasuk retry, reconcile, dan seleksi provider berbasis priority + 11 DTO mapping termasuk retry response + 9 error mapping handler termasuk MAX_RETRY_REACHED dan PAYMENT_NOT_RECONCILABLE + 8 request validation + 5 `domain::rules`); 0 failed |
| `cargo fmt -- --check` | **Lulus**, tidak ada isu format |
| `cargo clippy` | Belum dijalankan pada pass ini — masuk backlog P3 |
| `gen_api_key` tool | Dijalankan manual 2× untuk generate hash yang di-embed di migration seed; output diverifikasi cocok format `key_prefix`/Argon2 PHC yang diharapkan repository |
| Authentication middleware, idempotency middleware (baca + tulis), migration seed, create/get/search/cancel/retry/reconcile payment, atomic transaction end-to-end | **Belum diverifikasi** — tidak ada Postgres/Docker di environment ini. `PaymentService` (termasuk `PaymentTransactionRepository`), request validation, dan DTO/error-mapping tertest lewat fake repository/provider dan pure function (bukan DB/HTTP sungguhan) |
| Source scan untuk TODO/stub | Ditemukan pada webhook route, application services (audit/provider/reconciliation-otomatis/webhook), dan test file (`tests/api`, `tests/providers`) — reconcile payment route (§3.4) tidak lagi stub sejak v13.0 |
| Payment API runtime implementation | Seluruh 6 route payment (`POST /payments`, `GET /payments/{id}`, `GET /payments` search, `POST /payments/{id}/cancel`, `POST /payments/{id}/retry`, `POST /payments/{id}/reconcile`) tersambung ke `PaymentService` (§3.4) — tidak ada lagi handler `NOT_IMPLEMENTED` |
| Circuit breaker (`CircuitBreakerProvider`) | 9 unit test lulus dengan fake inner provider dan real (bukan mocked) wall-clock time via `tokio::time::sleep` untuk skenario HALF_OPEN — belum pernah diuji dengan provider adapter sungguhan (Midtrans/Xendit/DOKU/NICEPAY) yang benar-benar timeout/gagal berulang di runtime |
| Automated test implementation | 84 test: provider (5) + circuit breaker (9) + security (10) + idempotency middleware (3) + payment application service (24) + DTO response mapping (11) + handler error-code mapping (9) + request validation (8) + domain rules (5); domain unit test masih parsial (cuma `rules.rs`), API integration/webhook/concurrency test belum ada |
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
| 12.0 | 18 September 2026 | Implementasi P0 manual retry. `MerchantContext::is_operations()` baru (`security/api_key.rs`) — implementasi pertama authorization operations-only, dicek di handler `retry_payment` (`403 AUTHORIZATION_FAILED` kalau bukan). `PaymentRepository::get_by_id_unscoped` baru — lookup lintas-merchant untuk flow operations (`get_by_id` yang lama tetap merchant-scoped, dipakai get/cancel). `PaymentService::retry_payment` pakai `PaymentStatus::is_retryable()` untuk eligibility (BUKAN `validate_transition`, yang sejak v11.0 menganggap FAILED sebagai final dan akan selalu menolak) — set status ke Processing secara manual, dibatasi `MAX_RETRY_ATTEMPTS` (sekarang benar-benar ditegakkan, sebelumnya konstanta tak terpakai) via `AttemptRepository::count_attempts`, `400 MAX_RETRY_REACHED` kalau melebihi. `PaymentTransactionRepository` dapat method baru `update_status_with_attempt_and_audit` (atomic: update status + insert attempt + insert audit); `PgPaymentRepository::update_status` diperluas dengan parameter `provider` (perlu untuk retry yang bisa pilih provider berbeda; kolom itu sebelumnya tidak pernah di-update lewat jalur ini) — `cancel_payment`'s call site diupdate untuk parameter baru ini (passing `None`, tidak ada perubahan perilaku). DTO baru: `RetryPaymentResponse`/`RetryPaymentData` (`{payment_id, status, attempt_number, provider, message}` sesuai kontrak). Provider error saat retry tidak dipersist (retry gagal tidak menambah hitungan `MAX_RETRY_ATTEMPTS`) — sama seperti `create_payment`. Seluruh route payment kini tersambung ke `PaymentService` kecuali reconcile. 9 unit test baru (66/66 total lulus). Belum ditest terhadap Postgres sungguhan |
| 13.0 | 18 September 2026 | Implementasi P0 manual reconciliation — **route payment terakhir yang tersambung ke `PaymentService`**. `ReconciliationRecordRow` baru (`domain/repositories.rs`, mapping tabel `reconciliation_records`) dan method baru `PaymentTransactionRepository::reconcile` (atomic: selalu insert record, dan hanya kalau status provider berhasil di-resolve ikut update `payments.status`), diimplementasikan via helper executor-generic baru `insert_reconciliation_record` (`infrastructure/postgres/repositories.rs`, pola sama seperti `insert_payment`/`insert_attempt`/`insert_audit_log`). `ApplicationError::NotReconcilable(PaymentStatus)` baru → `409 PAYMENT_NOT_RECONCILABLE`. `PaymentService::reconcile_payment` pakai `PaymentStatus::needs_reconciliation()` untuk eligibility (bukan `validate_transition`, pola sama seperti retry v12.0) — ambil attempt terakhir, query `PaymentProvider::get_payment_status`, petakan `COMPLETED`/`FAILED`/lainnya ke `Success`/`Failed`/tetap `PENDING_RECONCILIATION` dengan resolution `UNCERTAIN`. Berbeda dari create/retry: error provider saat reconcile TIDAK di-propagate, direkam sebagai `UNCERTAIN` dengan detail error — tujuannya mencatat upaya reconciliation, bukan kehilangan jejaknya. Handler `reconcile_payment` (`api/routes/payment.rs`) cek `MerchantContext::is_operations()` sama seperti retry. DTO: `impl From<ReconciliationOutcome> for ReconcileResponse` baru (struct `ReconcileResponse`/`ReconcileData` sudah ada sebelumnya). Sengaja TIDAK diimplementasikan: insert `payment_attempts` baru untuk query reconciliation, insert `audit_logs` (record itu sendiri jadi audit trail-nya), dan Redis distributed lock `reconcile:{payment_id}` dari architecture doc (`PaymentService` belum punya dependency Redis). Endpoint sudah benar tapi **belum reachable lewat flow lain mana pun** — tidak ada worker yang mentransisikan payment ke `PENDING_RECONCILIATION` (masuk P1). Sekalian memperbaiki baris "Operational payment actions" (§3.8) yang sejak v11.0/v12.0 masih salah menyatakan "Belum" meski cancel dan retry sudah bekerja. 7 unit test baru (73/73 total lulus). Belum ditest terhadap Postgres sungguhan |
| 14.0 | 18 September 2026 | Implementasi P0 idempotency write-side — menutup gap yang tercatat sejak v6.0 (§5 item #3/#12). `PaymentTransactionRepository::create_with_attempt_and_audit` dapat parameter baru `idempotency: Option<&IdempotencyRow>`; kalau `Some`, `idempotency_keys` ikut di-insert dalam transaksi atomic yang sama dengan payment/attempt/audit. Helper baru `insert_idempotency_row` (executor-generic, `infrastructure/postgres/repositories.rs`) diekstrak dari `PgIdempotencyRepository::save` yang sudah ada, dipakai bersama oleh path non-transactional dan path atomic baru — pola sama seperti `insert_payment`/`insert_attempt`/`insert_audit_log`/`insert_reconciliation_record`. `IdempotencyKey` (request extension, `api/middleware/idempotency.rs`) berubah dari tuple struct 1-field jadi struct `{key, request_hash}` — middleware sudah menghitung SHA-256 hash untuk deteksi duplikat, sekarang diteruskan ke handler alih-alih dibuang. `CreatePaymentInput` dapat field baru `request_hash`. `PaymentService::create_payment` dapat parameter baru `response_snapshot: impl FnOnce(&Payment) -> serde_json::Value` — closure yang disuplai handler HTTP untuk membangun JSON body yang di-cache untuk replay (`idempotency_keys.response_body`), dipanggil dari `Payment` yang sudah lengkap tapi belum di-insert; desain ini (bukan `application` membangun `api::dto::payment::PaymentResponse` sendiri) menjaga layering — `application` tidak boleh depend ke `api`. `response_status_code` di-hardcode `"201"` (satu-satunya status yang `create_payment` kembalikan). `expires_at` di-set eksplisit 24 jam dari Rust, bukan mengandalkan DB `DEFAULT`. Sengaja TIDAK diimplementasikan: `ON CONFLICT` handling untuk idempotency key yang dipakai ulang setelah expired (PK `(idempotency_key, merchant_id)` berpotensi bentrok di edge case sangat sempit — dicatat di §7, bukan diperbaiki). Tidak ada test baru ditambahkan — 1 test yang sudah ada (`create_payment_persists_payment_attempt_and_audit_atomically`) diperluas dengan assertion untuk baris `idempotency_keys` yang di-generate (73/73 total tetap lulus). Belum ditest terhadap Postgres sungguhan |
| 15.0 | 18 September 2026 | Implementasi P0 terakhir yang masih open: provider selection berbasis prioritas — **dengan ini, seluruh 13 item P0 (§5) Selesai**. `PaymentProvider` trait (`providers/adapter.rs`) dapat method wajib baru `priority() -> i32`, diimplementasikan di keempat adapter (Alpha/Midtrans, Beta/Xendit, Gamma/DOKU, Nicepay) — masing-masing struct dapat field `priority: i32` baru, dialirkan lewat parameter `::new()` tambahan. `Settings` (`config/settings.rs`) dapat 4 field baru (`midtrans_priority`/`xendit_priority`/`doku_priority`/`nicepay_priority`, env var `*_PRIORITY`) dengan default 10/20/30/40 yang menjaga urutan registrasi lama supaya perilaku default tidak berubah. `providers::build_providers` meneruskan nilai-nilai ini ke tiap adapter. Fungsi baru `select_best_provider` (`application/payment.rs`) — di antara provider `is_available()`, pilih `priority()` terkecil (`filter().min_by_key()`) — menggantikan `.find(|p| p.is_available())` yang sebelumnya identik dipakai `create_payment` dan `retry_payment` (implisit berarti "provider pertama di `Vec`"; kini eksplisit dan bisa dikonfigurasi tanpa ubah kode). Sengaja TIDAK termasuk: circuit breaker awareness — `is_available()` masih cuma cek config, bukan runtime health, jadi provider yang sedang gagal berulang tapi terkonfigurasi tetap bisa terpilih; tetap P1 item #4, item terpisah. §3.5 baris "Provider selection by availability/priority" naik dari Parsial ke Selesai; baris "Circuit breaker" tetap Belum (item berbeda). 2 unit test baru: provider priority lebih kecil menang meski didaftarkan belakangan (bukan first-in-Vec), dan provider unavailable di-skip meski priority-nya lebih baik (75/75 total lulus). Belum ditest terhadap environment sungguhan |
| 16.0 | 18 September 2026 | Implementasi P1 pertama: circuit breaker per provider (§5 P1 item 4), menutup config yang sejak awal proyek ada (`circuit_breaker_threshold`, `circuit_breaker_timeout_seconds`) tapi tidak pernah dipakai runtime mana pun. `CircuitBreakerProvider` baru (`providers/circuit_breaker.rs`) — dekorator generik yang membungkus `Box<dyn PaymentProvider>` mana pun (tidak menyentuh 4 adapter yang ada), state machine CLOSED (normal, hitung kegagalan berturut-turut) / OPEN (>= `failure_threshold` kegagalan → langsung unavailable) / HALF_OPEN (setelah `open_duration` lewat, izinkan probe — sukses reset ke CLOSED, gagal kembali ke OPEN), sesuai §6.3 architecture doc. Direpresentasikan sebagai 2 variant internal (`Closed`/`Open`), bukan 3 — HALF_OPEN dihitung on-the-fly dari `Instant::elapsed()` di `is_available()` tanpa mutasi state, simplifikasi yang disengaja (spesifikasi "izinkan tepat 1 probe" butuh flag "probe in-flight" tambahan yang tidak diimplementasikan; beberapa request konkuren yang datang tepat setelah durasi lewat semua bisa lolos sebagai probe). State in-memory per-proses (bukan Redis/DB) — deployment multi-instance butuh koordinasi state bersama, di luar scope. `providers::build_providers` membungkus keempat adapter dengan ini via closure `with_circuit_breaker`, sehingga `PaymentService::select_best_provider` (v15.0) otomatis circuit-breaker-aware tanpa perubahan apa pun di `application/payment.rs` — dekorator transparan di belakang trait `PaymentProvider` yang sama. §3.5 baris "Circuit breaker" naik dari Belum ke Selesai. 9 unit test baru untuk `CircuitBreakerProvider`, sebagian pakai `tokio::time::sleep` dengan durasi pendek (bukan mocked clock — fitur `test-util` tokio tidak diaktifkan) untuk menguji transisi HALF_OPEN (84/84 total lulus). Belum diuji terhadap provider adapter sungguhan yang benar-benar timeout/gagal berulang |
