-- Accounts and transfers (docs/accounts-design.md, section 4).
--
-- An account is a place the money is: a bank account, savings, cash, a credit card. Every transaction, recurring item and ledger entry
-- belongs to one account, and a transfer moves money between two of them. A balance is stored nowhere: the view `account_movements` has
-- one row for everything that moved an account, and a balance is the sum of its `delta_minor` up to a day.
--
-- There is always the main account: local id 1 with the same `sync_id`, the oldest stamp and the same dates on every device, so two
-- devices that both ran this migration already hold the same row. Every row that exists lands in it through the default of the new
-- column, with no write: no trigger fires and no stamp moves.
--
-- Rows follow the rules of 0008 to 0016: `accounts` and `transfers` have a random `sync_id` and a `stamp` made by triggers that stand
-- aside while `sync_clock.applying` is on, and a deleted row leaves a marker. The clock is read as 0016 reads it.

-- 4.1 The markers of hard-deleted rows get two more kinds. The table is rebuilt as 0012 did: the triggers that write to it go first and
-- come back with the bodies they had (0016; the short ones of recurring items and transactions follow their `_sync_unlink`, 0012 and 0013).
DROP TRIGGER trg_budgets_sync_delete;
DROP TRIGGER trg_recurring_transactions_sync_delete;
DROP TRIGGER trg_transactions_sync_delete;
DROP TRIGGER trg_documents_sync_delete;
DROP TRIGGER trg_document_links_sync_delete;
DROP TRIGGER trg_ledger_entries_sync_delete;

CREATE TABLE sync_deleted_new (
  id      INTEGER PRIMARY KEY,
  sync_id TEXT NOT NULL CONSTRAINT ck_sync_deleted_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  kind    TEXT NOT NULL CONSTRAINT ck_sync_deleted_kind CHECK (kind IN ('budgets', 'recurring_transactions', 'transactions', 'documents', 'document_links', 'ledger_entries', 'accounts', 'transfers')),
  stamp   TEXT NOT NULL
) STRICT;
INSERT INTO sync_deleted_new (id, sync_id, kind, stamp) SELECT id, sync_id, kind, stamp FROM sync_deleted;
DROP TABLE sync_deleted;
ALTER TABLE sync_deleted_new RENAME TO sync_deleted;
CREATE UNIQUE INDEX uq_sync_deleted_sync_id ON sync_deleted (sync_id);

CREATE TRIGGER trg_budgets_sync_delete
AFTER DELETE ON budgets
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0 AND OLD.sync_id IS NOT NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'budgets', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_recurring_transactions_sync_delete
AFTER DELETE ON recurring_transactions
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0 AND OLD.sync_id IS NOT NULL
BEGIN
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'recurring_transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_transactions_sync_delete
AFTER DELETE ON transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_documents_sync_delete
AFTER DELETE ON documents
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'documents', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_document_links_sync_delete
AFTER DELETE ON document_links
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND EXISTS (SELECT 1 FROM documents WHERE id = OLD.document_id)
  AND (OLD.transaction_id IS NULL OR EXISTS (SELECT 1 FROM transactions WHERE id = OLD.transaction_id))
  AND (OLD.recurring_transaction_id IS NULL OR EXISTS (SELECT 1 FROM recurring_transactions WHERE id = OLD.recurring_transaction_id))
  AND (OLD.ledger_entry_id IS NULL OR EXISTS (SELECT 1 FROM ledger_entries WHERE id = OLD.ledger_entry_id))
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'document_links', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_ledger_entries_sync_delete
AFTER DELETE ON ledger_entries
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'ledger_entries', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

