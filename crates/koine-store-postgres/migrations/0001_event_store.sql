CREATE SCHEMA event_store;

CREATE TABLE event_store.events (
    global_seq     BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    stream_id      UUID        NOT NULL,
    version        BIGINT      NOT NULL,
    event_id       UUID        NOT NULL UNIQUE,
    event_type     TEXT        NOT NULL,
    schema_version SMALLINT    NOT NULL,
    payload        JSONB       NOT NULL,
    correlation_id UUID        NOT NULL,
    causation_id   UUID,
    traceparent    TEXT,
    recorded_at    TIMESTAMPTZ NOT NULL,
    CONSTRAINT events_stream_version_unique UNIQUE (stream_id, version)
);

CREATE SEQUENCE event_store.dispatch_seq;

CREATE TABLE event_store.dispatch_queue (
    job_id           UUID PRIMARY KEY,
    queue            TEXT        NOT NULL,
    priority         SMALLINT    NOT NULL,
    seq              BIGINT      NOT NULL DEFAULT nextval('event_store.dispatch_seq'),
    not_before       TIMESTAMPTZ,
    lease_id         UUID,
    worker_id        TEXT,
    lease_expires_at TIMESTAMPTZ
);

CREATE INDEX dispatch_claim_idx
    ON event_store.dispatch_queue (queue, priority DESC, seq)
    WHERE lease_id IS NULL;

CREATE INDEX dispatch_expiry_idx
    ON event_store.dispatch_queue (lease_expires_at)
    WHERE lease_id IS NOT NULL;

CREATE TABLE event_store.outbox (
    outbox_seq BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_id   UUID  NOT NULL,
    stream_id  UUID  NOT NULL,
    envelope   JSONB NOT NULL
);
