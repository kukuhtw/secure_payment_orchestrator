# Task Progress Report

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi report | 6.0 |
| Tanggal audit | 17 September 2026 |
| Status produk | Proof of Concept, belum production-ready |
| Dasar penilaian | `cargo build`, `cargo test`, `cargo fmt -- --check` (registry Cargo dapat diakses), ditambah pemeriksaan source code, migration, konfigurasi, dan dokumentasi |
| Verifikasi build | **Lulus.** `cargo build` sukses (lib + bin), `cargo test` sukses (16/16 test lulus), `cargo fmt -- --check` lulus tanpa isu |

## 1. Ringkasan Eksekutif

Fondasi proyek sudah tersedia: struktur aplikasi Rust, domain model, state machine,
PostgreSQL repositories, Redis lock helper, kontrak provider, empat provider adapter
(Midtrans/Alpha, Xendit/Beta, DOKU/Gamma, NICEPAY), konfigurasi, dokumentasi API,
container setup, authentication middleware, dan — baru pada pass ini — **idempotency
middleware yang berfungsi**.

Pass v6.0 mengimplementasikan P0 item kedua: idempotency middleware. Untuk request
`POST` pada route payment: header `Idempotency-Key` wajib ada (400 jika tidak),
body di-hash (SHA-256, cocok kolom `idempotency_keys.request_hash VARCHAR(64)`),
lalu dicek ke `IdempotencyRepository`. Kalau key yang sama sudah pernah dipakai dengan
payload identik, response yang tersimpan di-replay langsung (handler tidak dipanggil
ulang). Kalau key sama tapi payload beda → `409 IDEMPOTENCY_MISMATCH` (kode error persis
sesuai `documentation/api/API-Contract-Secure-Payment-Orchestrator.md` §3.1). Kalau key
belum pernah dipakai, middleware mencoba **acquire Redis lock** (`SET NX EX`, pakai
`infrastructure::redis::lock` yang sudah ada) sebelum meneruskan ke handler — mencegah
dua request konkuren dengan key yang sama diproses bersamaan (`409 IDEMPOTENCY_IN_PROGRESS`
jika lock gagal didapat). Middleware ini no-op untuk method selain `POST`, dan dipasang
di router **setelah** authentication middleware (dalam urutan eksekusi request) karena ia
butuh `MerchantContext` yang di-attach oleh auth.

**Batas scope yang disengaja:** middleware ini hanya menangani sisi *baca* (deteksi
duplikat + conflict + lock konkurensi). Sisi *tulis* — menyimpan `IdempotencyRow` baru
(butuh `payment_id`, yang baru ada setelah payment berhasil dibuat) — sengaja **belum**
diimplementasikan di sini, karena itu harus atomic dengan insert payment + audit log,
persis item P0 terpisah "Atomic transaction" yang masih terbuka. Mengerjakannya sekarang
berarti menebak bentuk response handler yang belum ada (`create_payment` masih
`NOT_IMPLEMENTED`) — jadi backlog itu tetap terbuka, bukan regresi.

**Keterbatasan verifikasi** (sama seperti authentication middleware di v5.0): tidak ada
Docker/Postgres di environment ini, jadi alur penuh belum ditest end-to-end. Yang
terverifikasi lewat `cargo test` adalah logic murni yang tidak butuh DB: hashing
request body deterministik, dan rekonstruksi cached response dari `IdempotencyRow`
(3 test baru, total 16/16 lulus).

| Status | Jumlah task | Persentase |
| --- | ---: | ---: |
| Selesai | 24 | 40% |
| Parsial | 14 | 23% |
| Belum | 22 | 37% |
| **Total** | **60** | **100%** |

Dibanding v5.0 (23 selesai / 14 parsial / 23 belum), satu task naik status: Idempotency
middleware dari Belum ke Selesai. Tidak ada task yang turun status pada pass ini.

