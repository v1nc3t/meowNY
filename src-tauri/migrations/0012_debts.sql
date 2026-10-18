-- Debts and expense splitting (phase 10, see docs/debts-design.md).
--
-- Four tables: `people` (the friends of the ledger: a name and nothing else), `ledger_entries` (a loan, a borrowing or a repayment
-- with one friend), `transaction_split_lines` (the parts of a bill that are not yours) and `recurring_split_lines` (the proportions
-- of a recurring item's split, copied into each of its payments). Two views give the numbers everything else uses: `transaction_costs`
-- (what a transaction costs YOU: its amount minus the other people's parts) and `person_balances` (who owes whom, and how much).
--
-- Your own part of a bill is never stored. It is the amount minus the lines, so no rounding, sync or currency conversion can make the
-- parts disagree with the total.
--
-- Rows follow the rules of 0008 to 0011: `people` and `ledger_entries` have a random `sync_id` and a `stamp` made by triggers that
-- stand aside while `sync_clock.applying` is on, and a ledger entry leaves a deletion marker. A person is archived, never deleted. The
-- lines have neither stamp nor marker: they belong to their transaction (or recurring item), and a change to a line gives the parent
-- a new stamp, so the next sync carries the whole bill. The checks of the parts against the bill stand aside while a sync, a restore
-- or a currency conversion is writing (they write the pieces in some order); `splits::check` looks at the finished state.

-- The markers of hard-deleted rows get one more kind. SQLite cannot loosen a CHECK in place, so the table is rebuilt, and the triggers
-- that write to it go first and come back below, because a table that triggers name must not be dropped from under them.
DROP TRIGGER trg_budgets_sync_delete;
DROP TRIGGER trg_recurring_transactions_sync_delete;
DROP TRIGGER trg_transactions_sync_delete;
DROP TRIGGER trg_documents_sync_delete;
DROP TRIGGER trg_document_links_sync_delete;

CREATE TABLE sync_deleted_new (
  id      INTEGER PRIMARY KEY,
  sync_id TEXT NOT NULL CONSTRAINT ck_sync_deleted_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  kind    TEXT NOT NULL CONSTRAINT ck_sync_deleted_kind CHECK (kind IN ('budgets', 'recurring_transactions', 'transactions', 'documents', 'document_links', 'ledger_entries')),
  stamp   TEXT NOT NULL
) STRICT;
INSERT INTO sync_deleted_new (id, sync_id, kind, stamp) SELECT id, sync_id, kind, stamp FROM sync_deleted;
DROP TABLE sync_deleted;
ALTER TABLE sync_deleted_new RENAME TO sync_deleted;
CREATE UNIQUE INDEX uq_sync_deleted_sync_id ON sync_deleted (sync_id);

-- The four triggers that were dropped, as 0010 and 0009 made them. (The one for links comes back below with the rebuilt table.)

CREATE TRIGGER trg_budgets_sync_delete
AFTER DELETE ON budgets
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0 AND OLD.sync_id IS NOT NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
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
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0 AND OLD.sync_id IS NOT NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_documents_sync_delete
AFTER DELETE ON documents
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'documents', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

-- people ---------------------------------------------------------------
-- A friend is a name. Nothing else about a person is stored: no phone number, email or address. Archived, never deleted: the balance
-- and the history stay. Names are unique among the people who are not archived, ignoring case (SQLite folds ASCII letters only).
CREATE TABLE people (
  id                INTEGER PRIMARY KEY,
  sync_id           TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_people_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp             TEXT,
  name              TEXT NOT NULL CONSTRAINT ck_people_name CHECK (length(name) BETWEEN 1 AND 50),
  archived          INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_people_archived CHECK (archived IN (0, 1)),
  -- the sync id of the person this one was merged into when two devices each added the same friend (set by a sync)
  merged_into       TEXT CONSTRAINT ck_people_merged CHECK (merged_into IS NULL OR (length(merged_into) = 32 AND merged_into NOT GLOB '*[^0-9a-f]*')),
  remind_on         TEXT CONSTRAINT ck_people_remind_on
    CHECK (remind_on IS NULL OR (remind_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(remind_on) IS remind_on)),
  remind_every_days INTEGER CONSTRAINT ck_people_remind_every CHECK (remind_every_days IS NULL OR remind_every_days BETWEEN 1 AND 365),
  created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_people_repeat_needs_date CHECK (remind_every_days IS NULL OR remind_on IS NOT NULL)
) STRICT;

CREATE UNIQUE INDEX uq_people_sync_id ON people (sync_id);
CREATE UNIQUE INDEX uq_people_active_name ON people (name COLLATE NOCASE) WHERE archived = 0;

CREATE TRIGGER trg_people_never_deleted
BEFORE DELETE ON people
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'a person is archived, never deleted');
END;

CREATE TRIGGER trg_people_updated_at
AFTER UPDATE OF id, name, archived, merged_into, remind_on, remind_every_days, created_at, updated_at ON people
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.archived IS NOT OLD.archived OR NEW.merged_into IS NOT OLD.merged_into OR NEW.remind_on IS NOT OLD.remind_on OR NEW.remind_every_days IS NOT OLD.remind_every_days OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE people SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_people_sync_insert
AFTER INSERT ON people
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE people SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_people_sync_update
AFTER UPDATE OF name, archived, merged_into, remind_on, remind_every_days ON people
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.archived IS NOT OLD.archived OR NEW.merged_into IS NOT OLD.merged_into OR NEW.remind_on IS NOT OLD.remind_on OR NEW.remind_every_days IS NOT OLD.remind_every_days)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE people SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_people_audit_insert
AFTER INSERT ON people
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('people', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'archived', NEW.archived, 'merged_into', NEW.merged_into, 'remind_on', NEW.remind_on, 'remind_every_days', NEW.remind_every_days, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_people_audit_update
AFTER UPDATE ON people
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.archived IS NOT NEW.archived OR OLD.merged_into IS NOT NEW.merged_into OR OLD.remind_on IS NOT NEW.remind_on OR OLD.remind_every_days IS NOT NEW.remind_every_days OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('people', NEW.id, 'UPDATE', json_object('id', OLD.id, 'name', OLD.name, 'archived', OLD.archived, 'merged_into', OLD.merged_into, 'remind_on', OLD.remind_on, 'remind_every_days', OLD.remind_every_days, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'name', NEW.name, 'archived', NEW.archived, 'merged_into', NEW.merged_into, 'remind_on', NEW.remind_on, 'remind_every_days', NEW.remind_every_days, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_people_audit_delete
AFTER DELETE ON people
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('people', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'archived', OLD.archived, 'merged_into', OLD.merged_into, 'remind_on', OLD.remind_on, 'remind_every_days', OLD.remind_every_days, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- ledger_entries ---------------------------------------------------------------
-- LENT: I lent them money. BORROWED: I borrowed from them. REPAID_TO_ME: they paid me back. REPAID_BY_ME: I paid them back.
-- Never income, never in a budget: nothing that adds up income or spending reads this table. The link to a transaction is cleared when
-- the transaction goes (the debt outlives the expense), and that is not an edit of the entry, so no stamp (as 0009 did for the link
-- from a transaction to its recurring item); the stamp triggers do not watch `transaction_id`, and the code that edits an entry
-- writes every column, which does.
CREATE TABLE ledger_entries (
  id             INTEGER PRIMARY KEY,
  sync_id        TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_ledger_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp          TEXT,
  person_id      INTEGER NOT NULL REFERENCES people (id) ON DELETE NO ACTION,
  kind           TEXT NOT NULL CONSTRAINT ck_ledger_kind CHECK (kind IN ('LENT', 'BORROWED', 'REPAID_TO_ME', 'REPAID_BY_ME')),
  amount_minor   INTEGER NOT NULL CONSTRAINT ck_ledger_amount CHECK (amount_minor BETWEEN 1 AND 999999999999),
  entry_date     TEXT NOT NULL CONSTRAINT ck_ledger_date
    CHECK (entry_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(entry_date) IS entry_date),
  note           TEXT CONSTRAINT ck_ledger_note CHECK (note IS NULL OR length(note) BETWEEN 1 AND 200),
  transaction_id INTEGER REFERENCES transactions (id) ON DELETE SET NULL,
  -- the bill code this entry came from (phase 10, batch 3); a code is accepted once
  origin_id      TEXT CONSTRAINT ck_ledger_origin CHECK (origin_id IS NULL OR (length(origin_id) = 32 AND origin_id NOT GLOB '*[^0-9a-f]*')),
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE UNIQUE INDEX uq_ledger_sync_id ON ledger_entries (sync_id);
CREATE UNIQUE INDEX uq_ledger_origin ON ledger_entries (origin_id) WHERE origin_id IS NOT NULL;
CREATE INDEX idx_ledger_person ON ledger_entries (person_id, entry_date, id);
-- the foreign key action that clears the link looks entries up by transaction
CREATE INDEX idx_ledger_transaction ON ledger_entries (transaction_id) WHERE transaction_id IS NOT NULL;

CREATE TRIGGER trg_ledger_entries_updated_at
AFTER UPDATE OF id, person_id, kind, amount_minor, entry_date, note, origin_id, created_at, updated_at ON ledger_entries
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.person_id IS NOT OLD.person_id OR NEW.kind IS NOT OLD.kind OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.entry_date IS NOT OLD.entry_date OR NEW.note IS NOT OLD.note OR NEW.origin_id IS NOT OLD.origin_id OR NEW.created_at IS NOT OLD.created_at OR NEW.transaction_id IS NOT OLD.transaction_id)
BEGIN
  UPDATE ledger_entries SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_ledger_entries_sync_insert
AFTER INSERT ON ledger_entries
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE ledger_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_ledger_entries_sync_update
AFTER UPDATE OF person_id, kind, amount_minor, entry_date, note, origin_id ON ledger_entries
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.person_id IS NOT OLD.person_id OR NEW.kind IS NOT OLD.kind OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.entry_date IS NOT OLD.entry_date OR NEW.note IS NOT OLD.note OR NEW.origin_id IS NOT OLD.origin_id OR NEW.transaction_id IS NOT OLD.transaction_id)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE ledger_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_ledger_entries_sync_delete
AFTER DELETE ON ledger_entries
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'ledger_entries', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_ledger_entries_audit_insert
AFTER INSERT ON ledger_entries
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('ledger_entries', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'person_id', NEW.person_id, 'kind', NEW.kind, 'amount_minor', NEW.amount_minor, 'entry_date', NEW.entry_date, 'note', NEW.note, 'transaction_id', NEW.transaction_id, 'origin_id', NEW.origin_id, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_ledger_entries_audit_update
AFTER UPDATE ON ledger_entries
WHEN OLD.id IS NOT NEW.id OR OLD.person_id IS NOT NEW.person_id OR OLD.kind IS NOT NEW.kind OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.entry_date IS NOT NEW.entry_date OR OLD.note IS NOT NEW.note OR OLD.transaction_id IS NOT NEW.transaction_id OR OLD.origin_id IS NOT NEW.origin_id OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('ledger_entries', NEW.id, 'UPDATE', json_object('id', OLD.id, 'person_id', OLD.person_id, 'kind', OLD.kind, 'amount_minor', OLD.amount_minor, 'entry_date', OLD.entry_date, 'note', OLD.note, 'transaction_id', OLD.transaction_id, 'origin_id', OLD.origin_id, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'person_id', NEW.person_id, 'kind', NEW.kind, 'amount_minor', NEW.amount_minor, 'entry_date', NEW.entry_date, 'note', NEW.note, 'transaction_id', NEW.transaction_id, 'origin_id', NEW.origin_id, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_ledger_entries_audit_delete
AFTER DELETE ON ledger_entries
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('ledger_entries', OLD.id, 'DELETE', json_object('id', OLD.id, 'person_id', OLD.person_id, 'kind', OLD.kind, 'amount_minor', OLD.amount_minor, 'entry_date', OLD.entry_date, 'note', OLD.note, 'transaction_id', OLD.transaction_id, 'origin_id', OLD.origin_id, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- transaction_split_lines ---------------------------------------------------------------
-- The parts of a bill that are not yours: a friend's, or a guest's (a name typed for someone who is not tracked; their part leaves your
-- cost and makes no debt). One line for each person and each guest name on a bill, at most 30 lines.
CREATE TABLE transaction_split_lines (
  id             INTEGER PRIMARY KEY,
  transaction_id INTEGER NOT NULL REFERENCES transactions (id) ON DELETE CASCADE,
  person_id      INTEGER REFERENCES people (id) ON DELETE NO ACTION,
  guest_name     TEXT CONSTRAINT ck_split_guest CHECK (guest_name IS NULL OR length(guest_name) BETWEEN 1 AND 50),
  amount_minor   INTEGER NOT NULL CONSTRAINT ck_split_amount CHECK (amount_minor BETWEEN 1 AND 999999999999),
  CONSTRAINT ck_split_one_who CHECK ((person_id IS NOT NULL) + (guest_name IS NOT NULL) = 1)
) STRICT;

-- with the amount in it, so that the cost of a transaction is worked out from the index alone
CREATE INDEX idx_split_lines_transaction ON transaction_split_lines (transaction_id, amount_minor);
CREATE INDEX idx_split_lines_person ON transaction_split_lines (person_id);
CREATE UNIQUE INDEX uq_split_lines_person ON transaction_split_lines (transaction_id, person_id) WHERE person_id IS NOT NULL;
CREATE UNIQUE INDEX uq_split_lines_guest ON transaction_split_lines (transaction_id, guest_name COLLATE NOCASE) WHERE guest_name IS NOT NULL;

-- A line is for an expense, there are at most 30 of them, and together they are never more than the bill.
CREATE TRIGGER trg_split_lines_insert_checks
BEFORE INSERT ON transaction_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'a bill has at most 30 parts')
    WHERE (SELECT count(*) FROM transaction_split_lines WHERE transaction_id = NEW.transaction_id) >= 30;
  SELECT RAISE(ABORT, 'only an expense can be split')
    WHERE (SELECT c.type FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.id = NEW.transaction_id) = 'INCOME';
  SELECT RAISE(ABORT, 'the parts of a bill cannot be more than the bill')
    WHERE (SELECT COALESCE(SUM(amount_minor), 0) FROM transaction_split_lines WHERE transaction_id = NEW.transaction_id) + NEW.amount_minor
        > (SELECT amount_minor FROM transactions WHERE id = NEW.transaction_id);
END;

CREATE TRIGGER trg_split_lines_update_checks
BEFORE UPDATE ON transaction_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'a part keeps its bill and its person')
    WHERE NEW.transaction_id IS NOT OLD.transaction_id OR NEW.person_id IS NOT OLD.person_id OR NEW.guest_name IS NOT OLD.guest_name;
  SELECT RAISE(ABORT, 'the parts of a bill cannot be more than the bill')
    WHERE (SELECT COALESCE(SUM(amount_minor), 0) FROM transaction_split_lines WHERE transaction_id = NEW.transaction_id AND id <> NEW.id) + NEW.amount_minor
        > (SELECT amount_minor FROM transactions WHERE id = NEW.transaction_id);
END;

-- The bill cannot shrink below its parts, and a split bill stays an expense.
CREATE TRIGGER trg_transactions_split_amount
BEFORE UPDATE OF amount_minor ON transactions
WHEN NEW.amount_minor < OLD.amount_minor AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND NEW.amount_minor < (SELECT COALESCE(SUM(amount_minor), 0) FROM transaction_split_lines WHERE transaction_id = NEW.id)
BEGIN
  SELECT RAISE(ABORT, 'the parts of a bill cannot be more than the bill');
END;

CREATE TRIGGER trg_transactions_split_category
BEFORE UPDATE OF category_id ON transactions
WHEN NEW.category_id IS NOT OLD.category_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND EXISTS (SELECT 1 FROM transaction_split_lines WHERE transaction_id = NEW.id)
  AND (SELECT type FROM categories WHERE id = NEW.category_id) = 'INCOME'
BEGIN
  SELECT RAISE(ABORT, 'only an expense can be split');
END;

-- A change to a line is a change to its bill: the parent gets a new stamp and updated_at (a write that names a column and gives it the value
-- it has still fires the triggers of 0008 and 0011 that watch it, which is what is wanted here), so the next sync carries the whole bill.
CREATE TRIGGER trg_split_lines_touch_insert
AFTER INSERT ON transaction_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE transactions SET name = name WHERE id = NEW.transaction_id;
END;

CREATE TRIGGER trg_split_lines_touch_update
AFTER UPDATE ON transaction_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE transactions SET name = name WHERE id = NEW.transaction_id;
END;

CREATE TRIGGER trg_split_lines_touch_delete
AFTER DELETE ON transaction_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE transactions SET name = name WHERE id = OLD.transaction_id;
END;

CREATE TRIGGER trg_transaction_split_lines_audit_insert
AFTER INSERT ON transaction_split_lines
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transaction_split_lines', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'transaction_id', NEW.transaction_id, 'person_id', NEW.person_id, 'guest_name', NEW.guest_name, 'amount_minor', NEW.amount_minor));
END;

CREATE TRIGGER trg_transaction_split_lines_audit_update
AFTER UPDATE ON transaction_split_lines
WHEN OLD.id IS NOT NEW.id OR OLD.transaction_id IS NOT NEW.transaction_id OR OLD.person_id IS NOT NEW.person_id OR OLD.guest_name IS NOT NEW.guest_name OR OLD.amount_minor IS NOT NEW.amount_minor
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transaction_split_lines', NEW.id, 'UPDATE', json_object('id', OLD.id, 'transaction_id', OLD.transaction_id, 'person_id', OLD.person_id, 'guest_name', OLD.guest_name, 'amount_minor', OLD.amount_minor), json_object('id', NEW.id, 'transaction_id', NEW.transaction_id, 'person_id', NEW.person_id, 'guest_name', NEW.guest_name, 'amount_minor', NEW.amount_minor));
END;

CREATE TRIGGER trg_transaction_split_lines_audit_delete
AFTER DELETE ON transaction_split_lines
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transaction_split_lines', OLD.id, 'DELETE', json_object('id', OLD.id, 'transaction_id', OLD.transaction_id, 'person_id', OLD.person_id, 'guest_name', OLD.guest_name, 'amount_minor', OLD.amount_minor), NULL);
END;

-- recurring_split_lines ---------------------------------------------------------------
-- The split of a recurring item is kept as weights, not amounts: a payment can differ from the plan (an electricity bill) and an item can
-- have no fixed amount. `split_my_weight` is yours; NULL means the item is not split. Each payment gets lines worked out from these
-- (`splits::allocate`), after which it is an ordinary split transaction. Editing the template changes the payments to come, never the ones made.
ALTER TABLE recurring_transactions ADD COLUMN split_my_weight INTEGER
  CONSTRAINT ck_rec_split_weight CHECK (split_my_weight IS NULL OR split_my_weight BETWEEN 0 AND 999999999999);

CREATE TABLE recurring_split_lines (
  id                       INTEGER PRIMARY KEY,
  recurring_transaction_id INTEGER NOT NULL REFERENCES recurring_transactions (id) ON DELETE CASCADE,
  person_id                INTEGER REFERENCES people (id) ON DELETE NO ACTION,
  guest_name               TEXT CONSTRAINT ck_rec_split_guest CHECK (guest_name IS NULL OR length(guest_name) BETWEEN 1 AND 50),
  weight                   INTEGER NOT NULL CONSTRAINT ck_rec_split_line_weight CHECK (weight BETWEEN 1 AND 999999999999),
  CONSTRAINT ck_rec_split_one_who CHECK ((person_id IS NOT NULL) + (guest_name IS NOT NULL) = 1)
) STRICT;

CREATE INDEX idx_rec_split_lines_recurring ON recurring_split_lines (recurring_transaction_id);
CREATE INDEX idx_rec_split_lines_person ON recurring_split_lines (person_id);
CREATE UNIQUE INDEX uq_rec_split_lines_person ON recurring_split_lines (recurring_transaction_id, person_id) WHERE person_id IS NOT NULL;
CREATE UNIQUE INDEX uq_rec_split_lines_guest ON recurring_split_lines (recurring_transaction_id, guest_name COLLATE NOCASE) WHERE guest_name IS NOT NULL;

CREATE TRIGGER trg_rec_split_lines_insert_checks
BEFORE INSERT ON recurring_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'a template has at most 30 parts')
    WHERE (SELECT count(*) FROM recurring_split_lines WHERE recurring_transaction_id = NEW.recurring_transaction_id) >= 30;
  SELECT RAISE(ABORT, 'a template needs the weight of the person who pays')
    WHERE (SELECT split_my_weight FROM recurring_transactions WHERE id = NEW.recurring_transaction_id) IS NULL;
  SELECT RAISE(ABORT, 'only an expense can be split')
    WHERE (SELECT c.type FROM recurring_transactions r JOIN categories c ON c.id = r.category_id WHERE r.id = NEW.recurring_transaction_id) = 'INCOME';
END;

CREATE TRIGGER trg_rec_split_lines_update_checks
BEFORE UPDATE ON recurring_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'a part keeps its item and its person')
    WHERE NEW.recurring_transaction_id IS NOT OLD.recurring_transaction_id OR NEW.person_id IS NOT OLD.person_id OR NEW.guest_name IS NOT OLD.guest_name;
END;

-- A template stays an expense, and its weight is not removed while it has lines.
CREATE TRIGGER trg_recurring_split_category
BEFORE UPDATE OF category_id ON recurring_transactions
WHEN NEW.category_id IS NOT OLD.category_id AND NEW.split_my_weight IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (SELECT type FROM categories WHERE id = NEW.category_id) = 'INCOME'
BEGIN
  SELECT RAISE(ABORT, 'only an expense can be split');
END;

CREATE TRIGGER trg_recurring_split_weight
BEFORE UPDATE OF split_my_weight ON recurring_transactions
WHEN NEW.split_my_weight IS NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND EXISTS (SELECT 1 FROM recurring_split_lines WHERE recurring_transaction_id = NEW.id)
BEGIN
  SELECT RAISE(ABORT, 'remove the parts of a template first');
END;

CREATE TRIGGER trg_rec_split_lines_touch_insert
AFTER INSERT ON recurring_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET name = name WHERE id = NEW.recurring_transaction_id;
END;

CREATE TRIGGER trg_rec_split_lines_touch_update
AFTER UPDATE ON recurring_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET name = name WHERE id = NEW.recurring_transaction_id;
END;

CREATE TRIGGER trg_rec_split_lines_touch_delete
AFTER DELETE ON recurring_split_lines
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET name = name WHERE id = OLD.recurring_transaction_id;
END;

CREATE TRIGGER trg_recurring_split_lines_audit_insert
AFTER INSERT ON recurring_split_lines
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_split_lines', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'person_id', NEW.person_id, 'guest_name', NEW.guest_name, 'weight', NEW.weight));
END;

CREATE TRIGGER trg_recurring_split_lines_audit_update
AFTER UPDATE ON recurring_split_lines
WHEN OLD.id IS NOT NEW.id OR OLD.recurring_transaction_id IS NOT NEW.recurring_transaction_id OR OLD.person_id IS NOT NEW.person_id OR OLD.guest_name IS NOT NEW.guest_name OR OLD.weight IS NOT NEW.weight
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_split_lines', NEW.id, 'UPDATE', json_object('id', OLD.id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'person_id', OLD.person_id, 'guest_name', OLD.guest_name, 'weight', OLD.weight), json_object('id', NEW.id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'person_id', NEW.person_id, 'guest_name', NEW.guest_name, 'weight', NEW.weight));
END;

CREATE TRIGGER trg_recurring_split_lines_audit_delete
AFTER DELETE ON recurring_split_lines
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_split_lines', OLD.id, 'DELETE', json_object('id', OLD.id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'person_id', OLD.person_id, 'guest_name', OLD.guest_name, 'weight', OLD.weight), NULL);
END;

-- The triggers of recurring_transactions that list its columns are made again, now with `split_my_weight`: the stamp (0008), updated_at (0009) and
-- the history (0006). The rest is as it was.
DROP TRIGGER trg_recurring_transactions_updated_at;
DROP TRIGGER trg_recurring_transactions_sync_update;
DROP TRIGGER trg_recurring_transactions_audit_insert;
DROP TRIGGER trg_recurring_transactions_audit_update;
DROP TRIGGER trg_recurring_transactions_audit_delete;

CREATE TRIGGER trg_recurring_transactions_updated_at
AFTER UPDATE OF id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date, is_active, split_my_weight, created_at, updated_at ON recurring_transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_sync_update
AFTER UPDATE OF category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date, is_active, split_my_weight ON recurring_transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE recurring_transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_audit_insert
AFTER INSERT ON recurring_transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_transactions', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'category_id', NEW.category_id, 'name', NEW.name, 'description', NEW.description, 'amount_minor', NEW.amount_minor, 'start_date', NEW.start_date, 'interval_unit', NEW.interval_unit, 'interval_count', NEW.interval_count, 'next_due_date', NEW.next_due_date, 'end_date', NEW.end_date, 'is_active', NEW.is_active, 'split_my_weight', NEW.split_my_weight, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_recurring_transactions_audit_update
AFTER UPDATE ON recurring_transactions
WHEN OLD.id IS NOT NEW.id OR OLD.category_id IS NOT NEW.category_id OR OLD.name IS NOT NEW.name OR OLD.description IS NOT NEW.description OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.start_date IS NOT NEW.start_date OR OLD.interval_unit IS NOT NEW.interval_unit OR OLD.interval_count IS NOT NEW.interval_count OR OLD.next_due_date IS NOT NEW.next_due_date OR OLD.end_date IS NOT NEW.end_date OR OLD.is_active IS NOT NEW.is_active OR OLD.split_my_weight IS NOT NEW.split_my_weight OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_transactions', NEW.id, 'UPDATE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'name', OLD.name, 'description', OLD.description, 'amount_minor', OLD.amount_minor, 'start_date', OLD.start_date, 'interval_unit', OLD.interval_unit, 'interval_count', OLD.interval_count, 'next_due_date', OLD.next_due_date, 'end_date', OLD.end_date, 'is_active', OLD.is_active, 'split_my_weight', OLD.split_my_weight, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'category_id', NEW.category_id, 'name', NEW.name, 'description', NEW.description, 'amount_minor', NEW.amount_minor, 'start_date', NEW.start_date, 'interval_unit', NEW.interval_unit, 'interval_count', NEW.interval_count, 'next_due_date', NEW.next_due_date, 'end_date', NEW.end_date, 'is_active', NEW.is_active, 'split_my_weight', NEW.split_my_weight, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_recurring_transactions_audit_delete
AFTER DELETE ON recurring_transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_transactions', OLD.id, 'DELETE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'name', OLD.name, 'description', OLD.description, 'amount_minor', OLD.amount_minor, 'start_date', OLD.start_date, 'interval_unit', OLD.interval_unit, 'interval_count', OLD.interval_count, 'next_due_date', OLD.next_due_date, 'end_date', OLD.end_date, 'is_active', OLD.is_active, 'split_my_weight', OLD.split_my_weight, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- document_links ---------------------------------------------------------------
-- A third target: a ledger entry (a loan note, an invoice). The table is rebuilt (the check on the targets changes), which drops its
-- triggers; they come back below as 0010 and 0011 made them, now knowing the third target.
CREATE TABLE document_links_new (
  id                       INTEGER PRIMARY KEY,
  sync_id                  TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_document_links_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp                    TEXT,
  document_id              INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
  transaction_id           INTEGER REFERENCES transactions (id) ON DELETE CASCADE,
  recurring_transaction_id INTEGER REFERENCES recurring_transactions (id) ON DELETE CASCADE,
  ledger_entry_id          INTEGER REFERENCES ledger_entries (id) ON DELETE CASCADE,
  created_at               TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_document_links_one_target CHECK ((transaction_id IS NOT NULL) + (recurring_transaction_id IS NOT NULL) + (ledger_entry_id IS NOT NULL) = 1)
) STRICT;
INSERT INTO document_links_new (id, sync_id, stamp, document_id, transaction_id, recurring_transaction_id, created_at)
  SELECT id, sync_id, stamp, document_id, transaction_id, recurring_transaction_id, created_at FROM document_links;
DROP TABLE document_links;
ALTER TABLE document_links_new RENAME TO document_links;

CREATE UNIQUE INDEX uq_document_links_sync_id ON document_links (sync_id);
CREATE UNIQUE INDEX uq_document_links_transaction ON document_links (document_id, transaction_id) WHERE transaction_id IS NOT NULL;
CREATE UNIQUE INDEX uq_document_links_recurring ON document_links (document_id, recurring_transaction_id) WHERE recurring_transaction_id IS NOT NULL;
CREATE UNIQUE INDEX uq_document_links_ledger ON document_links (document_id, ledger_entry_id) WHERE ledger_entry_id IS NOT NULL;
CREATE INDEX idx_document_links_transaction ON document_links (transaction_id) WHERE transaction_id IS NOT NULL;
CREATE INDEX idx_document_links_recurring ON document_links (recurring_transaction_id) WHERE recurring_transaction_id IS NOT NULL;
CREATE INDEX idx_document_links_ledger ON document_links (ledger_entry_id) WHERE ledger_entry_id IS NOT NULL;

CREATE TRIGGER trg_document_links_immutable
BEFORE UPDATE OF sync_id, document_id, transaction_id, recurring_transaction_id, ledger_entry_id ON document_links
WHEN NEW.sync_id IS NOT OLD.sync_id OR NEW.document_id IS NOT OLD.document_id OR NEW.transaction_id IS NOT OLD.transaction_id OR NEW.recurring_transaction_id IS NOT OLD.recurring_transaction_id OR NEW.ledger_entry_id IS NOT OLD.ledger_entry_id
BEGIN
  SELECT RAISE(ABORT, 'a link is made or removed, never changed');
END;

CREATE TRIGGER trg_document_links_sync_insert
AFTER INSERT ON document_links
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE document_links SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- A link needs a deletion marker only when a person removes it (0011): not when the foreign key removed it with its document or its target.
CREATE TRIGGER trg_document_links_sync_delete
AFTER DELETE ON document_links
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND EXISTS (SELECT 1 FROM documents WHERE id = OLD.document_id)
  AND (OLD.transaction_id IS NULL OR EXISTS (SELECT 1 FROM transactions WHERE id = OLD.transaction_id))
  AND (OLD.recurring_transaction_id IS NULL OR EXISTS (SELECT 1 FROM recurring_transactions WHERE id = OLD.recurring_transaction_id))
  AND (OLD.ledger_entry_id IS NULL OR EXISTS (SELECT 1 FROM ledger_entries WHERE id = OLD.ledger_entry_id))
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'document_links', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_document_links_audit_insert
AFTER INSERT ON document_links
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('document_links', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'document_id', NEW.document_id, 'transaction_id', NEW.transaction_id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'ledger_entry_id', NEW.ledger_entry_id, 'created_at', NEW.created_at));
END;

CREATE TRIGGER trg_document_links_audit_delete
AFTER DELETE ON document_links
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('document_links', OLD.id, 'DELETE', json_object('id', OLD.id, 'document_id', OLD.document_id, 'transaction_id', OLD.transaction_id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'ledger_entry_id', OLD.ledger_entry_id, 'created_at', OLD.created_at), NULL);
END;

-- The numbers ---------------------------------------------------------------
-- What a transaction costs YOU: its amount minus the other people's parts. An unsplit transaction costs its amount. Every sum that
-- is about what you spent or earned adds up `cost_minor`, never `transactions.amount_minor` (a test reads the source and fails when a
-- query does the second). The view carries the date and the category so a query can read it in place of the table. The subquery runs
-- for the rows a query reads, through `idx_split_lines_transaction`.
CREATE VIEW transaction_costs AS
SELECT t.id AS transaction_id,
       t.category_id AS category_id,
       t.payment_date AS payment_date,
       t.amount_minor - COALESCE((SELECT SUM(l.amount_minor) FROM transaction_split_lines l WHERE l.transaction_id = t.id), 0) AS cost_minor
FROM transactions t;

-- Who owes whom (positive: they owe you). A part of a bill counts like a LENT entry dated the day of the bill; a repayment is not matched
-- to a bill, so this is a running total.
CREATE VIEW person_balances AS
SELECT p.id AS person_id,
       COALESCE((SELECT SUM(l.amount_minor) FROM transaction_split_lines l WHERE l.person_id = p.id), 0)
       + COALESCE((SELECT SUM(CASE e.kind WHEN 'LENT' THEN e.amount_minor WHEN 'REPAID_BY_ME' THEN e.amount_minor ELSE -e.amount_minor END)
                   FROM ledger_entries e WHERE e.person_id = p.id), 0) AS balance_minor
FROM people p;
