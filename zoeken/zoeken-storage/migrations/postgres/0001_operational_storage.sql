CREATE TABLE engine_health (
    engine TEXT NOT NULL,
    bucket BIGINT NOT NULL,
    latency_ms_sum BIGINT NOT NULL DEFAULT 0,
    successes BIGINT NOT NULL DEFAULT 0,
    timeouts BIGINT NOT NULL DEFAULT 0,
    errors BIGINT NOT NULL DEFAULT 0,
    circuit_status TEXT NOT NULL DEFAULT 'closed',
    cooldown_until_ms BIGINT,
    last_error_category TEXT,
    PRIMARY KEY (engine, bucket)
);

CREATE TABLE origin_budgets (
    origin TEXT PRIMARY KEY,
    tokens DOUBLE PRECISION NOT NULL,
    last_refill_ms BIGINT NOT NULL
);

CREATE TABLE origin_leases (
    lease_id TEXT PRIMARY KEY,
    origin TEXT NOT NULL,
    expires_at_ms BIGINT NOT NULL
);

CREATE INDEX origin_leases_origin_expiry
    ON origin_leases (origin, expires_at_ms);

CREATE TABLE favicon_blobs (
    digest TEXT PRIMARY KEY,
    size_bytes BIGINT NOT NULL,
    mime TEXT NOT NULL,
    data BYTEA NOT NULL,
    created_at_ms BIGINT NOT NULL
);

CREATE TABLE favicon_mappings (
    resolver TEXT NOT NULL,
    authority TEXT NOT NULL,
    digest TEXT,
    is_negative BOOLEAN NOT NULL,
    expires_at_ms BIGINT NOT NULL,
    PRIMARY KEY (resolver, authority)
);