Persentase di atas adalah hitungan task pada report ini, bukan estimasi LOC atau klaim
kesiapan production. Core payment flow **masih** belum dapat digunakan: handler API
masih mengembalikan `NOT_IMPLEMENTED` — kedua middleware (auth + idempotency) sudah siap
melindunginya begitu handler tersebut diimplementasikan.

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

### 3.3 Domain dan persistence — 5 selesai, 2 parsial

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Payment aggregate dan Money | Selesai | `src/domain/payment.rs` — entity, validasi, transition wrapper |
| Payment status/state machine | Selesai | `src/domain/status.rs`, `src/domain/rules.rs` — enum dan transition matrix lengkap |
| Retry classification/backoff rules | Selesai | `is_retryable_http_status`, `is_retryable_error`, `retry_delay_seconds` di `src/domain/rules.rs` |
| Repository contracts | Selesai | 6 trait (payment, attempt, API key, idempotency, audit, webhook) di `src/domain/repositories.rs` |
| PostgreSQL repository implementation | Selesai | Diperbaiki: `PaymentAttemptRow` ditambahkan ke import di `repositories.rs`, dan `domain/attempt.rs` memakai `row.status.as_str()` / `row.attempt_type.as_str()` (bukan `&row.status`) agar cocok dengan `impl From<&str>`. Query CRUD/search/count untuk semua 6 repository lengkap dan compile bersih |
| Atomic business transaction | Parsial (tidak diubah pada pass ini) | Tidak ditemukan pemakaian `sqlx::Transaction`/`.begin()` di source manapun; create payment, audit log, dan idempotency record masih 3 query terpisah tanpa pembungkus transaksi — ini backlog fitur (P0), bukan compile blocker |
| Concurrency protection | Parsial | Redis lock helper (§3.6) kini dipakai nyata oleh idempotency middleware (§3.4) untuk mencegah request konkuren dengan `Idempotency-Key` sama diproses bersamaan; belum dipakai oleh webhook/reconciliation flow — `application/*.rs` masih skeleton satu baris |

### 3.4 Core Payment API — 2 selesai, 1 parsial, 5 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Request/response DTO | Parsial | Struct lengkap di `src/api/dto/payment.rs`; validation belum diterapkan pada handler |
| Authentication middleware | Selesai | `src/api/middleware/authentication.rs::require_api_key` — validasi `Authorization: Bearer`, lookup `key_prefix`, verifikasi Argon2 constant-time (`security::hash::verify_secret`), cek merchant `ACTIVE`, attach `MerchantContext` ke request extensions. Dipasang hanya pada payment routes via `axum::middleware::from_fn_with_state` di `api/mod.rs` (bukan global — health/ready/metrics/webhook tidak terpengaruh). **Belum ditest end-to-end** (tidak ada Postgres/Docker di environment ini, migration tidak seed `api_keys`); hanya unit test pure-logic yang lulus (lihat §3.7, §3.9) |
| Idempotency middleware | Selesai | `src/api/middleware/idempotency.rs::require_idempotency_key` — wajibkan header pada POST, hash body (SHA-256), cek duplikat/conflict ke `IdempotencyRepository` (replay cached response atau `409 IDEMPOTENCY_MISMATCH`), acquire Redis lock untuk cegah request konkuren (`409 IDEMPOTENCY_IN_PROGRESS`). Dipasang setelah auth middleware (butuh `MerchantContext`). Sisi tulis (`save` ke `idempotency_keys`) sengaja belum ada — menunggu atomic transaction bersama payment insert (lihat §5 P0). **Belum ditest end-to-end** (alasan sama seperti authentication middleware) |
| Create payment | Belum | Handler mengembalikan `NOT_IMPLEMENTED` (`src/api/routes/payment.rs`) |
| Get payment | Belum | Handler mengembalikan `NOT_IMPLEMENTED` |
| Search payment | Belum | Handler mengembalikan `NOT_IMPLEMENTED` |
| Cancel payment | Belum | Handler mengembalikan `NOT_IMPLEMENTED` |
| Manual retry endpoint | Belum | Handler mengembalikan `NOT_IMPLEMENTED` |

