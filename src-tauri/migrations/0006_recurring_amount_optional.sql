-- A recurring item may have no fixed amount: a bill that changes every time. The amount is then NULL, and
-- each payment says what was paid. SQLite cannot loosen a CHECK in place, so the table is rebuilt; the runner
-- turns foreign keys off around this migration (so dropping the old table does not clear the link on the
-- transactions that were paid from it) and checks them again before it commits.

CREATE TABLE recurring_transactions_new (
  id             INTEGER PRIMARY KEY,
  category_id    INTEGER NOT NULL REFERENCES categories (id) ON DELETE NO ACTION,
  name           TEXT NOT NULL,
  description    TEXT,
  amount_minor   INTEGER CONSTRAINT ck_rec_amount CHECK (amount_minor IS NULL OR amount_minor BETWEEN 1 AND 999999999999),
  start_date     TEXT NOT NULL,
  interval_unit  TEXT NOT NULL CONSTRAINT ck_rec_unit CHECK (interval_unit IN ('DAY', 'WEEK', 'MONTH', 'YEAR')),
  interval_count INTEGER NOT NULL DEFAULT 1 CONSTRAINT ck_rec_interval CHECK (interval_count > 0),
  next_due_date  TEXT NOT NULL,
  end_date       TEXT,
  is_active      INTEGER NOT NULL DEFAULT 1 CONSTRAINT ck_rec_active CHECK (is_active IN (0, 1)),
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_rec_dates CHECK (next_due_date >= start_date AND (end_date IS NULL OR end_date >= start_date))
) STRICT;

INSERT INTO recurring_transactions_new
  (id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count,
   next_due_date, end_date, is_active, created_at, updated_at)
SELECT id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count,
       next_due_date, end_date, is_active, created_at, updated_at
FROM recurring_transactions;

DROP TABLE recurring_transactions;
ALTER TABLE recurring_transactions_new RENAME TO recurring_transactions;

CREATE INDEX idx_recurring_due ON recurring_transactions (is_active, next_due_date);
CREATE INDEX idx_recurring_category ON recurring_transactions (category_id);

-- Dropping the table dropped its triggers; they are the ones from 0002, unchanged.
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
