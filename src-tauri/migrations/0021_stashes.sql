-- Stashes (docs/stash-design.md, section 4).
--
-- A stash is money set aside inside one account for a goal: a holiday, a laptop, a rainy-day fund. The money never leaves the account; a stash
-- only says that part of the account's balance is set aside. Money goes in and out by entries (`stash_entries`: above 0 an add, below 0 a
-- take), and what a stash holds is the sum of its entries. Nothing that is saved is stored: saved, set aside, free, left and per month are
-- worked out when they are asked for (section 3), and the view `account_movements` does not change, so no balance, statement, chart or net
-- worth moves when money goes into or out of a stash.
--
-- Rows follow the rules of 0008 to 0020: `stashes` and `stash_entries` have a random `sync_id` and a `stamp` made by triggers that stand aside
-- while `sync_clock.applying` is on, and a deleted row leaves a marker. The clock is read as 0016 reads it. Both tables are new, so nothing
-- that exists is written: no trigger fires, no stamp or `updated_at` moves and no history row is made.
--
-- What the accounts gain (section 4.5: the stashes count in `entries`, and `setAside` and `free`) is read from these tables by the code of
-- accounts; it needs no change of the schema.

-- 4.1 The markers of hard-deleted rows get two more kinds. The table is rebuilt as 0018 did: the triggers that write to it go first and come
-- back with the bodies they had (0018 made the newest of every one of them; 0019 and 0020 left them alone). This is every trigger whose SQL
-- writes to `sync_deleted` at version 20.

DROP TRIGGER trg_budgets_sync_delete;
DROP TRIGGER trg_recurring_transactions_sync_delete;
DROP TRIGGER trg_transactions_sync_delete;
DROP TRIGGER trg_documents_sync_delete;
DROP TRIGGER trg_document_links_sync_delete;
DROP TRIGGER trg_ledger_entries_sync_delete;
DROP TRIGGER trg_accounts_sync_delete;
DROP TRIGGER trg_transfers_sync_delete;

