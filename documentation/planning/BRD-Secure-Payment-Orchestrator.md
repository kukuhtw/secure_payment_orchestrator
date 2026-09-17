# Business Requirements Document

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi | 1.0 |
| Status | Draft untuk Proof of Concept |
| Tanggal | 13 September 2026 |
| Pemilik Produk | Product Owner |
| Target Implementasi | Portfolio dan technical demonstration |
| Teknologi Utama | Rust, Axum, Tokio, PostgreSQL, Redis |

## 1. Ringkasan Eksekutif

Secure Payment Orchestrator adalah layanan backend yang menyediakan satu antarmuka pembayaran untuk menghubungkan merchant dengan beberapa payment provider. Sistem menangani pemilihan provider, pencegahan transaksi ganda, retry yang aman, failover terbatas, webhook terverifikasi, rekonsiliasi, dan audit trail.

Proof of Concept menggunakan provider simulasi dan tidak memproses uang sungguhan. Tujuan utamanya adalah membuktikan rancangan layanan pembayaran yang benar, dapat diuji, aman, mudah diaudit, dan tahan terhadap kegagalan jaringan maupun provider.

## 2. Latar Belakang Bisnis

Integrasi pembayaran langsung dari aplikasi merchant ke beberapa provider menimbulkan sejumlah masalah:

1. Setiap provider memiliki kontrak API, autentikasi, status, dan format error yang berbeda.
2. Gangguan jaringan dapat menyebabkan status transaksi tidak diketahui.
3. Pengiriman request berulang dapat menghasilkan pembayaran ganda.
4. Webhook palsu atau webhook berulang dapat mengubah status secara tidak sah.
5. Tim operasional sulit menelusuri keputusan sistem tanpa audit trail terpusat.
6. Penambahan provider baru sering memerlukan perubahan besar pada aplikasi merchant.

Orchestrator dibutuhkan sebagai lapisan abstraksi yang menormalkan perbedaan provider dan menerapkan kontrol reliability serta security secara konsisten.

## 3. Tujuan Bisnis

| ID | Tujuan |
| --- | --- |
| BG-01 | Menyediakan satu API pembayaran yang konsisten untuk merchant. |
| BG-02 | Mencegah duplikasi pembayaran akibat retry dari merchant atau worker. |
| BG-03 | Mengurangi dampak gangguan provider melalui timeout, retry, circuit breaker, dan rekonsiliasi. |
| BG-04 | Memastikan webhook hanya diproses setelah signature dan keunikannya diverifikasi. |
| BG-05 | Menyediakan jejak audit untuk setiap perubahan status dan percobaan ke provider. |
| BG-06 | Memudahkan penambahan provider melalui pola provider adapter. |
| BG-07 | Menunjukkan praktik pengembangan Rust untuk sistem production-like yang security-sensitive. |

## 4. Indikator Keberhasilan

| ID | Indikator | Target POC |
| --- | --- | --- |
| KPI-01 | Request dengan idempotency key dan payload sama | Tidak membuat pembayaran baru |
| KPI-02 | Request dengan key sama tetapi payload berbeda | Ditolak dengan HTTP 409 |
| KPI-03 | Webhook dengan signature tidak valid | 100% ditolak |
| KPI-04 | Webhook yang sama dikirim berulang | Diproses tepat satu kali |
| KPI-05 | Perubahan status transaksi | 100% tercatat pada audit log |
| KPI-06 | Provider timeout sementara | Ditangani sesuai retry policy |
| KPI-07 | Test otomatis untuk domain kritis | Minimal 80% coverage pada domain layer |
| KPI-08 | Instalasi lingkungan demonstrasi | Dapat dijalankan dengan satu perintah Docker Compose |

## 5. Ruang Lingkup

### 5.1 Termasuk dalam POC

1. API key authentication untuk merchant.
2. Pembuatan dan pencarian pembayaran.
3. Idempotency key serta request hash.
4. Tiga provider simulasi, Alpha, Beta, dan Gamma.
5. Provider selection berdasarkan availability dan priority.
6. Timeout, retry dengan exponential backoff, dan circuit breaker sederhana.
7. Penyimpanan payment attempt.
8. Webhook dengan HMAC SHA-256, timestamp tolerance, dan deduplication.
9. Rekonsiliasi status ke provider.
10. Pembatalan sebelum pembayaran berhasil.
11. Audit log.
12. Health check, readiness check, structured logging, dan metrics.
13. Dokumentasi OpenAPI dan Postman collection.
14. Unit test, integration test, serta failure scenario test.

