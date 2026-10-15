-- Sync foundation (phase 8, see docs/sync-design.md).
--
-- Every row that is shared between the desktop and the phone gets a random `sync_id` (the integer ids stay local: two
-- devices would both make "transaction 57") and a `stamp` that says when it was last changed. A stamp is text that
-- sorts like (milliseconds, counter, first 8 characters of the device id), so a larger stamp is a newer change. It comes
-- from a hybrid clock kept in `sync_clock`: it never goes backwards and never falls behind a stamp the device has seen.
--
-- Stamps are set by triggers, as updated_at is, so no way of writing data (the screens, the command API, recurring
-- items) can forget one. While a sync or a restore writes rows it sets `sync_clock.applying`, and the triggers then
-- leave the stamps of the incoming rows as they are. Hard deletes leave a marker in `sync_deleted`.
--
-- Stamp triggers watch only the columns that are content (`UPDATE OF`), so the changes that need no sending do not
-- make a stamp: updated_at itself, the link from a transaction to its recurring item (it is cleared on both devices by
-- ON DELETE SET NULL), and the sync columns. SQLite does not run a trigger from inside itself, and these never touch
-- the columns they watch, so none of them can start another.
--
-- `sync_clock` and `sync_peers` belong to this device: a backup leaves them out and a restore does not touch them (see
-- EXCLUDED_TABLES in backup.rs). `sync_deleted` and `currency_changes` are data and are in backups.

CREATE TABLE sync_clock (
  id           INTEGER PRIMARY KEY CONSTRAINT ck_sync_clock_one_row CHECK (id = 1),
  device_id    TEXT NOT NULL CONSTRAINT ck_sync_clock_device CHECK (length(device_id) = 32 AND device_id NOT GLOB '*[^0-9a-f]*'),
  last_ms      INTEGER NOT NULL,
  last_counter INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_sync_clock_counter CHECK (last_counter BETWEEN 0 AND 65535),
  applying     INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_sync_clock_applying CHECK (applying IN (0, 1))
) STRICT;

INSERT INTO sync_clock (id, device_id, last_ms) VALUES (1, lower(hex(randomblob(16))), (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)));

-- The old updated_at triggers go first: the backfill below must not move any updated_at. They come back, restricted to
-- the columns they watched before, so that setting sync_id or stamp does not touch updated_at.

DROP TRIGGER trg_settings_updated_at;
DROP TRIGGER trg_categories_updated_at;
DROP TRIGGER trg_budgets_updated_at;
DROP TRIGGER trg_recurring_transactions_updated_at;
DROP TRIGGER trg_transactions_updated_at;

ALTER TABLE categories ADD COLUMN sync_id TEXT;
ALTER TABLE categories ADD COLUMN stamp TEXT;
ALTER TABLE budgets ADD COLUMN sync_id TEXT;
ALTER TABLE budgets ADD COLUMN stamp TEXT;
ALTER TABLE recurring_transactions ADD COLUMN sync_id TEXT;
ALTER TABLE recurring_transactions ADD COLUMN stamp TEXT;
ALTER TABLE transactions ADD COLUMN sync_id TEXT;
ALTER TABLE transactions ADD COLUMN stamp TEXT;
ALTER TABLE categories ADD COLUMN merged_into TEXT;
ALTER TABLE settings ADD COLUMN stamp TEXT;
ALTER TABLE settings ADD COLUMN currency_generation INTEGER NOT NULL DEFAULT 0;

-- Rows that exist now: a new random id, and a stamp worked out from when the row was last updated, so the order of old edits is kept.
UPDATE settings SET stamp = printf('%013d-0000-%s', COALESCE(CAST(round((julianday(updated_at) - 2440587.5) * 86400000.0) AS INTEGER), (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER))), (SELECT substr(device_id, 1, 8) FROM sync_clock WHERE id = 1));
UPDATE categories SET sync_id = lower(hex(randomblob(16))), stamp = printf('%013d-0000-%s', COALESCE(CAST(round((julianday(updated_at) - 2440587.5) * 86400000.0) AS INTEGER), (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER))), (SELECT substr(device_id, 1, 8) FROM sync_clock WHERE id = 1));
UPDATE budgets SET sync_id = lower(hex(randomblob(16))), stamp = printf('%013d-0000-%s', COALESCE(CAST(round((julianday(updated_at) - 2440587.5) * 86400000.0) AS INTEGER), (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER))), (SELECT substr(device_id, 1, 8) FROM sync_clock WHERE id = 1));
UPDATE recurring_transactions SET sync_id = lower(hex(randomblob(16))), stamp = printf('%013d-0000-%s', COALESCE(CAST(round((julianday(updated_at) - 2440587.5) * 86400000.0) AS INTEGER), (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER))), (SELECT substr(device_id, 1, 8) FROM sync_clock WHERE id = 1));
UPDATE transactions SET sync_id = lower(hex(randomblob(16))), stamp = printf('%013d-0000-%s', COALESCE(CAST(round((julianday(updated_at) - 2440587.5) * 86400000.0) AS INTEGER), (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER))), (SELECT substr(device_id, 1, 8) FROM sync_clock WHERE id = 1));

UPDATE sync_clock SET last_ms = MAX(last_ms, COALESCE((SELECT MAX(CAST(substr(stamp, 1, 13) AS INTEGER)) FROM (
  SELECT stamp FROM settings UNION ALL SELECT stamp FROM categories UNION ALL SELECT stamp FROM budgets
  UNION ALL SELECT stamp FROM recurring_transactions UNION ALL SELECT stamp FROM transactions)), 0)) WHERE id = 1;

