# Production Migration Plan

## Secure Payment Orchestrator

| Informasi | Nilai |
| --- | --- |
| Versi | 1.0 |
| Status | Draft untuk Production Readiness Assessment |
| Tanggal | 13 September 2026 |
| Base Source | POC Rust monolith |
| Target | Production-ready deployment |

---

## 1. Ringkasan Eksekutif

Dokumen ini menjelaskan langkah-langkah yang diperlukan untuk mentransisikan Secure Payment Orchestrator dari **Proof of Concept (POC)** ke **lingkungan production**. POC dibangun sebagai modular monolith dengan provider simulator. Production deployment memerlukan perubahan arsitektur, security hardening, operasional, dan compliance.

**Tingkat Kesiapan Saat Ini:** POC (Proof of Concept) — Tidak untuk production use.

---

## 2. Assessment Gap: POC vs Production

| Area | POC | Production Target |
| --- | --- | --- |
| **Provider** | Simulator lokal (Alpha, Beta, Gamma) | Provider riil (Midtrans, Xendit, Stripe) |
| **Arsitektur** | Modular monolith, satu proses | Microservices atau monolith horizontal scaling |
| **Database** | PostgreSQL 16 (single instance) | PostgreSQL cluster (primary-replica) |
| **Cache** | Redis single instance | Redis cluster / sentinel |
| **Deployment** | Docker Compose (local) | Kubernetes / ECS / GKE |
| **Networking** | HTTP (localhost) | HTTPS (TLS) + CDN + WAF |
| **API Gateway** | Tidak ada | API Gateway (Kong, Envoy, atau AWS API GW) |
| **Secrets** | Environment variables | Vault / AWS Secrets Manager |
| **Observability** | Basic logging + metrics | Full APM (Datadog / OpenTelemetry) |
| **CI/CD** | Manual / dasar | Automated pipeline with canary deployment |
| **Compliance** | Tidak ada | PCI DSS, ISO 27001, atau sesuai regulasi |
| **Disaster Recovery** | Tidak ada | Multi-AZ, backup, DR plan |
| **Load Testing** | Belum dilakukan | Load test dengan target throughput production |

---

## 3. Phase 1: Security Hardening

### 3.1 Secrets Management

| Task | Detail | Prioritas |
| --- | --- | --- |
| Pindahkan secrets ke vault | Integrasi HashiCorp Vault atau AWS Secrets Manager | **High** |
| Rotasi API key | Generate ulang semua API key, update merchant | **High** |
| Rotasi webhook secret | Generate ulang HMAC secret per provider | **High** |
| Implementasi encryption at rest | Enkripsi sensitive field di database | **High** |
| Audit dependency | `cargo audit` + `cargo deny` untuk license compliance | **Medium** |

**Checklist:**
- [ ] Tidak ada secret hardcoded di source code
- [ ] Secret store terintegrasi dengan aplikasi
- [ ] Semua secret memiliki expiry date
- [ ] Access audit untuk secret retrieval

### 3.2 TLS / HTTPS

| Task | Detail | Prioritas |
| --- | --- | --- |
| Sertifikat TLS dari CA trusted | Let's Encrypt / AWS Certificate Manager | **High** |
| Force HTTPS redirect | HTTP → 301 → HTTPS | **High** |
| HSTS header | `Strict-Transport-Security: max-age=31536000` | **Medium** |
| TLS minimum version 1.2 | Disable TLS 1.0 / 1.1 | **High** |

### 3.3 Security Headers & Rate Limiting

| Task | Detail | Prioritas |
| --- | --- | --- |
| Rate limiting per merchant | Token bucket, 1000 req/jam per merchant | **High** |
| Rate limiting per IP | Global rate limit per IP address | **Medium** |
| Security headers | Semua response memiliki security headers | **High** |
| CORS configuration | Whitelist origin merchant yang legitimate | **Medium** |

Security headers yang harus ada di setiap response:
- `X-Content-Type-Options: nosniff`
- `X-Frame-Options: DENY`
- `X-XSS-Protection: 1; mode=block`
- `Strict-Transport-Security: max-age=31536000; includeSubDomains`

### 3.4 Input Validation & Sanitization

- Upgrade validasi dari basic check ke comprehensive validation
- Implementasi content-length limit (max 10KB per request)
- Request body size limit di Axum layer
- JSON schema validation dengan library external

---
## 4. Phase 2: Infrastructure & Deployment

### 4.1 Production Architecture