### 5.2 Tidak Termasuk dalam POC

1. Transaksi dengan uang sungguhan.
2. Sertifikasi PCI DSS.
3. Penyimpanan data kartu, PIN, CVV, atau kredensial perbankan.
4. Settlement dan laporan keuangan resmi.
5. Chargeback dan dispute management.
6. Refund parsial atau kompleks.
7. Dashboard operator production-grade.
8. Multi-region active-active deployment.
9. Integrasi KYC, AML, fraud engine, atau core banking nyata.
10. Jaminan legal dan regulatory compliance untuk penggunaan produksi.

## 6. Pemangku Kepentingan

| Peran | Kepentingan dan Tanggung Jawab |
| --- | --- |
| Product Owner | Menentukan prioritas, scope, dan acceptance akhir. |
| Merchant Developer | Mengintegrasikan API dan menerima notifikasi status. |
| Customer | Menyelesaikan pembayaran melalui halaman provider simulasi. |
| Operations | Memantau transaksi dan menjalankan rekonsiliasi. |
| Security Reviewer | Memeriksa autentikasi, signature, logging, dan threat model. |
| Rust Engineer | Merancang, membangun, menguji, dan mendokumentasikan sistem. |
| QA Engineer | Menjalankan functional, integration, concurrency, dan failure test. |

## 7. Proses Bisnis Target

1. Merchant mengirim permintaan pembayaran dengan API key dan idempotency key.
2. Sistem memvalidasi identitas merchant, payload, currency, dan nominal.
3. Sistem mengembalikan transaksi sebelumnya jika request identik pernah diproses.
4. Sistem membuat transaksi `PENDING` dan mencatat audit event.
5. Orchestrator memilih provider yang tersedia.
6. Provider adapter mengirim permintaan menggunakan format provider.
7. Hasil percobaan disimpan sebagai payment attempt.
8. Sistem mengubah status menjadi `PROCESSING`, `FAILED`, atau status final yang sesuai.
9. Provider mengirim webhook yang ditandatangani.
10. Sistem memverifikasi signature, timestamp, dan event ID.
11. Sistem memperbarui status hanya jika transisi diperbolehkan.
12. Merchant memeriksa status melalui API atau menerima notifikasi.
13. Operations menjalankan rekonsiliasi jika status tetap tidak diketahui.

## 8. Kebutuhan Bisnis

| ID | Kebutuhan | Prioritas |
| --- | --- | --- |
| BR-01 | Merchant harus dapat membuat pembayaran melalui satu API. | Must |
| BR-02 | Sistem harus menjamin satu idempotency key merepresentasikan satu request logis. | Must |
| BR-03 | Sistem harus menormalkan status dan error dari provider. | Must |
| BR-04 | Sistem harus menyimpan setiap percobaan komunikasi dengan provider. | Must |
| BR-05 | Sistem harus memverifikasi authenticity dan freshness webhook. | Must |
| BR-06 | Sistem harus mencegah webhook yang sama diproses lebih dari sekali. | Must |
| BR-07 | Sistem harus mencegah transisi status yang tidak valid. | Must |
| BR-08 | Sistem harus dapat memeriksa kembali transaksi berstatus tidak pasti. | Must |
| BR-09 | Operations harus dapat menelusuri riwayat transaksi. | Must |
| BR-10 | Sistem harus menghindari pencatatan secret dan data sensitif pada log. | Must |
| BR-11 | Provider baru dapat ditambahkan tanpa mengubah domain pembayaran. | Should |
| BR-12 | Sistem menyediakan metrics untuk keberhasilan, kegagalan, timeout, dan retry. | Should |
| BR-13 | Merchant dapat membatalkan transaksi yang belum mencapai status final. | Should |
| BR-14 | Sistem dapat menggunakan provider alternatif ketika kegagalan telah dipastikan aman untuk failover. | Could |

## 9. Aturan Bisnis

