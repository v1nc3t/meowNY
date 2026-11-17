-- More of what an account has (docs/accounts-design.md, section 20.2): a BIC, notes, a group, whether it counts in net worth, and a
-- virtual balance (for a credit card, its limit: what the account can still pay is the balance plus it).
--
-- Columns only, no table is rebuilt. Each has its CHECK in the column definition. SQLite tests a new CHECK against the rows that exist,
-- and they all hold the default.
--
-- A new column does not write the rows that exist: no trigger fires, no stamp or `updated_at` moves, so the main account (made by 0018 with
-- constants) is still the same row on every device, with the oldest stamp.
--
-- A change of one of them is a change of the row like any other: the two triggers of `accounts` that list the columns they watch (the
-- sync stamp and `updated_at`, 0018) are made again with the five columns among them, their bodies as 0018 left them. So are the three
-- audit triggers: without that, a change of a detail alone would leave no row of history. They are made after the columns, so adding the
-- columns is not an edit and writes no history of the main account.

ALTER TABLE accounts ADD COLUMN bic TEXT CONSTRAINT ck_accounts_bic CHECK (bic IS NULL OR length(bic) BETWEEN 1 AND 20);
ALTER TABLE accounts ADD COLUMN notes TEXT CONSTRAINT ck_accounts_notes CHECK (notes IS NULL OR length(notes) BETWEEN 1 AND 1000);
ALTER TABLE accounts ADD COLUMN group_name TEXT CONSTRAINT ck_accounts_group CHECK (group_name IS NULL OR length(group_name) BETWEEN 1 AND 50);
ALTER TABLE accounts ADD COLUMN include_net_worth INTEGER NOT NULL DEFAULT 1 CONSTRAINT ck_accounts_include_net_worth CHECK (include_net_worth IN (0, 1));
ALTER TABLE accounts ADD COLUMN virtual_balance_minor INTEGER NOT NULL DEFAULT 0
  CONSTRAINT ck_accounts_virtual CHECK (virtual_balance_minor BETWEEN -999999999999 AND 999999999999);

DROP TRIGGER trg_accounts_updated_at;
DROP TRIGGER trg_accounts_sync_update;
DROP TRIGGER trg_accounts_audit_insert;
DROP TRIGGER trg_accounts_audit_update;
DROP TRIGGER trg_accounts_audit_delete;

CREATE TRIGGER trg_accounts_updated_at
AFTER UPDATE OF id, name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor, created_at, updated_at ON accounts
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.bic IS NOT OLD.bic OR NEW.notes IS NOT OLD.notes OR NEW.group_name IS NOT OLD.group_name OR NEW.include_net_worth IS NOT OLD.include_net_worth OR NEW.virtual_balance_minor IS NOT OLD.virtual_balance_minor OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE accounts SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_accounts_sync_update
AFTER UPDATE OF name, kind, number, opening_balance_minor, opening_date, archived, position, bic, notes, group_name, include_net_worth, virtual_balance_minor ON accounts
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.kind IS NOT OLD.kind OR NEW.number IS NOT OLD.number OR NEW.opening_balance_minor IS NOT OLD.opening_balance_minor OR NEW.opening_date IS NOT OLD.opening_date OR NEW.archived IS NOT OLD.archived OR NEW.position IS NOT OLD.position OR NEW.bic IS NOT OLD.bic OR NEW.notes IS NOT OLD.notes OR NEW.group_name IS NOT OLD.group_name OR NEW.include_net_worth IS NOT OLD.include_net_worth OR NEW.virtual_balance_minor IS NOT OLD.virtual_balance_minor)
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
  VALUES ('accounts', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'bic', NEW.bic, 'notes', NEW.notes, 'group_name', NEW.group_name, 'include_net_worth', NEW.include_net_worth, 'virtual_balance_minor', NEW.virtual_balance_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_update
AFTER UPDATE ON accounts
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.kind IS NOT NEW.kind OR OLD.number IS NOT NEW.number OR OLD.opening_balance_minor IS NOT NEW.opening_balance_minor OR OLD.opening_date IS NOT NEW.opening_date OR OLD.archived IS NOT NEW.archived OR OLD.position IS NOT NEW.position OR OLD.bic IS NOT NEW.bic OR OLD.notes IS NOT NEW.notes OR OLD.group_name IS NOT NEW.group_name OR OLD.include_net_worth IS NOT NEW.include_net_worth OR OLD.virtual_balance_minor IS NOT NEW.virtual_balance_minor OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', NEW.id, 'UPDATE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'bic', OLD.bic, 'notes', OLD.notes, 'group_name', OLD.group_name, 'include_net_worth', OLD.include_net_worth, 'virtual_balance_minor', OLD.virtual_balance_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'name', NEW.name, 'kind', NEW.kind, 'number', NEW.number, 'opening_balance_minor', NEW.opening_balance_minor, 'opening_date', NEW.opening_date, 'archived', NEW.archived, 'position', NEW.position, 'bic', NEW.bic, 'notes', NEW.notes, 'group_name', NEW.group_name, 'include_net_worth', NEW.include_net_worth, 'virtual_balance_minor', NEW.virtual_balance_minor, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_accounts_audit_delete
AFTER DELETE ON accounts
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('accounts', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'kind', OLD.kind, 'number', OLD.number, 'opening_balance_minor', OLD.opening_balance_minor, 'opening_date', OLD.opening_date, 'archived', OLD.archived, 'position', OLD.position, 'bic', OLD.bic, 'notes', OLD.notes, 'group_name', OLD.group_name, 'include_net_worth', OLD.include_net_worth, 'virtual_balance_minor', OLD.virtual_balance_minor, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;