-- 4.2 accounts ---------------------------------------------------------------
-- No unique index on the name: the name is checked on this device only (`accounts.rs`), and two devices that pick the same name keep two
-- accounts. `position` counts from 0 over all accounts; lists sort by `position, id`.
CREATE TABLE accounts (
  id                    INTEGER PRIMARY KEY,
  sync_id               TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_accounts_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp                 TEXT,
  name                  TEXT NOT NULL CONSTRAINT ck_accounts_name CHECK (length(name) BETWEEN 1 AND 50),
  kind                  TEXT NOT NULL CONSTRAINT ck_accounts_kind CHECK (kind IN ('CHECKING', 'SAVINGS', 'CASH', 'CREDIT_CARD')),
  number                TEXT CONSTRAINT ck_accounts_number CHECK (number IS NULL OR length(number) BETWEEN 1 AND 50),
  opening_balance_minor INTEGER NOT NULL DEFAULT 0
    CONSTRAINT ck_accounts_opening CHECK (opening_balance_minor BETWEEN -999999999999 AND 999999999999),
  opening_date          TEXT NOT NULL CONSTRAINT ck_accounts_opening_date
    CHECK (opening_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(opening_date) IS opening_date),
  archived              INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_accounts_archived CHECK (archived IN (0, 1)),
  position              INTEGER NOT NULL DEFAULT 0,
  created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  -- the main account is never archived, whoever writes (a sync and a restore too)
  CONSTRAINT ck_accounts_main_active CHECK (id <> 1 OR archived = 0)
) STRICT;
CREATE UNIQUE INDEX uq_accounts_sync_id ON accounts (sync_id);

-- 4.3 The main account, before any trigger of `accounts` exists: no stamp is made and no history row. Every field is a constant, so every
-- device holds the same row; the stamp is the oldest possible one, and the first real edit on any device wins.
INSERT INTO accounts (id, sync_id, stamp, name, kind, opening_balance_minor, opening_date, position, created_at, updated_at)
VALUES (1, '00000000000000000000000000000001', '0000000000000-0000-00000000', 'Main account', 'CHECKING', 0, '1970-01-01', 0,
        '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z');

-- 4.4 The account of a transaction, a recurring item and a ledger entry. SQLite allows a `NOT NULL DEFAULT 1 REFERENCES` column only while
-- foreign keys are off, which is how every migration runs (`migrate::apply`). The default stays: a row that names no account is in the
-- main account.
ALTER TABLE transactions           ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1 REFERENCES accounts (id) ON DELETE NO ACTION;
ALTER TABLE recurring_transactions ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1 REFERENCES accounts (id) ON DELETE NO ACTION;
ALTER TABLE ledger_entries         ADD COLUMN account_id INTEGER NOT NULL DEFAULT 1 REFERENCES accounts (id) ON DELETE NO ACTION;
CREATE INDEX idx_transactions_account ON transactions (account_id, payment_date);
CREATE INDEX idx_recurring_account ON recurring_transactions (account_id);
CREATE INDEX idx_ledger_account ON ledger_entries (account_id, entry_date);

-- A move to another account is a change of the row: the triggers that list the columns they watch are made again with `account_id` among
-- them, their bodies as 0017 (transactions, recurring items), 0012 and 0016 (ledger entries) left them. The audit triggers are left alone,
-- as in 0014 to 0017.
DROP TRIGGER trg_transactions_updated_at;
DROP TRIGGER trg_transactions_sync_update;
DROP TRIGGER trg_recurring_transactions_updated_at;
DROP TRIGGER trg_recurring_transactions_sync_update;
DROP TRIGGER trg_ledger_entries_updated_at;
DROP TRIGGER trg_ledger_entries_sync_update;

