-- Access tokens for the command-line client (phase 7). A token is made in the app, shown once, and
-- only its SHA-256 is kept here, so a copy of this table cannot be used to call the app. The table
-- lives in the encrypted database: while the app is locked no token can be checked, which is the
-- behaviour wanted (a locked app answers "locked" and nothing else).
-- There are no audit triggers on purpose, as with backup_schedule: nothing here belongs in the
-- history. The table is device-specific, so backups leave it out (see EXCLUDED_TABLES in
-- backup.rs) and restoring does not touch it. Revoking a token deletes its row.
-- Timestamps are ISO 8601 UTC with whole seconds ('2026-10-14T09:30:00Z'), so they compare as text.

CREATE TABLE api_tokens (
  id           INTEGER PRIMARY KEY,
  name         TEXT NOT NULL CONSTRAINT ck_api_tokens_name CHECK (length(name) BETWEEN 1 AND 50),
  scope        TEXT NOT NULL CONSTRAINT ck_api_tokens_scope CHECK (scope IN ('READ', 'WRITE')),
  token_hash   TEXT NOT NULL CONSTRAINT ck_api_tokens_hash
    CHECK (length(token_hash) = 64 AND token_hash NOT GLOB '*[^0-9a-f]*'),
  created_at   TEXT NOT NULL,
  expires_at   TEXT NOT NULL,
  last_used_at TEXT
) STRICT;

CREATE UNIQUE INDEX uq_api_tokens_hash ON api_tokens (token_hash);
