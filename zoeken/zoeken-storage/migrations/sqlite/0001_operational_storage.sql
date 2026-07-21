CREATE TABLE engine_health (
    engine TEXT NOT NULL,
    bucket INTEGER NOT NULL,
    latency_ms_sum INTEGER NOT NULL DEFAULT 0,
    successes INTEGER NOT NULL DEFAULT 0,
    timeouts INTEGER NOT NULL DEFAULT 0,
    errors INTEGER NOT NULL DEFAULT 0,
    circuit_status TEXT NOT NULL DEFAULT 'closed',
    cooldown_until_ms INTEGER,
    last_error_category TEXT,
    PRIMARY KEY (engine, bucket)
);

CREATE TABLE origin_budgets (
    origin TEXT PRIMARY KEY,
    tokens REAL NOT NULL,
    last_refill_ms INTEGER NOT NULL
);

CREATE TABLE origin_leases (
    lease_id TEXT PRIMARY KEY,
    origin TEXT NOT NULL,
    expires_at_ms INTEGER NOT NULL
);

CREATE INDEX origin_leases_origin_expiry
    ON origin_leases (origin, expires_at_ms);

CREATE TABLE favicon_blobs (
    digest TEXT PRIMARY KEY,
    size_bytes INTEGER NOT NULL,
    mime TEXT NOT NULL,
    data BLOB NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE favicon_mappings (
    resolver TEXT NOT NULL,
    authority TEXT NOT NULL,
    digest TEXT,
    is_negative INTEGER NOT NULL,
    expires_at_ms INTEGER NOT NULL,
    PRIMARY KEY (resolver, authority)
);
