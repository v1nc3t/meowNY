-- Scheduled backups: one row, set up in Settings. It lives in the database so the backup passphrase
-- is only readable while the app is unlocked. There are no audit triggers on purpose: the
-- passphrase must never reach `audit_log`. The table is device-specific, so backups leave it out
-- (see EXCLUDED_TABLES in backup.rs) and restoring does not change it.

CREATE TABLE backup_schedule (
  id              INTEGER PRIMARY KEY CONSTRAINT ck_backup_schedule_one_row CHECK (id = 1),
  folder          TEXT NOT NULL,
  every_days      INTEGER NOT NULL CONSTRAINT ck_backup_schedule_every CHECK (every_days IN (1, 7, 30)),
  passphrase      TEXT NOT NULL,
  last_attempt_at TEXT,
  last_ok         INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_backup_schedule_ok CHECK (last_ok IN (0, 1))
) STRICT;
