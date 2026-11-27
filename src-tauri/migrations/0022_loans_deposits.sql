-- Loans, deposits and return rates (docs/loans-deposits-design.md, section 4).
--
-- Three more kinds of account (CREDIT, LOAN, MORTGAGE: the accounts you owe on), a rate on any account (the return of savings, the interest of a
-- debt) with the installment of a debt, and the columns a term deposit needs on a stash. Nothing that is saved is worked out here: owed, state,
-- interest, payoff and return are all computed when they are asked for (docs/loans-deposits-design.md, section 3).
--
-- The kind `CHECK` of `accounts` cannot be widened in place (accounts-design.md 20.1), so the table is rebuilt by SQLite's own twelve steps:
-- the view and the triggers that depend on it go first, the new table is filled from the old one, and everything comes back. Foreign keys are
-- off while a migration runs and checked before the commit (`migrate::apply`), so dropping the table that four others and `stashes` point at
-- does not touch their rows, and they point at the new table by its name. Every row keeps its id, `sync_id`, stamp and dates: nothing is
-- an edit, so no trigger fires, no stamp or `updated_at` moves and no history row is made. The new columns are NULL, which makes the main
-- account (made by 0018 with constants) still the same row on every device, so no device stamps it.
--
-- No trigger of another table mentions `accounts`, and the only view that reads it is `account_movements`; both were looked up in
-- `sqlite_master` of a database built by migrations 1 to 21 (a test proves that the same names exist after this one).

-- 4.1 accounts is rebuilt ------------------------------------------------------
DROP VIEW account_movements;

DROP TRIGGER trg_accounts_updated_at;
DROP TRIGGER trg_accounts_sync_insert;
DROP TRIGGER trg_accounts_sync_update;
DROP TRIGGER trg_accounts_sync_delete;
DROP TRIGGER trg_accounts_audit_insert;
DROP TRIGGER trg_accounts_audit_update;
DROP TRIGGER trg_accounts_audit_delete;
DROP TRIGGER trg_accounts_keep_main;
DROP TRIGGER trg_accounts_main_currency;

-- Every column of 0018 to 0020 in the same order and with the same checks, then the three new ones. `rate_milli` is the percent times 1000
-- (3.25 % is 3250); a rate and its period are both there or both absent. An installment is for a liability only: the check names `kind`, and
-- `kind` cannot change across the groups (`accounts::update`), so it never ends on an asset.
CREATE TABLE accounts_new (
  id                    INTEGER PRIMARY KEY,
  sync_id               TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_accounts_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp                 TEXT,
  name                  TEXT NOT NULL CONSTRAINT ck_accounts_name CHECK (length(name) BETWEEN 1 AND 50),
  kind                  TEXT NOT NULL CONSTRAINT ck_accounts_kind CHECK (kind IN ('CHECKING', 'SAVINGS', 'CASH', 'CREDIT_CARD', 'CREDIT', 'LOAN', 'MORTGAGE')),
  number                TEXT CONSTRAINT ck_accounts_number CHECK (number IS NULL OR length(number) BETWEEN 1 AND 50),
  opening_balance_minor INTEGER NOT NULL DEFAULT 0
    CONSTRAINT ck_accounts_opening CHECK (opening_balance_minor BETWEEN -999999999999 AND 999999999999),
  opening_date          TEXT NOT NULL CONSTRAINT ck_accounts_opening_date
    CHECK (opening_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(opening_date) IS opening_date),
  archived              INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_accounts_archived CHECK (archived IN (0, 1)),
  position              INTEGER NOT NULL DEFAULT 0,
  created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  bic                   TEXT CONSTRAINT ck_accounts_bic CHECK (bic IS NULL OR length(bic) BETWEEN 1 AND 20),
  notes                 TEXT CONSTRAINT ck_accounts_notes CHECK (notes IS NULL OR length(notes) BETWEEN 1 AND 1000),
  group_name            TEXT CONSTRAINT ck_accounts_group CHECK (group_name IS NULL OR length(group_name) BETWEEN 1 AND 50),
  include_net_worth     INTEGER NOT NULL DEFAULT 1 CONSTRAINT ck_accounts_include_net_worth CHECK (include_net_worth IN (0, 1)),
  virtual_balance_minor INTEGER NOT NULL DEFAULT 0
    CONSTRAINT ck_accounts_virtual CHECK (virtual_balance_minor BETWEEN -999999999999 AND 999999999999),
  currency              TEXT CONSTRAINT ck_accounts_currency CHECK (currency IS NULL OR currency GLOB '[A-Z][A-Z][A-Z]'),
  rate                  TEXT CONSTRAINT ck_accounts_rate CHECK ((currency IS NULL) = (rate IS NULL) AND (rate IS NULL OR length(rate) BETWEEN 1 AND 20)),
  rate_milli            INTEGER CONSTRAINT ck_accounts_rate_milli CHECK (rate_milli IS NULL OR rate_milli BETWEEN 0 AND 100000),
  rate_period           TEXT CONSTRAINT ck_accounts_rate_period
    CHECK ((rate_milli IS NULL) = (rate_period IS NULL) AND (rate_period IS NULL OR rate_period IN ('MONTHLY', 'YEARLY'))),
  installment_minor     INTEGER CONSTRAINT ck_accounts_installment
    CHECK (installment_minor IS NULL OR (installment_minor BETWEEN 1 AND 999999999999 AND kind IN ('CREDIT', 'LOAN', 'MORTGAGE'))),
  -- the main account is never archived, whoever writes (a sync and a restore too)
  CONSTRAINT ck_accounts_main_active CHECK (id <> 1 OR archived = 0)
) STRICT;

