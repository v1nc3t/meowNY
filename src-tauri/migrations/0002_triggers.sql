-- Triggers: category type immutability, updated_at maintenance and the audit history.
--
-- updated_at: an AFTER UPDATE trigger stamps the row unless the statement set updated_at itself.
-- That second write fires the audit trigger again, but its WHEN clause skips changes
-- where only updated_at differs, so each real change is recorded once.
-- Audit rows hold the row as written by the statement, so on UPDATE new_values.updated_at
-- is the previous stamp.

CREATE TRIGGER trg_category_type_immutable
BEFORE UPDATE OF type ON categories
WHEN NEW.type <> OLD.type
BEGIN
  SELECT RAISE(ABORT, 'categories.type is immutable');
END;

-- settings ---------------------------------------------------------------
CREATE TRIGGER trg_settings_updated_at
AFTER UPDATE ON settings
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE settings SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_settings_audit_insert
AFTER INSERT ON settings
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('settings', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'currency', NEW.currency, 'locale', NEW.locale, 'timezone', NEW.timezone, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_settings_audit_update
AFTER UPDATE ON settings
WHEN OLD.id IS NOT NEW.id OR OLD.currency IS NOT NEW.currency OR OLD.locale IS NOT NEW.locale OR OLD.timezone IS NOT NEW.timezone OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('settings', NEW.id, 'UPDATE', json_object('id', OLD.id, 'currency', OLD.currency, 'locale', OLD.locale, 'timezone', OLD.timezone, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'currency', NEW.currency, 'locale', NEW.locale, 'timezone', NEW.timezone, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_settings_audit_delete
AFTER DELETE ON settings
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('settings', OLD.id, 'DELETE', json_object('id', OLD.id, 'currency', OLD.currency, 'locale', OLD.locale, 'timezone', OLD.timezone, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- categories ---------------------------------------------------------------
CREATE TRIGGER trg_categories_updated_at
AFTER UPDATE ON categories
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE categories SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_categories_audit_insert
AFTER INSERT ON categories
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('categories', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'type', NEW.type, 'name', NEW.name, 'deleted_at', NEW.deleted_at, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_categories_audit_update
AFTER UPDATE ON categories
WHEN OLD.id IS NOT NEW.id OR OLD.type IS NOT NEW.type OR OLD.name IS NOT NEW.name OR OLD.deleted_at IS NOT NEW.deleted_at OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('categories', NEW.id, 'UPDATE', json_object('id', OLD.id, 'type', OLD.type, 'name', OLD.name, 'deleted_at', OLD.deleted_at, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'type', NEW.type, 'name', NEW.name, 'deleted_at', NEW.deleted_at, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_categories_audit_delete
AFTER DELETE ON categories
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('categories', OLD.id, 'DELETE', json_object('id', OLD.id, 'type', OLD.type, 'name', OLD.name, 'deleted_at', OLD.deleted_at, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- budgets ---------------------------------------------------------------
CREATE TRIGGER trg_budgets_updated_at
AFTER UPDATE ON budgets
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE budgets SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_budgets_audit_insert
AFTER INSERT ON budgets
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('budgets', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'type', NEW.type, 'scope', NEW.scope, 'category_id', NEW.category_id, 'amount_limit_minor', NEW.amount_limit_minor, 'effective_from', NEW.effective_from, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_budgets_audit_update
AFTER UPDATE ON budgets
WHEN OLD.id IS NOT NEW.id OR OLD.type IS NOT NEW.type OR OLD.scope IS NOT NEW.scope OR OLD.category_id IS NOT NEW.category_id OR OLD.amount_limit_minor IS NOT NEW.amount_limit_minor OR OLD.effective_from IS NOT NEW.effective_from OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('budgets', NEW.id, 'UPDATE', json_object('id', OLD.id, 'type', OLD.type, 'scope', OLD.scope, 'category_id', OLD.category_id, 'amount_limit_minor', OLD.amount_limit_minor, 'effective_from', OLD.effective_from, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'type', NEW.type, 'scope', NEW.scope, 'category_id', NEW.category_id, 'amount_limit_minor', NEW.amount_limit_minor, 'effective_from', NEW.effective_from, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_budgets_audit_delete
AFTER DELETE ON budgets
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('budgets', OLD.id, 'DELETE', json_object('id', OLD.id, 'type', OLD.type, 'scope', OLD.scope, 'category_id', OLD.category_id, 'amount_limit_minor', OLD.amount_limit_minor, 'effective_from', OLD.effective_from, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- recurring_transactions ---------------------------------------------------------------
CREATE TRIGGER trg_recurring_transactions_updated_at
AFTER UPDATE ON recurring_transactions
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE recurring_transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_audit_insert
AFTER INSERT ON recurring_transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_transactions', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'category_id', NEW.category_id, 'name', NEW.name, 'description', NEW.description, 'amount_minor', NEW.amount_minor, 'start_date', NEW.start_date, 'interval_unit', NEW.interval_unit, 'interval_count', NEW.interval_count, 'next_due_date', NEW.next_due_date, 'end_date', NEW.end_date, 'is_active', NEW.is_active, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_recurring_transactions_audit_update
AFTER UPDATE ON recurring_transactions
WHEN OLD.id IS NOT NEW.id OR OLD.category_id IS NOT NEW.category_id OR OLD.name IS NOT NEW.name OR OLD.description IS NOT NEW.description OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.start_date IS NOT NEW.start_date OR OLD.interval_unit IS NOT NEW.interval_unit OR OLD.interval_count IS NOT NEW.interval_count OR OLD.next_due_date IS NOT NEW.next_due_date OR OLD.end_date IS NOT NEW.end_date OR OLD.is_active IS NOT NEW.is_active OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_transactions', NEW.id, 'UPDATE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'name', OLD.name, 'description', OLD.description, 'amount_minor', OLD.amount_minor, 'start_date', OLD.start_date, 'interval_unit', OLD.interval_unit, 'interval_count', OLD.interval_count, 'next_due_date', OLD.next_due_date, 'end_date', OLD.end_date, 'is_active', OLD.is_active, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'category_id', NEW.category_id, 'name', NEW.name, 'description', NEW.description, 'amount_minor', NEW.amount_minor, 'start_date', NEW.start_date, 'interval_unit', NEW.interval_unit, 'interval_count', NEW.interval_count, 'next_due_date', NEW.next_due_date, 'end_date', NEW.end_date, 'is_active', NEW.is_active, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_recurring_transactions_audit_delete
AFTER DELETE ON recurring_transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('recurring_transactions', OLD.id, 'DELETE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'name', OLD.name, 'description', OLD.description, 'amount_minor', OLD.amount_minor, 'start_date', OLD.start_date, 'interval_unit', OLD.interval_unit, 'interval_count', OLD.interval_count, 'next_due_date', OLD.next_due_date, 'end_date', OLD.end_date, 'is_active', OLD.is_active, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

-- transactions ---------------------------------------------------------------
CREATE TRIGGER trg_transactions_updated_at
AFTER UPDATE ON transactions
WHEN NEW.updated_at IS OLD.updated_at
BEGIN
  UPDATE transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_audit_insert
AFTER INSERT ON transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transactions', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'category_id', NEW.category_id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'recurring_due_date', NEW.recurring_due_date, 'client_uuid', NEW.client_uuid, 'name', NEW.name, 'amount_minor', NEW.amount_minor, 'planned_amount_minor', NEW.planned_amount_minor, 'payment_date', NEW.payment_date, 'description', NEW.description, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transactions_audit_update
AFTER UPDATE ON transactions
WHEN OLD.id IS NOT NEW.id OR OLD.category_id IS NOT NEW.category_id OR OLD.recurring_transaction_id IS NOT NEW.recurring_transaction_id OR OLD.recurring_due_date IS NOT NEW.recurring_due_date OR OLD.client_uuid IS NOT NEW.client_uuid OR OLD.name IS NOT NEW.name OR OLD.amount_minor IS NOT NEW.amount_minor OR OLD.planned_amount_minor IS NOT NEW.planned_amount_minor OR OLD.payment_date IS NOT NEW.payment_date OR OLD.description IS NOT NEW.description OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transactions', NEW.id, 'UPDATE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'recurring_due_date', OLD.recurring_due_date, 'client_uuid', OLD.client_uuid, 'name', OLD.name, 'amount_minor', OLD.amount_minor, 'planned_amount_minor', OLD.planned_amount_minor, 'payment_date', OLD.payment_date, 'description', OLD.description, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), json_object('id', NEW.id, 'category_id', NEW.category_id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'recurring_due_date', NEW.recurring_due_date, 'client_uuid', NEW.client_uuid, 'name', NEW.name, 'amount_minor', NEW.amount_minor, 'planned_amount_minor', NEW.planned_amount_minor, 'payment_date', NEW.payment_date, 'description', NEW.description, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_transactions_audit_delete
AFTER DELETE ON transactions
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('transactions', OLD.id, 'DELETE', json_object('id', OLD.id, 'category_id', OLD.category_id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'recurring_due_date', OLD.recurring_due_date, 'client_uuid', OLD.client_uuid, 'name', OLD.name, 'amount_minor', OLD.amount_minor, 'planned_amount_minor', OLD.planned_amount_minor, 'payment_date', OLD.payment_date, 'description', OLD.description, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;