| ID | Aturan |
| --- | --- |
| RULE-01 | Nominal disimpan dalam integer unit terkecil untuk menghindari floating-point error. |
| RULE-02 | Currency menggunakan kode ISO 4217 yang didukung sistem. |
| RULE-03 | Merchant reference wajib unik dalam konteks merchant sesuai kebijakan konfigurasi. |
| RULE-04 | Idempotency key yang sama dan payload yang sama mengembalikan transaksi sebelumnya. |
| RULE-05 | Idempotency key yang sama dan payload berbeda ditolak. |
| RULE-06 | Transaksi `SUCCESS`, `FAILED`, `CANCELLED`, atau `REFUNDED` diperlakukan sebagai status final sesuai transition matrix. |
| RULE-07 | `SUCCESS` tidak dapat kembali ke `PROCESSING` atau `PENDING`. |
| RULE-08 | Retry hanya berlaku untuk error sementara, seperti timeout, HTTP 429, 502, atau 503. |
| RULE-09 | Validation error, authentication error, dan provider rejection tidak boleh di-retry otomatis. |
| RULE-10 | Timeout setelah request terkirim menghasilkan status tidak pasti dan memicu rekonsiliasi sebelum failover. |
| RULE-11 | Webhook tanpa signature valid harus ditolak sebelum mengubah data bisnis. |
| RULE-12 | Semua perubahan status menyimpan actor, waktu, status sebelum, status sesudah, dan metadata. |

## 10. Kebutuhan Nonfungsional

| Area | Kebutuhan POC |
| --- | --- |
| Security | API key disimpan dalam bentuk hash, HMAC diverifikasi dengan constant-time comparison, secret berasal dari environment atau secret store. |
| Reliability | Database transaction digunakan untuk operasi atomik, request eksternal memiliki timeout, worker mendukung graceful shutdown. |
| Performance | Target p95 endpoint pembacaan di bawah 300 ms pada lingkungan lokal, tidak termasuk latensi provider. |
| Availability | Health dan readiness endpoint tersedia, kegagalan satu provider tidak menjatuhkan seluruh API. |
| Auditability | Payment attempt, webhook, dan status transition dapat ditelusuri dengan correlation ID. |
| Maintainability | Domain, application, infrastructure, API, dan provider adapter dipisahkan. |
| Testability | Provider dapat diganti dengan mock, domain rules dapat diuji tanpa database. |
| Observability | Log JSON, request ID, metrics Prometheus, dan masking data sensitif. |
| Portability | API, database, Redis, dan provider simulator dapat dijalankan melalui Docker Compose. |

## 11. Risiko dan Mitigasi

| Risiko | Dampak | Mitigasi |
| --- | --- | --- |
| Pembayaran ganda | Finansial dan reputasi | Idempotency, unique constraint, distributed lock, reconciliation. |
| Timeout dengan hasil tidak diketahui | Failover dapat menggandakan transaksi | Tandai sebagai uncertain, query status provider sebelum failover. |
| Webhook palsu | Manipulasi status | HMAC, timestamp tolerance, replay protection. |
| Race condition | Status tidak konsisten | Transaction, row locking atau optimistic concurrency. |
| Secret tercatat di log | Kebocoran keamanan | Redaction dan structured logging policy. |
| Provider response berubah | Integrasi gagal | Adapter, schema validation, contract test. |
| Scope POC melebar | Keterlambatan | MoSCoW priority dan change control. |

## 12. Asumsi dan Batasan

1. Seluruh provider adalah simulator lokal.
2. Mata uang awal adalah IDR dan SGD.
3. Satu pembayaran hanya menggunakan satu currency.
4. Sistem tidak menyimpan instrumen pembayaran pelanggan.
5. Merchant menerima API key untuk lingkungan POC.
6. PostgreSQL menjadi source of truth.
7. Redis meningkatkan coordination tetapi bukan source of truth.
8. Estimasi WBS mengasumsikan satu engineer full-time dengan pengalaman backend dan pengetahuan dasar Rust.

## 13. Persetujuan Bisnis

| Peran | Nama | Status | Tanggal |
| --- | --- | --- | --- |
| Product Owner | TBD | Pending | TBD |
| Technical Lead | TBD | Pending | TBD |
| Security Reviewer | TBD | Pending | TBD |