CREATE TRIGGER trg_transactions_updated_at
AFTER UPDATE OF id, category_id, recurring_due_date, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, paid_by, account_id, created_at, updated_at ON transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_sync_update
AFTER UPDATE OF category_id, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, paid_by, account_id ON transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_updated_at
AFTER UPDATE OF id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, skipped_dates, end_date, is_active, split_my_weight, split_paid_by, account_id, created_at, updated_at ON recurring_transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_sync_update
AFTER UPDATE OF category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, skipped_dates, end_date, is_active, split_my_weight, split_paid_by, account_id ON recurring_transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE recurring_transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_ledger_entries_updated_at
AFTER UPDATE OF id, person_id, kind, amount_minor, entry_date, note, origin_id, account_id, created_at, updated_at ON ledger_entries
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.person_id IS NOT OLD.person_id OR NEW.kind IS NOT OLD.kind OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.entry_date IS NOT OLD.entry_date OR NEW.note IS NOT OLD.note OR NEW.origin_id IS NOT OLD.origin_id OR NEW.account_id IS NOT OLD.account_id OR NEW.created_at IS NOT OLD.created_at OR NEW.transaction_id IS NOT OLD.transaction_id)
BEGIN
  UPDATE ledger_entries SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_ledger_entries_sync_update
AFTER UPDATE OF person_id, kind, amount_minor, entry_date, note, origin_id, account_id ON ledger_entries
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.person_id IS NOT OLD.person_id OR NEW.kind IS NOT OLD.kind OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.entry_date IS NOT OLD.entry_date OR NEW.note IS NOT OLD.note OR NEW.origin_id IS NOT OLD.origin_id OR NEW.account_id IS NOT OLD.account_id OR NEW.transaction_id IS NOT OLD.transaction_id)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE ledger_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- 4.5 transfers ---------------------------------------------------------------
-- Money moved between two accounts. Never income or expense, never in a budget or a statistic: nothing that adds up spending or earning
-- reads this table. A side may be an archived account.
CREATE TABLE transfers (
  id              INTEGER PRIMARY KEY,
  sync_id         TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_transfers_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp           TEXT,
  from_account_id INTEGER NOT NULL REFERENCES accounts (id) ON DELETE NO ACTION,
  to_account_id   INTEGER NOT NULL REFERENCES accounts (id) ON DELETE NO ACTION,
  amount_minor    INTEGER NOT NULL CONSTRAINT ck_transfers_amount CHECK (amount_minor BETWEEN 1 AND 999999999999),
  transfer_date   TEXT NOT NULL CONSTRAINT ck_transfers_date
    CHECK (transfer_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(transfer_date) IS transfer_date),
  note            TEXT CONSTRAINT ck_transfers_note CHECK (note IS NULL OR length(note) BETWEEN 1 AND 200),
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_transfers_two_accounts CHECK (from_account_id <> to_account_id)
) STRICT;
CREATE UNIQUE INDEX uq_transfers_sync_id ON transfers (sync_id);
CREATE INDEX idx_transfers_from ON transfers (from_account_id, transfer_date);
CREATE INDEX idx_transfers_to ON transfers (to_account_id, transfer_date);

-- 4.6 The triggers of the two tables, as those of `people` and `ledger_entries` (0012): `updated_at` and the stamp move on a real change
-- only, a deleted row leaves a marker, and the history keeps every column but the sync ones.

CREATE TRIGGER trg_accounts_updated_at
AFTER UPDATE OF id, name, kind, number, opening_balance_minor, opening_date, archived, position, created_at, updated_at ON accounts
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE accounts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_accounts_sync_insert
AFTER INSERT ON accounts
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE accounts SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_accounts_sync_update
AFTER UPDATE OF name, kind, number, opening_balance_minor, opening_date, archived, position ON accounts
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE accounts SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_accounts_sync_delete
AFTER DELETE ON accounts
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'accounts', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_accounts_audit_insert
AFTER INSERT ON accounts
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_update
AFTER UPDATE ON accounts
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.kind IS NOT NEW.kind OR OLD.number IS NOT NEW.number OR OLD.opening_balance_minor IS NOT NEW.opening_balance_minor OR OLD.opening_date IS NOT NEW.opening_date OR OLD.archived IS NOT NEW.archived OR OLD.position IS NOT NEW.position OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', NEW.id, 'UPDATE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_delete
AFTER DELETE ON accounts
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- The main account is never deleted. The trigger stands aside while applying, because a restore empties every table before it fills them;
-- a sync refuses a deletion of the main account itself. An account that still has rows is kept by the four foreign keys.
CREATE TRIGGER trg_accounts_keep_main
BEFORE DELETE ON accounts
WHEN OLD.id = 1 AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'the main account is never deleted');
END;