INSERT INTO accounts_new (id, sync_id, stamp, name, kind, number, opening_balance_minor, opening_date, archived, position, created_at, updated_at,
                          bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate)
SELECT id, sync_id, stamp, name, kind, number, opening_balance_minor, opening_date, archived, position, created_at, updated_at,
       bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate
FROM accounts;
DROP TABLE accounts;
ALTER TABLE accounts_new RENAME TO accounts;
CREATE UNIQUE INDEX uq_accounts_sync_id ON accounts (sync_id);

-- The triggers of `accounts` as 0021 left them (0018 made them, 0019 and 0020 added columns, 0021 made `_sync_delete` again for the rebuilt table of markers), with the three
-- new columns in every list: the two that list the columns they watch, their `WHEN` tests, and the history. Without them in the history, a
-- change of the rate alone would leave no row of it. The bodies of the others are word for word the same.
CREATE TRIGGER trg_accounts_updated_at
AFTER UPDATE OF id, name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate, rate_milli, rate_period, installment_minor, created_at, updated_at ON accounts
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.bic IS NOT OLD.bic OR NEW.notes IS NOT OLD.notes OR NEW.group_name IS NOT OLD.group_name OR NEW.include_net_worth IS NOT OLD.include_net_worth OR NEW.virtual_balance_minor IS NOT OLD.virtual_balance_minor OR NEW.currency IS NOT OLD.currency OR NEW.rate IS NOT OLD.rate OR NEW.rate_milli IS NOT OLD.rate_milli OR NEW.rate_period IS NOT OLD.rate_period OR NEW.installment_minor IS NOT OLD.installment_minor OR NEW.created_at IS NOT OLD.created_at)
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
AFTER UPDATE OF name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate, rate_milli, rate_period, installment_minor ON accounts
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.bic IS NOT OLD.bic OR NEW.notes IS NOT OLD.notes OR NEW.group_name IS NOT OLD.group_name OR NEW.include_net_worth IS NOT OLD.include_net_worth OR NEW.virtual_balance_minor IS NOT OLD.virtual_balance_minor OR NEW.currency IS NOT OLD.currency OR NEW.rate IS NOT OLD.rate OR NEW.rate_milli IS NOT OLD.rate_milli OR NEW.rate_period IS NOT OLD.rate_period OR NEW.installment_minor IS NOT OLD.installment_minor)
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
  VALUES ('accounts', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'bic', NEW.bic, 'notes', NEW.notes, 'group_name', NEW.group_name, 'include_net_worth', NEW.include_net_worth, 'virtual_balance_minor', NEW.virtual_balance_minor, 'currency', NEW.currency, 'rate', NEW.rate, 'rate_milli', NEW.rate_milli, 'rate_period', NEW.rate_period, 'installment_minor', NEW.installment_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_update
AFTER UPDATE ON accounts
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.kind IS NOT NEW.kind OR OLD.number IS NOT NEW.number OR OLD.opening_balance_minor IS NOT NEW.opening_balance_minor OR OLD.opening_date IS NOT NEW.opening_date OR OLD.archived IS NOT NEW.archived OR OLD.position IS NOT NEW.position OR OLD.bic IS NOT NEW.bic OR OLD.notes IS NOT NEW.notes OR OLD.group_name IS NOT NEW.group_name OR OLD.include_net_worth IS NOT NEW.include_net_worth OR OLD.virtual_balance_minor IS NOT NEW.virtual_balance_minor OR OLD.currency IS NOT NEW.currency OR OLD.rate IS NOT NEW.rate OR OLD.rate_milli IS NOT NEW.rate_milli OR OLD.rate_period IS NOT NEW.rate_period OR OLD.installment_minor IS NOT NEW.installment_minor OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', NEW.id, 'UPDATE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'bic', OLD.bic, 'notes', OLD.notes, 'group_name', OLD.group_name, 'include_net_worth', OLD.include_net_worth, 'virtual_balance_minor', OLD.virtual_balance_minor, 'currency', OLD.currency, 'rate', OLD.rate, 'rate_milli', OLD.rate_milli, 'rate_period', OLD.rate_period, 'installment_minor', OLD.installment_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'bic', NEW.bic, 'notes', NEW.notes, 'group_name', NEW.group_name, 'include_net_worth', NEW.include_net_worth, 'virtual_balance_minor', NEW.virtual_balance_minor, 'currency', NEW.currency, 'rate', NEW.rate, 'rate_milli', NEW.rate_milli, 'rate_period', NEW.rate_period, 'installment_minor', NEW.installment_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_delete
AFTER DELETE ON accounts
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'bic', OLD.bic, 'notes', OLD.notes, 'group_name', OLD.group_name, 'include_net_worth', OLD.include_net_worth, 'virtual_balance_minor', OLD.virtual_balance_minor, 'currency', OLD.currency, 'rate', OLD.rate, 'rate_milli', OLD.rate_milli, 'rate_period', OLD.rate_period, 'installment_minor', OLD.installment_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

CREATE TRIGGER trg_accounts_keep_main
BEFORE DELETE ON accounts
WHEN OLD.id = 1 AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'the main account is never deleted');
END;