CREATE TABLE sync_deleted_new (
  id      INTEGER PRIMARY KEY,
  sync_id TEXT NOT NULL CONSTRAINT ck_sync_deleted_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  kind    TEXT NOT NULL CONSTRAINT ck_sync_deleted_kind CHECK (kind IN ('budgets', 'recurring_transactions', 'transactions', 'documents', 'document_links', 'ledger_entries', 'accounts', 'transfers', 'stashes', 'stash_entries')),
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

-- The one body that is not as 0018 left it: the marker of a transfer is made with the stamp that `trg_transfers_sync_unlink` (below) ticked
-- before the delete, as the marker of a transaction is made with the tick of `trg_transactions_sync_unlink` (0013). Without that the entries
-- that lose their link would have the stamp of the tick before the delete and the marker the one of a second tick after it, and the two devices
-- would not end with the same stamp (section 7.4). Every delete that makes a marker runs the trigger before it, so the marker's stamp is the same
-- one a trigger that ticked itself would have made.
CREATE TRIGGER trg_transfers_sync_delete
AFTER DELETE ON transfers
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'transfers', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

-- 4.2 stashes -----------------------------------------------------------------
-- The account is always there: a stash lives in one account, chosen when it is made. An account with a stash is not deleted (the foreign key is
-- `NO ACTION`). No unique index on the name: the name is checked on this device only (`stashes.rs`), as for accounts, and two devices that pick
-- the same name keep two stashes. `position` counts from 0 over all stashes; lists sort by `position, id`.
CREATE TABLE stashes (
  id           INTEGER PRIMARY KEY,
  sync_id      TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_stashes_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp        TEXT,
  name         TEXT NOT NULL CONSTRAINT ck_stashes_name CHECK (length(name) BETWEEN 1 AND 50),
  account_id   INTEGER NOT NULL REFERENCES accounts (id) ON DELETE NO ACTION,
  target_minor INTEGER CONSTRAINT ck_stashes_target CHECK (target_minor IS NULL OR target_minor BETWEEN 1 AND 999999999999),
  target_date  TEXT CONSTRAINT ck_stashes_target_date
    CHECK (target_date IS NULL OR (target_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(target_date) IS target_date)),
  notes        TEXT CONSTRAINT ck_stashes_notes CHECK (notes IS NULL OR length(notes) BETWEEN 1 AND 1000),
  archived     INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_stashes_archived CHECK (archived IN (0, 1)),
  position     INTEGER NOT NULL DEFAULT 0,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX uq_stashes_sync_id ON stashes (sync_id);
CREATE INDEX idx_stashes_account ON stashes (account_id);

-- 4.3 stash_entries -----------------------------------------------------------
-- One add (above 0) or one take (below 0). The sign says what the entry is, and an entry never changes from one to the other (section 5.2).
-- The link to the transfer that came with it is cleared when the transfer is deleted (the money stays set aside); that is not an edit of the
-- entry, see `trg_transfers_sync_unlink` below.
CREATE TABLE stash_entries (
  id           INTEGER PRIMARY KEY,
  sync_id      TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_stash_entries_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp        TEXT,
  stash_id     INTEGER NOT NULL REFERENCES stashes (id) ON DELETE CASCADE,
  amount_minor INTEGER NOT NULL CONSTRAINT ck_stash_entries_amount
    CHECK (amount_minor <> 0 AND amount_minor BETWEEN -999999999999 AND 999999999999),
  entry_date   TEXT NOT NULL CONSTRAINT ck_stash_entries_date
    CHECK (entry_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(entry_date) IS entry_date),
  note         TEXT CONSTRAINT ck_stash_entries_note CHECK (note IS NULL OR length(note) BETWEEN 1 AND 200),
  transfer_id  INTEGER REFERENCES transfers (id) ON DELETE SET NULL,
  created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
CREATE UNIQUE INDEX uq_stash_entries_sync_id ON stash_entries (sync_id);
CREATE INDEX idx_stash_entries_stash ON stash_entries (stash_id, entry_date, id);
-- the foreign key action that clears the link looks entries up by transfer
CREATE INDEX idx_stash_entries_transfer ON stash_entries (transfer_id) WHERE transfer_id IS NOT NULL;

-- 4.4 The triggers of the two tables, as those of `transfers` (0018, 0020): `updated_at` and the stamp move on a real change only, a deleted
-- row leaves a marker, and the history keeps every column but the sync ones.
--
-- Two things differ from the table of section 4.4 and follow the ledger instead (`ledger_entries.transaction_id`, 0012, 0013):
-- * The two triggers that list the columns they watch (`_updated_at`, `_sync_update`) leave `transfer_id` out of their `UPDATE OF` list and keep
--   it in the `WHEN` test, as the ledger's do with `transaction_id`. The `SET NULL` that clears the link is then not an edit: the entry has its
--   one new stamp from `trg_transfers_sync_unlink`, `updated_at` does not move (a sync clears the link with the triggers aside and does not
--   move it either, so both devices end equal), and the history gets its one row from `_audit_update`, which has no list. The code that edits an
--   entry writes `amount_minor`, `entry_date` and `note`, which are in the list.
-- * `trg_stash_entries_sync_delete` makes a marker only while the stash is still there. An entry removed by the cascade of a stash's deletion
--   leaves none: the marker of the stash covers it, and the other device cascades too (as `trg_document_links_sync_delete`, 0011).


CREATE TRIGGER trg_stashes_updated_at
AFTER UPDATE OF id, name, account_id, target_minor, target_date, notes, archived, position, created_at, updated_at ON stashes
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.account_id IS NOT OLD.account_id OR NEW.target_minor IS NOT OLD.target_minor OR NEW.target_date IS NOT OLD.target_date OR NEW.notes IS NOT OLD.notes OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE stashes SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stashes_sync_insert
AFTER INSERT ON stashes
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE stashes SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stashes_sync_update
AFTER UPDATE OF name, account_id, target_minor, target_date, notes, archived, position ON stashes
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.account_id IS NOT OLD.account_id OR NEW.target_minor IS NOT OLD.target_minor OR NEW.target_date IS NOT OLD.target_date OR NEW.notes IS NOT OLD.notes OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE stashes SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stashes_sync_delete
AFTER DELETE ON stashes
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'stashes', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_stashes_audit_insert
AFTER INSERT ON stashes
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stashes', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'account_id', NEW.account_id, 'target_minor', NEW.target_minor, 'target_date', NEW.target_date, 'notes', NEW.notes, 'archived', NEW.archived, 'position', NEW.position, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_stashes_audit_update
AFTER UPDATE ON stashes
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.account_id IS NOT NEW.account_id OR OLD.target_minor IS NOT NEW.target_minor OR OLD.target_date IS NOT NEW.target_date OR OLD.notes IS NOT NEW.notes OR OLD.archived IS NOT NEW.archived OR OLD.position IS NOT NEW.position OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stashes', NEW.id, 'UPDATE', json_object('id', OLD.id, 'name', OLD.name, 'account_id', OLD.account_id, 'target_minor', OLD.target_minor, 'target_date', OLD.target_date, 'notes', OLD.notes, 'archived', OLD.archived, 'position', OLD.position, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'name', NEW.name, 'account_id', NEW.account_id, 'target_minor', NEW.target_minor, 'target_date', NEW.target_date, 'notes', NEW.notes, 'archived', NEW.archived, 'position', NEW.position, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_stashes_audit_delete
AFTER DELETE ON stashes
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stashes', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'account_id', OLD.account_id, 'target_minor', OLD.target_minor, 'target_date', OLD.target_date, 'notes', OLD.notes, 'archived', OLD.archived, 'position', OLD.position, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

CREATE TRIGGER trg_stash_entries_updated_at
AFTER UPDATE OF id, stash_id, amount_minor, entry_date, note, created_at, updated_at ON stash_entries
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.stash_id IS NOT OLD.stash_id OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.entry_date IS NOT OLD.entry_date OR NEW.note IS NOT OLD.note OR NEW.transfer_id IS NOT OLD.transfer_id OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE stash_entries SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stash_entries_sync_insert
AFTER INSERT ON stash_entries
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE stash_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stash_entries_sync_update
AFTER UPDATE OF stash_id, amount_minor, entry_date, note ON stash_entries
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.stash_id IS NOT OLD.stash_id OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.entry_date IS NOT OLD.entry_date OR NEW.note IS NOT OLD.note OR NEW.transfer_id IS NOT OLD.transfer_id)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE stash_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stash_entries_sync_delete
AFTER DELETE ON stash_entries
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND EXISTS (SELECT 1 FROM stashes WHERE id = OLD.stash_id)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'stash_entries', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_stash_entries_audit_insert
AFTER INSERT ON stash_entries
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stash_entries', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'stash_id', NEW.stash_id, 'amount_minor', NEW.amount_minor, 'entry_date', NEW.entry_date, 'note', NEW.note, 'transfer_id', NEW.transfer_id, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_stash_entries_audit_update
AFTER UPDATE ON stash_entries
WHEN OLD.id IS NOT NEW.id OR OLD.stash_id IS NOT NEW.stash_id OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.entry_date IS NOT NEW.entry_date OR OLD.note IS NOT NEW.note OR OLD.transfer_id IS NOT NEW.transfer_id OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stash_entries', NEW.id, 'UPDATE', json_object('id', OLD.id, 'stash_id', OLD.stash_id, 'amount_minor', OLD.amount_minor, 'entry_date', OLD.entry_date, 'note', OLD.note, 'transfer_id', OLD.transfer_id, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'stash_id', NEW.stash_id, 'amount_minor', NEW.amount_minor, 'entry_date', NEW.entry_date, 'note', NEW.note, 'transfer_id', NEW.transfer_id, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_stash_entries_audit_delete
AFTER DELETE ON stash_entries
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stash_entries', OLD.id, 'DELETE', json_object('id', OLD.id, 'stash_id', OLD.stash_id, 'amount_minor', OLD.amount_minor, 'entry_date', OLD.entry_date, 'note', OLD.note, 'transfer_id', OLD.transfer_id, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- The entries that came with a transfer lose the link when it is deleted (`ON DELETE SET NULL`). They get a stamp first, so the other device
-- learns that the link is gone: the trigger before the delete ticks the clock and stamps them, and `trg_transfers_sync_delete` makes the marker
-- with the same stamp (one tick for both, as `trg_transactions_sync_unlink` and the ledger entries, 0013). A device that learns of the deletion
-- from a sync does the same from its end (section 7.4), so the trigger stands aside while applying.
CREATE TRIGGER trg_transfers_sync_unlink
BEFORE DELETE ON transfers
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE stash_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE transfer_id = OLD.id;
END;