CREATE TRIGGER trg_transfers_updated_at
AFTER UPDATE OF id, from_account_id, to_account_id, amount_minor, transfer_date, note, created_at, updated_at ON transfers
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.from_account_id IS NOT OLD.from_account_id OR NEW.to_account_id IS NOT OLD.to_account_id OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.transfer_date IS NOT OLD.transfer_date OR NEW.note IS NOT OLD.note OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE transfers SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transfers_sync_insert
AFTER INSERT ON transfers
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transfers SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transfers_sync_update
AFTER UPDATE OF from_account_id, to_account_id, amount_minor, transfer_date, note ON transfers
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.from_account_id IS NOT OLD.from_account_id OR NEW.to_account_id IS NOT OLD.to_account_id OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.transfer_date IS NOT OLD.transfer_date OR NEW.note IS NOT OLD.note)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transfers SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transfers_sync_delete
AFTER DELETE ON transfers
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'transfers', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_transfers_audit_insert
AFTER INSERT ON transfers
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transfers', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'from_account_id', NEW.from_account_id, 'to_account_id', NEW.to_account_id, 'amount_minor', NEW.amount_minor, 'transfer_date', NEW.transfer_date, 'note', NEW.note, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transfers_audit_update
AFTER UPDATE ON transfers
WHEN OLD.id IS NOT NEW.id OR OLD.from_account_id IS NOT NEW.from_account_id OR OLD.to_account_id IS NOT NEW.to_account_id OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.transfer_date IS NOT NEW.transfer_date OR OLD.note IS NOT NEW.note OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transfers', NEW.id, 'UPDATE', json_object('id', OLD.id, 'from_account_id', OLD.from_account_id, 'to_account_id', OLD.to_account_id, 'amount_minor', OLD.amount_minor, 'transfer_date', OLD.transfer_date, 'note', OLD.note, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'from_account_id', NEW.from_account_id, 'to_account_id', NEW.to_account_id, 'amount_minor', NEW.amount_minor, 'transfer_date', NEW.transfer_date, 'note', NEW.note, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transfers_audit_delete
AFTER DELETE ON transfers
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transfers', OLD.id, 'DELETE', json_object('id', OLD.id, 'from_account_id', OLD.from_account_id, 'to_account_id', OLD.to_account_id, 'amount_minor', OLD.amount_minor, 'transfer_date', OLD.transfer_date, 'note', OLD.note, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- 4.7 What moved an account, one row per movement: the opening balance on its date, each transaction you paid or received (the whole bill
-- of a split one; a bill a friend paid moved nothing), each side of a transfer, and each ledger entry. A balance on a day is the sum of
-- `delta_minor` up to that day, and nothing else.
CREATE VIEW account_movements AS
SELECT a.id AS account_id, a.opening_date AS day, a.opening_balance_minor AS delta_minor, 'OPENING' AS source, a.id AS source_id
  FROM accounts a WHERE a.opening_balance_minor <> 0
UNION ALL
SELECT t.account_id, t.payment_date, CASE c.type WHEN 'INCOME' THEN t.amount_minor ELSE -t.amount_minor END, 'TRANSACTION', t.id
  FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.paid_by IS NULL
UNION ALL
SELECT x.to_account_id, x.transfer_date, x.amount_minor, 'TRANSFER_IN', x.id FROM transfers x
UNION ALL
SELECT x.from_account_id, x.transfer_date, -x.amount_minor, 'TRANSFER_OUT', x.id FROM transfers x
UNION ALL
SELECT e.account_id, e.entry_date,
       CASE e.kind WHEN 'BORROWED' THEN e.amount_minor WHEN 'REPAID_TO_ME' THEN e.amount_minor ELSE -e.amount_minor END, 'LEDGER', e.id
  FROM ledger_entries e;