CREATE UNIQUE INDEX uq_categories_sync_id ON categories (sync_id);
CREATE UNIQUE INDEX uq_budgets_sync_id ON budgets (sync_id);
CREATE UNIQUE INDEX uq_recurring_transactions_sync_id ON recurring_transactions (sync_id);
CREATE UNIQUE INDEX uq_transactions_sync_id ON transactions (sync_id);

-- Hard deletes (transactions and recurring items are deleted for good) leave a marker, so the other device learns of them.
CREATE TABLE sync_deleted (
  id      INTEGER PRIMARY KEY,
  sync_id TEXT NOT NULL CONSTRAINT ck_sync_deleted_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  kind    TEXT NOT NULL CONSTRAINT ck_sync_deleted_kind CHECK (kind IN ('budgets', 'recurring_transactions', 'transactions')),
  stamp   TEXT NOT NULL
) STRICT;
CREATE UNIQUE INDEX uq_sync_deleted_sync_id ON sync_deleted (sync_id);

-- The devices this one is paired with and the secret each pairing made (32 random bytes as 64 hex characters). It lives in the
-- encrypted database. There are no audit triggers on purpose, and a backup, an export, the command line and a sync never carry it.
CREATE TABLE sync_peers (
  id           INTEGER PRIMARY KEY,
  device_id    TEXT NOT NULL CONSTRAINT ck_sync_peers_device CHECK (length(device_id) = 32 AND device_id NOT GLOB '*[^0-9a-f]*'),
  name         TEXT NOT NULL CONSTRAINT ck_sync_peers_name CHECK (length(name) BETWEEN 1 AND 50),
  secret       TEXT NOT NULL CONSTRAINT ck_sync_peers_secret CHECK (length(secret) = 64 AND secret NOT GLOB '*[^0-9a-f]*'),
  paired_at    TEXT NOT NULL,
  last_sync_at TEXT
) STRICT;
CREATE UNIQUE INDEX uq_sync_peers_device ON sync_peers (device_id);

-- Every change of currency, in order. `id` is the generation the change leads to (1, 2, ...), and `settings.currency_generation` is
-- the generation this device is at. A NULL rate means the change only relabelled the amounts. See docs/sync-design.md, section 11.
CREATE TABLE currency_changes (
  id            INTEGER PRIMARY KEY,
  from_currency TEXT NOT NULL CONSTRAINT ck_currency_changes_from CHECK (from_currency GLOB '[A-Z][A-Z][A-Z]'),
  to_currency   TEXT NOT NULL CONSTRAINT ck_currency_changes_to CHECK (to_currency GLOB '[A-Z][A-Z][A-Z]'),
  rate          TEXT,
  stamp         TEXT NOT NULL
) STRICT;

-- updated_at, as before (see 0002_triggers.sql), now watching the columns the table had before this migration.
CREATE TRIGGER trg_settings_updated_at
AFTER UPDATE OF id, currency, locale, timezone, created_at, updated_at, language ON settings
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE settings SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_categories_updated_at
AFTER UPDATE OF id, type, name, deleted_at, created_at, updated_at ON categories
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE categories SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_budgets_updated_at
AFTER UPDATE OF id, type, scope, category_id, amount_limit_minor, effective_from, created_at, updated_at ON budgets
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE budgets SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_updated_at
AFTER UPDATE OF id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date, is_active, created_at, updated_at ON recurring_transactions
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE recurring_transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_updated_at
AFTER UPDATE OF id, category_id, recurring_transaction_id, recurring_due_date, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, created_at, updated_at ON transactions
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

-- A new row gets its sync_id and stamp. A row that arrives from a sync or a restore has them already.
CREATE TRIGGER trg_categories_sync_insert
AFTER INSERT ON categories
WHEN NEW.sync_id IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE categories SET sync_id = lower(hex(randomblob(16))), stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_budgets_sync_insert
AFTER INSERT ON budgets
WHEN NEW.sync_id IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE budgets SET sync_id = lower(hex(randomblob(16))), stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_sync_insert
AFTER INSERT ON recurring_transactions
WHEN NEW.sync_id IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE recurring_transactions SET sync_id = lower(hex(randomblob(16))), stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_sync_insert
AFTER INSERT ON transactions
WHEN NEW.sync_id IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transactions SET sync_id = lower(hex(randomblob(16))), stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_settings_sync_insert
AFTER INSERT ON settings
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE settings SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- A change to what a row holds gives it a new stamp, unless a sync or a restore is writing it.
CREATE TRIGGER trg_categories_sync_update
AFTER UPDATE OF type, name, deleted_at, merged_into ON categories
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE categories SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_budgets_sync_update
AFTER UPDATE OF type, scope, category_id, amount_limit_minor, effective_from ON budgets
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE budgets SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_sync_update
AFTER UPDATE OF category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date, is_active ON recurring_transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE recurring_transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_sync_update
AFTER UPDATE OF category_id, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description ON transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- Only the shared settings count. The language belongs to this device, and the currency travels as a currency change.
CREATE TRIGGER trg_settings_sync_update
AFTER UPDATE OF locale, timezone ON settings
WHEN (NEW.locale IS NOT OLD.locale OR NEW.timezone IS NOT OLD.timezone) AND NEW.stamp IS OLD.stamp AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE settings SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- A deleted row leaves a marker with a stamp.
CREATE TRIGGER trg_budgets_sync_delete
AFTER DELETE ON budgets
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'budgets', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_recurring_transactions_sync_delete
AFTER DELETE ON recurring_transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'recurring_transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_transactions_sync_delete
AFTER DELETE ON transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;
