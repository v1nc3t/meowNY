-- A currency per account (docs/accounts-design.md, section 21).
--
-- An account may be in a currency of its own: `currency` is three capital letters, NULL for an account in the app's currency (it follows
-- a change of that currency by itself), and `rate` is how many units of the account's currency one unit of the app's currency buys, as the
-- person typed it (NULL exactly when `currency` is NULL). A transaction in such an account keeps what really left or reached it in
-- `native_amount_minor` beside `amount_minor`, which stays in the app's currency and is what every budget and statistic counts. A transfer
-- between accounts of different currencies keeps what arrives in `to_amount_minor`; `amount_minor` is what leaves.
--
-- Columns only, no table is rebuilt. Each has its CHECK in the column definition. SQLite tests a new CHECK against the rows that exist, and they
-- all hold NULL. The CHECK of `rate` names `currency`: a column CHECK may name any column of its table, and `currency` is added before it.
--
-- A new column does not write the rows that exist: no trigger fires, no stamp or `updated_at` moves, so the main account (made by 0018 with
-- constants) is still the same row on every device, with the oldest stamp, and writes no history.
--
-- A change of one of them is a change of the row like any other: the triggers of the three tables that list the columns they watch (the sync
-- stamp and `updated_at`, and the history: the audit triggers) are made again with the new columns among them, their bodies as 0002 (the history
-- of transactions), 0018 and 0019 left them. They are made after the columns, so adding the columns is not an edit and writes no history. The
-- audit triggers of transactions still leave out `paid_by` and `account_id`, as 0014 to 0018 left them.

ALTER TABLE accounts ADD COLUMN currency TEXT CONSTRAINT ck_accounts_currency CHECK (currency IS NULL OR currency GLOB '[A-Z][A-Z][A-Z]');
ALTER TABLE accounts ADD COLUMN rate TEXT CONSTRAINT ck_accounts_rate CHECK ((currency IS NULL) = (rate IS NULL) AND (rate IS NULL OR length(rate) BETWEEN 1 AND 20));
ALTER TABLE transactions ADD COLUMN native_amount_minor INTEGER CONSTRAINT ck_transactions_native CHECK (native_amount_minor IS NULL OR native_amount_minor BETWEEN 0 AND 999999999999);
ALTER TABLE transfers ADD COLUMN to_amount_minor INTEGER CONSTRAINT ck_transfers_to_amount CHECK (to_amount_minor IS NULL OR to_amount_minor BETWEEN 1 AND 999999999999);

DROP TRIGGER trg_accounts_updated_at;
DROP TRIGGER trg_accounts_sync_update;
DROP TRIGGER trg_accounts_audit_insert;
DROP TRIGGER trg_accounts_audit_update;
DROP TRIGGER trg_accounts_audit_delete;

DROP TRIGGER trg_transactions_updated_at;
DROP TRIGGER trg_transactions_sync_update;
DROP TRIGGER trg_transactions_audit_insert;
DROP TRIGGER trg_transactions_audit_update;
DROP TRIGGER trg_transactions_audit_delete;

DROP TRIGGER trg_transfers_updated_at;
DROP TRIGGER trg_transfers_sync_update;
DROP TRIGGER trg_transfers_audit_insert;
DROP TRIGGER trg_transfers_audit_update;
DROP TRIGGER trg_transfers_audit_delete;