```
                    ┌──────────────┐
                    │  Load Balancer │
                    │  (ALB / Nginx)│
                    └──────┬───────┘
                           │
              ┌────────────┼────────────┐
              ▼             ▼             ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │ SPO API 1│ │ SPO API 2│ │ SPO API 3│
        └────┬─────┘ └────┬─────┘ └────┬─────┘
             │             │             │
        ┌────┴─────────────┴─────────────┴────┐
        │        PostgreSQL Cluster          │
        │    Primary ← Streaming → Standby   │
        └─────────────────────────────────────┘
             │             │             │
        ┌────┴─────────────┴─────────────┴────┐
        │          Redis Cluster             │
        │     Master + Sentinel / Replicas   │
        └─────────────────────────────────────┘
```

### 4.2 Dockerfile Production

```dockerfile
FROM rust:1.80-slim-bullseye AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM gcr.io/distroless/cc-debian11
WORKDIR /app
COPY --from=builder /app/target/release/spo-api .
COPY --from=builder /app/migrations ./migrations
EXPOSE 8080
USER 10001
HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
  CMD ["/app/spo-api", "--check-health"]
ENTRYPOINT ["/app/spo-api"]
```

### 4.3 Database Production

| Task | Detail | Prioritas |
| --- | --- | --- |
| PostgreSQL cluster | Primary + Standby streaming replication | **High** |
| Automated backup | pg_dump harian + WAL archiving untuk PITR | **High** |
| Connection pooling | PgBouncer untuk manajemen koneksi | **High** |
| Migration strategy | Zero-downtime, backward-compatible changes | **Medium** |

### 4.4 Redis Production

| Task | Detail | Prioritas |
| --- | --- | --- |
| Redis Sentinel / Cluster | High availability untuk distributed lock | **High** |
| Persistent mode | AOF + RDB untuk durability | **Medium** |
| Memory limit | `maxmemory` dengan `allkeys-lru` eviction | **High** |
| Authentication | Redis AUTH dengan strong password | **High** |

---
## 6. Phase 4: Observability & Monitoring

### 6.1 Logging Production

| Requirement | Detail |
| --- | --- |
| Format | JSON structured logging via `tracing` |
| Level | INFO untuk flow, WARN untuk anomalies, ERROR untuk failures |
| Sampling | 100% error & warn, 10% info (untuk biaya) |
| Aggregation | CloudWatch / ELK Stack / Datadog |
| Retention | 30 hari active, 1 tahun archive |
| Redaction | Implementasi `Sensitive` wrapper untuk data sensitif |

### 6.2 Metrics & Alerting

| Metric | Type | Alert Threshold | Action |
| --- | --- | --- | --- |
| `payments_error_rate` | Counter | > 5% dalam 5 menit | PagerDuty critical |
| `payment_p95_latency_ms` | Histogram | > 2000ms | PagerDuty warning |
| `provider_failure_rate` | Counter | > 20% per provider | Provider failover |
| `circuit_breaker_state` | Gauge | state = OPEN | PagerDuty critical |
| `database_connections` | Gauge | > 80% dari pool | Scale connection pool |

### 6.3 Distributed Tracing

- Implementasi OpenTelemetry SDK
- Trace propagation via HTTP headers (W3C Trace Context)
- Integrasi dengan Jaeger / AWS X-Ray
- Span: API → PaymentService → Provider call → Database query

### 6.4 Grafana Dashboard

Buat dashboard dengan panel:
- **Payment Volume** — Total payments, success rate, error rate
- **Provider Performance** — Latency p50/p95/p99 per provider
- **Business Metrics** — Total amount processed, unique merchants
- **System Health** — CPU, memory, connections, uptime

---

## 7. Phase 5: Compliance & Legal

### 7.1 PCI DSS Readiness

| Requirement | Action | Timeline |
| --- | --- | --- |
| Tidak menyimpan PAN | ✅ Already compliant | Saat ini |
| Encryption in transit | TLS 1.2+ untuk semua komunikasi | Phase 1 |
| Access control | API key + RBAC | Phase 1 |
| Audit trail | ✅ Already compliant (audit_logs table) | Saat ini |
| Vulnerability scanning | `cargo audit` + dependency scanning | Phase 1 |
| Penetration testing | Jadwalkan penetration test | Phase 4 |

### 7.2 SLA & Error Budget

| Metrik | Target SLA | Error Budget (30 hari) |
| --- | --- | --- |
| Uptime API | 99.9% | 43 menit downtime/bulan |
| Payment success rate | ≥ 98% | 1.2% failure rate |
| API p95 latency | < 500ms | — |
| Webhook processing | < 60 detik | — |

