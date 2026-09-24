CREATE TABLE IF NOT EXISTS version_history (
    account_key TEXT NOT NULL,
    package TEXT NOT NULL,
    channel TEXT NOT NULL,
    version_code TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('uploading', 'complete', 'failed')),
    manifest TEXT NOT NULL,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (account_key, package, channel, version_code)
);
