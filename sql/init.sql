-- SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
-- SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
-- SPDX-FileCopyrightText: Copyright Contributors to the Bill8:Risk
-- SPDX-License-Identifier: Apache-2.0

-- Bill8:Risk — the service's own schema `risk`: rules, screenings,
-- reviews. Evaluation fails closed: a malformed rule blocks.

CREATE SCHEMA IF NOT EXISTS risk;

CREATE TABLE IF NOT EXISTS risk.rules (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    env_id      UUID NOT NULL,
    predicate    TEXT NOT NULL CHECK (predicate IN ('amount_above', 'country_in', 'velocity_above')),
    params      JSONB NOT NULL DEFAULT '{}',
    action      TEXT NOT NULL CHECK (action IN ('allow', 'review', 'block')),
    priority    INTEGER NOT NULL DEFAULT 100,
    enabled     BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    resource_version INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_risk_rules_env ON risk.rules (env_id);

CREATE TABLE IF NOT EXISTS risk.screenings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    env_id          UUID NOT NULL,
    charge_context  JSONB NOT NULL,
    outcome         TEXT NOT NULL CHECK (outcome IN ('allow', 'review', 'block')),
    matched_rule_id UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_risk_screenings_env ON risk.screenings (env_id);

CREATE TABLE IF NOT EXISTS risk.reviews (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    env_id      UUID NOT NULL,
    screening_id UUID NOT NULL REFERENCES risk.screenings(id),
    status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'approved', 'declined')),
    resolved_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    resource_version INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_risk_reviews_env ON risk.reviews (env_id);

-- ── Row-level security ────────────────────────────────────────────────
ALTER TABLE risk.rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE risk.rules FORCE ROW LEVEL SECURITY;
ALTER TABLE risk.screenings ENABLE ROW LEVEL SECURITY;
ALTER TABLE risk.screenings FORCE ROW LEVEL SECURITY;
ALTER TABLE risk.reviews ENABLE ROW LEVEL SECURITY;
ALTER TABLE risk.reviews FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON risk.rules USING (env_id = current_setting('app.env_id', true)::uuid);
CREATE POLICY tenant_isolation ON risk.screenings USING (env_id = current_setting('app.env_id', true)::uuid);
CREATE POLICY tenant_isolation ON risk.reviews USING (env_id = current_setting('app.env_id', true)::uuid);

-- ── Producer tables ────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS risk.outbox (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    env_id              UUID NOT NULL,
    event_type          TEXT NOT NULL,
    stream_family       TEXT NOT NULL,
    payload             JSONB NOT NULL,
    publisher_id_token  TEXT,
    publisher_subject   TEXT,
    relayed             BOOLEAN NOT NULL DEFAULT false,
    relayed_at          TIMESTAMPTZ,
    relay_attempts      INT NOT NULL DEFAULT 0,
    next_retry_at       TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_risk_outbox_unrelayed ON risk.outbox (created_at) WHERE relayed = false;

CREATE OR REPLACE FUNCTION risk.notify_outbox_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('risk_outbox_changed', NEW.id::text);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS risk_outbox_notify ON risk.outbox;
CREATE TRIGGER risk_outbox_notify AFTER INSERT ON risk.outbox
    FOR EACH ROW EXECUTE FUNCTION risk.notify_outbox_changed();

CREATE TABLE IF NOT EXISTS risk.audit_journal (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    env_id      UUID,
    action      TEXT NOT NULL,
    actor       TEXT,
    subject_type TEXT,
    subject_id  TEXT,
    detail      JSONB NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    pushed      BOOLEAN NOT NULL DEFAULT false
);
CREATE INDEX IF NOT EXISTS idx_risk_audit_journal_unpushed ON risk.audit_journal (created_at) WHERE pushed = false;
-- ── Role grants (schema-ownership convention, bank8-ledger's form) ─────
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'bill8_risk_service') THEN
        ALTER ROLE bill8_risk_service SET search_path = risk, public;
        GRANT USAGE ON SCHEMA risk TO bill8_risk_service;
        GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA risk TO bill8_risk_service;
        GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA risk TO bill8_risk_service;
    END IF;
END $$;