### 7.3 Data Retention

| Tipe Data | Retention | Deletion Policy |
| --- | --- | --- |
| Payment records | 5 tahun | Soft-delete → hard-delete |
| Audit logs | 3 tahun | Archive → delete |
| Attempt logs | 1 tahun | Auto-delete |
| Webhook payloads | 90 hari | Auto-delete |
| API key hashes | Revoked + 30 hari | Hard-delete |

---

## 8. Phase 6: Disaster Recovery & HA

### 8.1 Multi-AZ Deployment

```
Region: ap-southeast-1 (Singapura)
├── AZ A: SPO API 1, PostgreSQL Primary, Redis Master
├── AZ B: SPO API 2, PostgreSQL Standby, Redis Replica
└── AZ C: SPO API 3, PostgreSQL Standby (failover)
```

### 8.2 Backup Strategy

| Backup | Frequency | Retention | RTO |
| --- | --- | --- | --- |
| PostgreSQL full backup | Harian (02:00) | 30 hari | 2 jam |
| WAL streaming | Continuous | 7 hari | 15 menit (PITR) |
| Redis snapshot | Setiap 6 jam | 3 hari | 30 menit |
| Configuration | Setiap deploy | 90 hari | 30 menit |

### 8.3 Disaster Recovery Plan

| Skenario | RTO | RPO | Prosedur |
| --- | --- | --- | --- |
| Instance crash | 30 detik | 0 | Auto-restart orchestrator |
| AZ failure | 5 menit | 5 detik | Route traffic ke AZ lain |
| Database corruption | 2 jam | 1 hari | Restore dari backup |
| Region failure | 4 jam | 1 jam | Active-passive region 2 |
| Data center disaster | 8 jam | 1 hari | Restore backup offsite |

### 8.4 Runbook Checklist

**Daily:**
- [ ] Cek health endpoint (automated)
- [ ] Monitor error rate dashboard
- [ ] Review slow query log

**Weekly:**
- [ ] Review audit log untuk anomalies
- [ ] Test backup restore (staging)
- [ ] Rotasi API key yang akan expired
## 10. Capacity Planning

| Komponen | 100 req/mnt | 1000 req/mnt | 10000 req/mnt |
| --- | --- | --- | --- |
| CPU (API) | 0.5 core | 2 core | 8 core (auto-scale) |
| Memory (API) | 256 MB | 1 GB | 4 GB |
| Database connections | 10 | 50 | 200 (Pakai PgBouncer) |
| Redis memory | 50 MB | 200 MB | 1 GB |
| Storage (db/bulan) | 100 MB | 1 GB | 10 GB |

### Auto-scaling (Kubernetes HPA)

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: spo-api-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: spo-api
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

---

## 11. Timeline & Milestones

| Phase | Minggu | Aktivitas Utama |
| --- | --- | --- |
| **Phase 1** | 1-2 | Security hardening (secrets, TLS, rate limit, validasi) |
| **Phase 2** | 3-4 | Infrastructure (Docker, database, Redis, CI/CD) |
| **Phase 3** | 4-6 | Provider integration (Midtrans, Xendit, contract test) |
| **Phase 4** | 5-6 | Observability (logging, metrics, tracing, dashboard) |
| **Phase 5** | 5-8 | Compliance (PCI DSS, data retention, SLA) |
| **Phase 6** | 7-8 | DR & HA (multi-AZ, backup, runbook, DR drill) |

**Total estimasi:** 8 minggu (2 bulan) untuk production readiness.

---

## 12. Risk Register

| Risiko | Prob. | Impact | Mitigasi |
| --- | --- | --- | --- |
| Provider API breaking change | Medium | High | Contract test, adapter isolation |
| Secrets exposure di log | Low | Critical | Log redaction, `cargo audit` |
| Database migration failure | Medium | High | Backward-compatible, rollback test |
| Redis lock inconsistency | Low | Medium | DB sebagai source of truth |
| TLS certificate expiry | Low | High | Auto-renewal (cert-manager) |
| DDoS attack | Medium | High | WAF + rate limiting + auto-scale |
| Provider downtime | High | Medium | Circuit breaker + failover + retry |

---

## 13. Production Readiness Checklist

