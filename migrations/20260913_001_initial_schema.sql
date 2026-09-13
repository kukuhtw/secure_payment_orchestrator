-- Initial schema for Secure Payment Orchestrator
-- Migration: 001_initial_schema

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ─── Merchants ──────────────────────────────────────────
CREATE TABLE merchants (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name        VARCHAR(255) NOT NULL,
    merchant_code VARCHAR(50) NOT NULL UNIQUE,
    contact_email VARCHAR(255) NOT NULL,
    status      VARCHAR(20) NOT NULL DEFAULT 'ACTIVE'
                CHECK (status IN ('ACTIVE', 'SUSPENDED', 'INACTIVE')),
    supported_currencies JSONB NOT NULL DEFAULT '["IDR", "SGD"]'::JSONB,
    config      JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ─── API Keys ────────────────────────────────────────────
CREATE TABLE api_keys (
    id          UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    merchant_id UUID NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    key_hash    VARCHAR(255) NOT NULL,
    key_prefix  VARCHAR(10) NOT NULL UNIQUE,
    label       VARCHAR(100),
    permissions VARCHAR(50) NOT NULL DEFAULT 'standard'
                CHECK (permissions IN ('standard', 'operations', 'admin')),
    expires_at  TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at  TIMESTAMPTZ
);

CREATE INDEX idx_api_keys_merchant_id ON api_keys(merchant_id);

-- ─── Payments ────────────────────────────────────────────
CREATE TABLE payments (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    merchant_id         UUID NOT NULL REFERENCES merchants(id),
    idempotency_key     VARCHAR(255) NOT NULL,
    merchant_reference  VARCHAR(255) NOT NULL,
    currency            VARCHAR(3) NOT NULL,
    amount              BIGINT NOT NULL CHECK (amount > 0),
    description         TEXT,
    status              VARCHAR(30) NOT NULL DEFAULT 'PENDING'
                        CHECK (status IN ('PENDING', 'PROCESSING', 'SUCCESS',
                          'FAILED', 'PENDING_RETRY', 'PENDING_RECONCILIATION',
                          'CANCELLED')),
    provider            VARCHAR(50),
    failure_reason      TEXT,
    payment_url         VARCHAR(500),
    created_by_api_key_id UUID REFERENCES api_keys(id),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at        TIMESTAMPTZ,

    CONSTRAINT uq_payments_idempotency UNIQUE (merchant_id, idempotency_key),
    CONSTRAINT uq_payments_merchant_ref UNIQUE (merchant_id, merchant_reference)
);

-- ─── Payment Attempts ────────────────────────────────────
CREATE TABLE payment_attempts (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    payment_id          UUID NOT NULL REFERENCES payments(id) ON DELETE CASCADE,
    provider            VARCHAR(50) NOT NULL,
    provider_payment_id VARCHAR(255),
    provider_status     VARCHAR(50),
    status              VARCHAR(30) NOT NULL,
    attempt_type        VARCHAR(20) NOT NULL
                        CHECK (attempt_type IN ('INITIAL', 'RETRY', 'RECONCILIATION', 'FAILOVER')),
    attempt_number      INTEGER NOT NULL,
    request_snapshot    JSONB,
    response_snapshot   JSONB,
    http_status_code    INTEGER,
    error_code          VARCHAR(100),
    error_message       TEXT,
    duration_ms         BIGINT,
    started_at          TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_attempts_payment_id ON payment_attempts(payment_id);
CREATE INDEX idx_attempts_payment_attempt ON payment_attempts(payment_id, attempt_number);

-- ─── Webhook Events ──────────────────────────────────────
CREATE TABLE webhook_events (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    payment_id          UUID NOT NULL REFERENCES payments(id),
    provider            VARCHAR(50) NOT NULL,
    event_id            VARCHAR(255) NOT NULL,
    event_type          VARCHAR(50) NOT NULL,
    signature           VARCHAR(500) NOT NULL,
    raw_body            TEXT NOT NULL,
    verification_status VARCHAR(20) NOT NULL DEFAULT 'PENDING'
                        CHECK (verification_status IN ('PENDING', 'VALID', 'INVALID')),
    processing_status   VARCHAR(20) NOT NULL DEFAULT 'PENDING'
                        CHECK (processing_status IN ('PENDING', 'PROCESSED', 'DUPLICATE', 'FAILED')),
    verification_error  TEXT,
    received_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    verified_at         TIMESTAMPTZ,
    processed_at        TIMESTAMPTZ,

    CONSTRAINT uq_webhook_events_provider_event UNIQUE (provider, event_id)
);

CREATE INDEX idx_webhook_events_payment_id ON webhook_events(payment_id);

-- ─── Audit Logs ──────────────────────────────────────────
CREATE TABLE audit_logs (
    id              UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    merchant_id     UUID REFERENCES merchants(id),
    payment_id      UUID REFERENCES payments(id),
    entity_id       UUID,
    entity_type     VARCHAR(50) NOT NULL,
    action          VARCHAR(50) NOT NULL,
    actor           VARCHAR(100) NOT NULL,
    field_name      VARCHAR(100),
    old_value       TEXT,
    new_value       TEXT,
    metadata        JSONB,
    ip_address      INET,
    correlation_id  VARCHAR(100),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_audit_logs_merchant_id ON audit_logs(merchant_id);
CREATE INDEX idx_audit_logs_payment_id ON audit_logs(payment_id);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at DESC);
CREATE INDEX idx_audit_logs_correlation_id ON audit_logs(correlation_id);

-- ─── Idempotency Keys ────────────────────────────────────
CREATE TABLE idempotency_keys (
    idempotency_key     VARCHAR(255) NOT NULL,
    merchant_id         UUID NOT NULL REFERENCES merchants(id),
    request_hash        VARCHAR(64) NOT NULL,
    payment_id          UUID NOT NULL REFERENCES payments(id),
    response_status_code VARCHAR(3),
    response_body       JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at          TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '24 hours',
    PRIMARY KEY (idempotency_key, merchant_id)
);

CREATE INDEX idx_idempotency_keys_expires ON idempotency_keys(expires_at)
    WHERE expires_at > NOW();

-- ─── Reconciliation Records ──────────────────────────────
CREATE TABLE reconciliation_records (
    id                  UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    payment_id          UUID NOT NULL REFERENCES payments(id),
    payment_attempt_id  UUID REFERENCES payment_attempts(id),
    reconciliation_type VARCHAR(30) NOT NULL
                        CHECK (reconciliation_type IN ('TIMEOUT', 'FAILOVER', 'MANUAL')),
    previous_status     VARCHAR(30) NOT NULL,
    current_status      VARCHAR(30),
    provider_status     VARCHAR(50),
    resolution          VARCHAR(30) NOT NULL
                        CHECK (resolution IN ('COMPLETED', 'FAILED', 'UNCERTAIN')),
    details             JSONB,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reconciliation_payment_id ON reconciliation_records(payment_id);

-- ─── Circuit Breakers ────────────────────────────────────
CREATE TABLE circuit_breakers (
    provider_key        VARCHAR(50) PRIMARY KEY,
    state               VARCHAR(15) NOT NULL DEFAULT 'CLOSED'
                        CHECK (state IN ('CLOSED', 'OPEN', 'HALF_OPEN')),
    failure_count       INTEGER NOT NULL DEFAULT 0,
    success_count       INTEGER NOT NULL DEFAULT 0,
    last_failure_at     TIMESTAMPTZ,
    last_success_at     TIMESTAMPTZ,
    opened_at           TIMESTAMPTZ,
    half_open_at        TIMESTAMPTZ,
    next_retry_at       TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ─── Seed Data ───────────────────────────────────────────
INSERT INTO merchants (id, name, merchant_code, contact_email, status)
VALUES ('00000000-0000-0000-0000-000000000001', 'Demo Merchant', 'DEMO', 'demo@example.com', 'ACTIVE');

INSERT INTO merchants (id, name, merchant_code, contact_email, status)
VALUES ('00000000-0000-0000-0000-000000000002', 'Operations', 'OPS', 'ops@example.com', 'ACTIVE');
CREATE INDEX idx_payments_merchant_id ON payments(merchant_id);
CREATE INDEX idx_payments_status ON payments(status);
CREATE INDEX idx_payments_created_at ON payments(created_at DESC);