### 3.5 Provider integration dan fallback — 2 selesai, 3 parsial, 4 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| Canonical `PaymentProvider` contract | Selesai | `src/providers/adapter.rs` — trait, request, response, error types |
| Midtrans/Xendit/DOKU provider adapters | Selesai | Alpha (277 baris), Beta (232 baris), Gamma (346 baris) — Midtrans, Xendit, DOKU Sandbox |
| NICEPAY example adapter | Parsial | Registration/create tersedia (269 baris); inquiry memerlukan referenceNo dan amt yang belum dibawa kontrak status provider |
| Provider availability contract | Parsial | `is_available()` di trait; Midtrans unavailable jika Server Key kosong; health/circuit breaker runtime belum tersedia |
| Failover data model | Parsial | `AttemptType::Failover` tersedia; flow belum diimplementasikan |
| Provider selection by availability/priority | Belum | `src/application/provider.rs` masih skeleton satu baris |
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

### 3.9 Quality assurance dan delivery — 2 selesai, 1 parsial, 5 belum

| Task | Status | Bukti / catatan |
| --- | --- | --- |
| API contract | Selesai | `documentation/api/API-Contract-Secure-Payment-Orchestrator.md` |
| OpenAPI specification | Selesai | `documentation/api/openapi.yaml` |
| Domain unit tests | Belum | Tidak ditemukan `#[cfg(test)]`/`#[test]` di `src/domain/*.rs` |
| API integration tests | Belum | `tests/api/mod.rs` masih TODO |
| Provider tests | Parsial | 5 unit test di provider layer (Midtrans/Xendit/DOKU/NICEPAY status mapping) — semuanya **lulus** via `cargo test`; `tests/providers/mod.rs` (integration) masih TODO. Di luar provider, ada 11 unit test lain (8 security, 3 idempotency middleware) — total 16/16 lulus |
| CI/CD workflow | Belum | Folder `.github/` tidak ditemukan di repository |
| Postman collection | Belum | File tidak tersedia |
| Demo/end-to-end script | Belum | File tidak tersedia |

## 4. Daftar yang Sudah Selesai

1. BRD, PRD, WBS, architecture, dan ERD.
2. Configuration loader (seluruh provider + retry/circuit-breaker settings).
3. Migration schema, indexes, dan seed dasar.
4. Payment aggregate, Money, payment status, dan transition rules.
5. Retry classification dan exponential-backoff calculation.
6. Repository traits (6 kontrak) dan seluruh implementasi PostgreSQL-nya.
7. Provider adapter contract serta adapter create/status untuk Midtrans, Xendit, dan DOKU.
8. Contoh registration/create payment NICEPAY; inquiry masih parsial.
9. Dockerfile dan Docker Compose.
10. API contract dan OpenAPI specification.
11. PostgreSQL pool creation dan Redis client setup (path/naming bug diperbaiki).
12. Redis distributed lock helper (compile bug diperbaiki).
13. Structured JSON logging initialization.
14. `spo-api` compile bersih: `cargo build`, `cargo test` (16/16 lulus), dan `cargo fmt -- --check` semuanya lulus.
15. Authentication middleware — Argon2 API-key verification, merchant context, dipasang hanya pada payment routes (belum ditest end-to-end, lihat §3.4 dan §7).
16. Idempotency middleware (sisi baca) — duplicate/conflict detection dan Redis lock concurrency guard, dipasang setelah auth (belum ditest end-to-end; sisi tulis menunggu atomic transaction, lihat §3.4 dan §7).

## 5. Daftar yang Belum Selesai

Seluruh compile blocker dari v3.0 (12 error + 3 temuan static-review) sudah diperbaiki —
lihat §9 Changelog untuk daftar lengkap dan §3 untuk detail per task. Backlog di bawah ini
murni tentang fitur/business logic yang belum dikerjakan, bukan lagi tentang kode yang
tidak bisa dikompilasi.

### Prioritas P0 — agar core payment flow dapat berjalan