CREATE TRIGGER trg_accounts_updated_at
AFTER UPDATE OF id, name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate, created_at, updated_at ON accounts
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.bic IS NOT OLD.bic OR NEW.notes IS NOT OLD.notes OR NEW.group_name IS NOT OLD.group_name OR NEW.include_net_worth IS NOT OLD.include_net_worth OR NEW.virtual_balance_minor IS NOT OLD.virtual_balance_minor OR NEW.currency IS NOT OLD.currency OR NEW.rate IS NOT OLD.rate OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE accounts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_accounts_sync_update
AFTER UPDATE OF name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, currency, rate ON accounts
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.bic IS NOT OLD.bic OR NEW.notes IS NOT OLD.notes OR NEW.group_name IS NOT OLD.group_name OR NEW.include_net_worth IS NOT OLD.include_net_worth OR NEW.virtual_balance_minor IS NOT OLD.virtual_balance_minor OR NEW.currency IS NOT OLD.currency OR NEW.rate IS NOT OLD.rate)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE accounts SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_accounts_audit_insert
AFTER INSERT ON accounts
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'bic', NEW.bic, 'notes', NEW.notes, 'group_name', NEW.group_name, 'include_net_worth', NEW.include_net_worth, 'virtual_balance_minor', NEW.virtual_balance_minor, 'currency', NEW.currency, 'rate', NEW.rate, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_update
AFTER UPDATE ON accounts
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.kind IS NOT NEW.kind OR OLD.number IS NOT NEW.number OR OLD.opening_balance_minor IS NOT NEW.opening_balance_minor OR OLD.opening_date IS NOT NEW.opening_date OR OLD.archived IS NOT NEW.archived OR OLD.position IS NOT NEW.position OR OLD.bic IS NOT NEW.bic OR OLD.notes IS NOT NEW.notes OR OLD.group_name IS NOT NEW.group_name OR OLD.include_net_worth IS NOT NEW.include_net_worth OR OLD.virtual_balance_minor IS NOT NEW.virtual_balance_minor OR OLD.currency IS NOT NEW.currency OR OLD.rate IS NOT NEW.rate OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', NEW.id, 'UPDATE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'bic', OLD.bic, 'notes', OLD.notes, 'group_name', OLD.group_name, 'include_net_worth', OLD.include_net_worth, 'virtual_balance_minor', OLD.virtual_balance_minor, 'currency', OLD.currency, 'rate', OLD.rate, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'bic', NEW.bic, 'notes', NEW.notes, 'group_name', NEW.group_name, 'include_net_worth', NEW.include_net_worth, 'virtual_balance_minor', NEW.virtual_balance_minor, 'currency', NEW.currency, 'rate', NEW.rate, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_delete
AFTER DELETE ON accounts
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'bic', OLD.bic, 'notes', OLD.notes, 'group_name', OLD.group_name, 'include_net_worth', OLD.include_net_worth, 'virtual_balance_minor', OLD.virtual_balance_minor, 'currency', OLD.currency, 'rate', OLD.rate, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

CREATE TRIGGER trg_transactions_updated_at
AFTER UPDATE OF id, category_id, recurring_due_date, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, paid_by, account_id, native_amount_minor, created_at, updated_at ON transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_sync_update
AFTER UPDATE OF category_id, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, paid_by, account_id, native_amount_minor ON transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_audit_insert
AFTER INSERT ON transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transactions', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'category_id', NEW.category_id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'recurring_due_date', NEW.recurring_due_date, 'client_uuid', NEW.client_uuid, 'name', NEW.name, 'amount_minor', NEW.amount_minor, 'planned_amount_minor', NEW.planned_amount_minor, 'payment_date', NEW.payment_date, 'description', NEW.description, 'native_amount_minor', NEW.native_amount_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transactions_audit_update
AFTER UPDATE ON transactions
WHEN OLD.id IS NOT NEW.id OR OLD.category_id IS NOT NEW.category_id OR OLD.recurring_transaction_id IS NOT NEW.recurring_transaction_id OR OLD.recurring_due_date IS NOT NEW.recurring_due_date OR OLD.client_uuid IS NOT NEW.client_uuid OR OLD.name IS NOT NEW.name OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.planned_amount_minor IS NOT NEW.planned_amount_minor OR OLD.payment_date IS NOT NEW.payment_date OR OLD.description IS NOT NEW.description OR OLD.native_amount_minor IS NOT NEW.native_amount_minor OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transactions', NEW.id, 'UPDATE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'recurring_due_date', OLD.recurring_due_date, 'client_uuid', OLD.client_uuid, 'name', OLD.name, 'amount_minor', OLD.amount_minor, 'planned_amount_minor', OLD.planned_amount_minor, 'payment_date', OLD.payment_date, 'description', OLD.description, 'native_amount_minor', OLD.native_amount_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'category_id', NEW.category_id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'recurring_due_date', NEW.recurring_due_date, 'client_uuid', NEW.client_uuid, 'name', NEW.name, 'amount_minor', NEW.amount_minor, 'planned_amount_minor', NEW.planned_amount_minor, 'payment_date', NEW.payment_date, 'description', NEW.description, 'native_amount_minor', NEW.native_amount_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transactions_audit_delete
AFTER DELETE ON transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transactions', OLD.id, 'DELETE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'recurring_due_date', OLD.recurring_due_date, 'client_uuid', OLD.client_uuid, 'name', OLD.name, 'amount_minor', OLD.amount_minor, 'planned_amount_minor', OLD.planned_amount_minor, 'payment_date', OLD.payment_date, 'description', OLD.description, 'native_amount_minor', OLD.native_amount_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

