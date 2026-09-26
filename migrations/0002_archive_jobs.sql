ALTER TABLE version_history ADD COLUMN plan TEXT;
ALTER TABLE version_history ADD COLUMN hash_state TEXT;
ALTER TABLE version_history ADD COLUMN hash_bytes TEXT NOT NULL DEFAULT '0';
ALTER TABLE version_history ADD COLUMN lease_until INTEGER NOT NULL DEFAULT 0;
