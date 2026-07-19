CREATE TABLE event_store.workers (
    worker_id  TEXT PRIMARY KEY,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen  TIMESTAMPTZ NOT NULL,
    last_queue TEXT
);