1. ~~Authentication middleware dan merchant context.~~ **Selesai pada v5.0** — lihat §3.4, §3.7. Belum ditest terhadap database sungguhan.
2. ~~API-key hashing/verification.~~ **Selesai pada v5.0.**
3. ~~Idempotency flow dengan Redis lock dan DB constraint.~~ **Sisi baca selesai pada v6.0** (duplicate/conflict detection, concurrency lock) — lihat §3.4. Sisi tulis (persist `IdempotencyRow`) masih menunggu item #7 di bawah.
4. Seed/tooling untuk membuat API key (migration hanya seed `merchants`, tidak ada `api_keys` — perlu untuk testing end-to-end).
5. Request validation.
6. Payment application service.
7. Provider selection dan invocation.
8. Create, get, search, dan cancel payment handlers (perlu mengonsumsi `MerchantContext` dari request extensions untuk scoping per-merchant).
9. Atomic transaction untuk payment, idempotency record, attempt, dan audit log — begitu ini selesai, idempotency middleware perlu ditambah langkah `save()` setelah `next.run()` sukses.

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
| Core API handlers masih `NOT_IMPLEMENTED` | Aplikasi belum dapat memproses payment meskipun source sudah compile dan kedua middleware (auth + idempotency) sudah siap | Selesaikan P0 di §5 secara berurutan |
| Authentication & idempotency middleware belum ditest terhadap database sungguhan | Bug logic (mis. salah tangani expired/revoked key, atau race condition pada lock) berpotensi belum terdeteksi meski unit test pure-logic lulus | Jalankan integration test begitu Postgres/Redis tersedia; migration perlu seed/tooling API key (lihat P0 §5) |
| Idempotency middleware belum bisa menyimpan response (sisi tulis) | Setelah handler create-payment nanti diimplementasikan, duplicate request kedua **tidak akan** di-replay dari cache sampai langkah `save()` ditambahkan bersama atomic transaction | Ingat untuk sambungkan `IdempotencyRepository::save()` saat mengerjakan item P0 "Atomic transaction" |
| README lama menandai beberapa fitur runtime sebagai selesai | Ekspektasi pengguna tidak sesuai kondisi kode | Gunakan report ini sebagai sumber status; sinkronkan README berikutnya |
| Automated test masih terbatas (16 unit test: 5 provider + 8 security + 3 idempotency) | Regression dan correctness belum terukur untuk domain/API/webhook/middleware end-to-end | Tambahkan test bersamaan dengan setiap use case di P0-P2 |
| `cargo clippy` belum pernah dijalankan | Lint issue/anti-pattern berpotensi belum terdeteksi | Jalankan `cargo clippy` sebelum CI dibuat |
| Timeout tanpa reconciliation | Risiko duplicate transaction saat fallback | Larang fallback otomatis sampai reconciliation tersedia |
| Security layer masih skeleton | Endpoint belum aman diekspos | Jangan deploy ke production |

## 8. Hasil Verifikasi

| Pemeriksaan | Hasil |
| --- | --- |
| `cargo build` (lib + bin) | **Lulus** — 0 error, hanya warning kosmetik (`unused variable`, `dead_code` pada fungsi yang memang belum dipakai) |
| `cargo test` | **Lulus** — 16/16 test passed (5 provider status-mapping + 8 security + 3 idempotency middleware: hashing, cached-response reconstruction); 0 failed |
| `cargo fmt -- --check` | **Lulus**, tidak ada isu format |
| `cargo clippy` | Belum dijalankan pada pass ini — masuk backlog P3 |
| Authentication & idempotency middleware end-to-end | **Belum diverifikasi** — tidak ada Postgres/Docker di environment ini, dan migration tidak seed `api_keys`. Hanya pure-logic unit test yang lulus |
| Source scan untuk TODO/stub | Ditemukan pada payment routes, webhook route, application services, dan test file (`tests/api`, `tests/providers`) |
| Payment API runtime implementation | Belum tersedia |
| Automated test implementation | Terbatas pada unit test provider + security + idempotency (16 test); domain/API/webhook/concurrency/integration test belum ada |
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