CREATE TRIGGER trg_accounts_main_currency
BEFORE UPDATE OF currency ON accounts
WHEN NEW.id = 1 AND NEW.currency IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'the main account is in the main currency');
END;

-- What moved an account (0018, section 4.7, as 0020 made it), word for word: it has to come back, since it was dropped for the rebuild.
CREATE VIEW account_movements AS
SELECT a.id AS account_id, a.opening_date AS day, a.opening_balance_minor AS delta_minor, 'OPENING' AS source, a.id AS source_id
  FROM accounts a WHERE a.opening_balance_minor <> 0
UNION ALL
SELECT t.account_id, t.payment_date,
       CASE c.type WHEN 'INCOME' THEN 1 ELSE -1 END * COALESCE(CASE WHEN a.currency IS NOT NULL THEN t.native_amount_minor END, t.amount_minor),
       'TRANSACTION', t.id
  FROM transactions t JOIN categories c ON c.id = t.category_id JOIN accounts a ON a.id = t.account_id WHERE t.paid_by IS NULL
UNION ALL
SELECT x.to_account_id, x.transfer_date, COALESCE(x.to_amount_minor, x.amount_minor), 'TRANSFER_IN', x.id FROM transfers x
UNION ALL
SELECT x.from_account_id, x.transfer_date, -x.amount_minor, 'TRANSFER_OUT', x.id FROM transfers x
UNION ALL
SELECT e.account_id, e.entry_date,
       CASE e.kind WHEN 'BORROWED' THEN e.amount_minor WHEN 'REPAID_TO_ME' THEN e.amount_minor ELSE -e.amount_minor END, 'LEDGER', e.id
  FROM ledger_entries e;