### Pre-Launch (H-7)
- [ ] Semua security headers terpasang
- [ ] TLS certificate aktif & auto-renewal terkonfigurasi
- [ ] Rate limiting aktif untuk semua endpoint
- [ ] Database backup terverifikasi
- [ ] Secrets management terintegrasi
- [ ] Health & readiness endpoint berfungsi
- [ ] Monitoring dashboard siap
- [ ] Alerting threshold terkonfigurasi
- [ ] Rollback script terverifikasi

### Launch Day (H-0)
- [ ] Smoke test di production
- [ ] Monitor error rate (60 menit pertama)
- [ ] Monitor latency (60 menit pertama)
- [ ] Webhook signature test dengan provider riil
- [ ] DNS cutover (jika ada domain change)

### Post-Launch (H+48)
- [ ] Review error rate & latency
- [ ] Review audit log untuk anomalies
- [ ] Optimasi query jika diperlukan
- [ ] Update documentation
- [ ] Post-mortem jika ada incident (P1/P2)

---

## 14. Referensi

| Dokumen | Deskripsi |
| --- | --- |
| [BRD-Secure-Payment-Orchestrator.md](../BRD-Secure-Payment-Orchestrator.md) | Business Requirements Document |
| [PRD-Secure-Payment-Orchestrator.md](../PRD-Secure-Payment-Orchestrator.md) | Product Requirements Document |
| [Architecture-Secure-Payment-Orchestrator.md](./architecture/Architecture-Secure-Payment-Orchestrator.md) | Architecture Document |
| [ERD-Secure-Payment-Orchestrator.md](./erd/ERD-Secure-Payment-Orchestrator.md) | Entity Relationship Diagram |

| External | Deskripsi |
| --- | --- |
| [OWASP Top 10](https://owasp.org/www-project-top-ten/) | Web application security risks |
| [PCI DSS v4.0](https://www.pcisecuritystandards.org/) | Payment card industry standards |
| [AWS Well-Architected Framework](https://aws.amazon.com/architecture/well-architected/) | Cloud architecture best practices |

---

## 15. Changelog

| Versi | Tanggal | Perubahan | Penulis |
| --- | --- | --- | --- |
| 1.0 | 13 September 2026 | Draft awal Production Migration Plan | Engineering Team |

**Monthly:**
- [ ] Review dependency vulnerabilities (`cargo audit`)
- [ ] Load test dengan expected peak traffic
- [ ] Disaster recovery drill

**Incident Response:**
1. Deteksi anomaly (alert)
2. Pager duty on-call engineer
3. Assess severity (P1-P4)
4. Mitigasi (rollback, failover, atau fix forward)
5. Post-mortem dalam 48 jam

---

## 9. Rollback Strategy

| Strategy | Method | Time | Impact |
| --- | --- | --- | --- |
| Blue-green | Dua environment identik | 5 menit | Zero downtime |
| Canary | 5% → 25% → 50% → 100% | 30 menit | Minimal impact |
| Rollback | Re-deploy versi sebelumnya | 5 menit | 1-2 menit downtime |
| Feature flag | Matikan fitur via config | 1 menit | Zero downtime |

**Recommended:** Blue-green + feature flags untuk perubahan berisiko.

---

## 5. Phase 3: Provider Integration

### 5.1 Dari Simulator ke Provider Riil

| Provider | Tipe | API Method | Authentication |
| --- | --- | --- | --- |
| **Midtrans** | Payment gateway | REST + Webhook | Server Key (Basic Auth) |
| **Xendit** | Payment gateway | REST + Webhook | API Key (Bearer) |
| **Stripe** | Payment gateway | REST + Webhook | API Key (Bearer) |

### 5.2 Provider Adapter Template

```rust
#[async_trait]
impl PaymentProvider for MidtransAdapter {
    fn name(&self) -> &str { "midtrans" }
    fn is_available(&self) -> bool { self.circuit_breaker.is_closed() }
    
    async fn create_payment(&self, request: ProviderRequest) 
        -> Result<ProviderResponse, ProviderError> 
    {
        // 1. Transform request ke format provider
        // 2. Sign request dengan credentials
        // 3. Send HTTPS request dengan timeout 5s
        // 4. Parse & normalize response
        // 5. Return canonical ProviderResponse
    }
}
```

### 5.3 Contract Testing

| Task | Detail | Prioritas |
| --- | --- | --- |
| Provider contract test | Test setiap adapter secara terisolasi | **High** |
| Integration test sandbox | Gunakan sandbox environment provider | **High** |
| Error mapping completeness | Mapping semua HTTP status dan error codes | **High** |
| Webhook signature test | Verifikasi HMAC dengan secret production | **High** |

---