CREATE TRIGGER trg_transfers_updated_at
AFTER UPDATE OF id, from_account_id, to_account_id, amount_minor, transfer_date, note, to_amount_minor, created_at, updated_at ON transfers
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.from_account_id IS NOT OLD.from_account_id OR NEW.to_account_id IS NOT OLD.to_account_id OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.transfer_date IS NOT OLD.transfer_date OR NEW.note IS NOT OLD.note OR NEW.to_amount_minor IS NOT OLD.to_amount_minor OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE transfers SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transfers_sync_update
AFTER UPDATE OF from_account_id, to_account_id, amount_minor, transfer_date, note, to_amount_minor ON transfers
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.from_account_id IS NOT OLD.from_account_id OR NEW.to_account_id IS NOT OLD.to_account_id OR NEW.amount_minor IS NOT OLD.amount_minor OR NEW.transfer_date IS NOT OLD.transfer_date OR NEW.note IS NOT OLD.note OR NEW.to_amount_minor IS NOT OLD.to_amount_minor)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transfers SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transfers_audit_insert
AFTER INSERT ON transfers
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transfers', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'from_account_id', NEW.from_account_id, 'to_account_id', NEW.to_account_id, 'amount_minor', NEW.amount_minor, 'transfer_date', NEW.transfer_date, 'note', NEW.note, 'to_amount_minor', NEW.to_amount_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transfers_audit_update
AFTER UPDATE ON transfers
WHEN OLD.id IS NOT NEW.id OR OLD.from_account_id IS NOT NEW.from_account_id OR OLD.to_account_id IS NOT NEW.to_account_id OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.transfer_date IS NOT NEW.transfer_date OR OLD.note IS NOT NEW.note OR OLD.to_amount_minor IS NOT NEW.to_amount_minor OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transfers', NEW.id, 'UPDATE', json_object('id', OLD.id, 'from_account_id', OLD.from_account_id, 'to_account_id', OLD.to_account_id, 'amount_minor', OLD.amount_minor, 'transfer_date', OLD.transfer_date, 'note', OLD.note, 'to_amount_minor', OLD.to_amount_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'from_account_id', NEW.from_account_id, 'to_account_id', NEW.to_account_id, 'amount_minor', NEW.amount_minor, 'transfer_date', NEW.transfer_date, 'note', NEW.note, 'to_amount_minor', NEW.to_amount_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transfers_audit_delete
AFTER DELETE ON transfers
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transfers', OLD.id, 'DELETE', json_object('id', OLD.id, 'from_account_id', OLD.from_account_id, 'to_account_id', OLD.to_account_id, 'amount_minor', OLD.amount_minor, 'transfer_date', OLD.transfer_date, 'note', OLD.note, 'to_amount_minor', OLD.to_amount_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;


-- `CHECK (id <> 1 OR currency IS NULL)` cannot be added to a table that exists, so the main account is kept in the app's currency by this
-- trigger, as `trg_accounts_keep_main` (0018) keeps it from being deleted, and by `accounts::check`. It stands aside while applying: a restore
-- and a sync write rows as they are and `accounts::check` refuses the result. There is no trigger for an INSERT: the row exists from 0018 on
-- and cannot be deleted, except by a restore, which stands aside.
CREATE TRIGGER trg_accounts_main_currency
BEFORE UPDATE OF currency ON accounts
WHEN NEW.id = 1 AND NEW.currency IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  SELECT RAISE(ABORT, 'the main account is in the main currency');
END;

-- What moved an account (0018, section 4.7), made again: a transaction in an account of another currency moves it by what the account
-- really got, `native_amount_minor`, whenever it has one; in an account of the app's currency the column is not looked at, so a value left
-- there (a row that came back to the main account when the account it was in was deleted on the other device) does no harm. A transfer
-- arrives in the `to` account by `to_amount_minor` when it has one and leaves the `from` account by `amount_minor`.
DROP VIEW account_movements;

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