-- 4.2 stashes gain the columns of a term deposit -------------------------------
-- Columns only, in this order (a column `CHECK` may name a column added before it, as 0020 did for `rate`). SQLite tests each new `CHECK`
-- against the rows that exist, and they all hold the defaults: a `GOAL`, no rate, no dates. `target_date` is the maturity of a deposit; that it
-- is there and after `start_date` cannot be a `CHECK` here (ALTER TABLE adds column checks only, and `target_date` came before), so it is a rule
-- of the core (`stashes.rs`).
ALTER TABLE stashes ADD COLUMN kind TEXT NOT NULL DEFAULT 'GOAL' CONSTRAINT ck_stashes_kind CHECK (kind IN ('GOAL', 'DEPOSIT'));
ALTER TABLE stashes ADD COLUMN rate_milli INTEGER CONSTRAINT ck_stashes_rate_milli
  CHECK ((kind = 'DEPOSIT') = (rate_milli IS NOT NULL) AND (rate_milli IS NULL OR rate_milli BETWEEN 0 AND 100000));
ALTER TABLE stashes ADD COLUMN rate_period TEXT CONSTRAINT ck_stashes_rate_period
  CHECK ((rate_milli IS NULL) = (rate_period IS NULL) AND (rate_period IS NULL OR rate_period IN ('MONTHLY', 'YEARLY')));
ALTER TABLE stashes ADD COLUMN start_date TEXT CONSTRAINT ck_stashes_start_date
  CHECK ((kind = 'DEPOSIT') = (start_date IS NOT NULL) AND (start_date IS NULL OR (start_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(start_date) IS start_date)));
ALTER TABLE stashes ADD COLUMN broken_on TEXT CONSTRAINT ck_stashes_broken_on
  CHECK (broken_on IS NULL OR (kind = 'DEPOSIT' AND broken_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(broken_on) IS broken_on));
ALTER TABLE stashes ADD COLUMN returned_on TEXT CONSTRAINT ck_stashes_returned_on
  CHECK (returned_on IS NULL OR (kind = 'DEPOSIT' AND returned_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(returned_on) IS returned_on));

-- A change of one of the six is a change of the row like any other, so the triggers of `stashes` that list the columns they watch (`updated_at`,
-- the sync stamp, and the three of the history) are made again with them, as 0019 did for accounts, bodies as 0021 left them. They are made
-- after the columns, so adding the columns is not an edit and writes no history.
DROP TRIGGER trg_stashes_updated_at;
DROP TRIGGER trg_stashes_sync_update;
DROP TRIGGER trg_stashes_audit_insert;
DROP TRIGGER trg_stashes_audit_update;
DROP TRIGGER trg_stashes_audit_delete;

CREATE TRIGGER trg_stashes_updated_at
AFTER UPDATE OF id, name, account_id, target_minor, target_date, notes, archived, position, kind, rate_milli, rate_period, start_date, broken_on, returned_on, created_at, updated_at ON stashes
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.account_id IS NOT OLD.account_id OR NEW.target_minor IS NOT OLD.target_minor OR NEW.target_date IS NOT OLD.target_date OR NEW.notes IS NOT OLD.notes OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.kind IS NOT OLD.kind OR NEW.rate_milli IS NOT OLD.rate_milli OR NEW.rate_period IS NOT OLD.rate_period OR NEW.start_date IS NOT OLD.start_date OR NEW.broken_on IS NOT OLD.broken_on OR NEW.returned_on IS NOT OLD.returned_on OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE stashes SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stashes_sync_update
AFTER UPDATE OF name, account_id, target_minor, target_date, notes, archived, position, kind, rate_milli, rate_period, start_date, broken_on, returned_on ON stashes
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.account_id IS NOT OLD.account_id OR NEW.target_minor IS NOT OLD.target_minor OR NEW.target_date IS NOT OLD.target_date OR NEW.notes IS NOT OLD.notes OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.kind IS NOT OLD.kind OR NEW.rate_milli IS NOT OLD.rate_milli OR NEW.rate_period IS NOT OLD.rate_period OR NEW.start_date IS NOT OLD.start_date OR NEW.broken_on IS NOT OLD.broken_on OR NEW.returned_on IS NOT OLD.returned_on)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE stashes SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_stashes_audit_insert
AFTER INSERT ON stashes
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stashes', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'account_id', NEW.account_id, 'target_minor', NEW.target_minor, 'target_date', NEW.target_date, 'notes', NEW.notes, 'archived', NEW.archived, 'position', NEW.position, 'kind', NEW.kind, 'rate_milli', NEW.rate_milli, 'rate_period', NEW.rate_period, 'start_date', NEW.start_date, 'broken_on', NEW.broken_on, 'returned_on', NEW.returned_on, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_stashes_audit_update
AFTER UPDATE ON stashes
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.account_id IS NOT NEW.account_id OR OLD.target_minor IS NOT NEW.target_minor OR OLD.target_date IS NOT NEW.target_date OR OLD.notes IS NOT NEW.notes OR OLD.archived IS NOT NEW.archived OR OLD.position IS NOT NEW.position OR OLD.kind IS NOT NEW.kind OR OLD.rate_milli IS NOT NEW.rate_milli OR OLD.rate_period IS NOT NEW.rate_period OR OLD.start_date IS NOT NEW.start_date OR OLD.broken_on IS NOT NEW.broken_on OR OLD.returned_on IS NOT NEW.returned_on OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stashes', NEW.id, 'UPDATE', json_object('id', OLD.id, 'name', OLD.name, 'account_id', OLD.account_id, 'target_minor', OLD.target_minor, 'target_date', OLD.target_date, 'notes', OLD.notes, 'archived', OLD.archived, 'position', OLD.position, 'kind', OLD.kind, 'rate_milli', OLD.rate_milli, 'rate_period', OLD.rate_period, 'start_date', OLD.start_date, 'broken_on', OLD.broken_on, 'returned_on', OLD.returned_on, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'name', NEW.name, 'account_id', NEW.account_id, 'target_minor', NEW.target_minor, 'target_date', NEW.target_date, 'notes', NEW.notes, 'archived', NEW.archived, 'position', NEW.position, 'kind', NEW.kind, 'rate_milli', NEW.rate_milli, 'rate_period', NEW.rate_period, 'start_date', NEW.start_date, 'broken_on', NEW.broken_on, 'returned_on', NEW.returned_on, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_stashes_audit_delete
AFTER DELETE ON stashes
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('stashes', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'account_id', OLD.account_id, 'target_minor', OLD.target_minor, 'target_date', OLD.target_date, 'notes', OLD.notes, 'archived', OLD.archived, 'position', OLD.position, 'kind', OLD.kind, 'rate_milli', OLD.rate_milli, 'rate_period', OLD.rate_period, 'start_date', OLD.start_date, 'broken_on', OLD.broken_on, 'returned_on', OLD.returned_on, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;
