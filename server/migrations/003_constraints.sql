-- Up Migration

ALTER TABLE user_settings ADD CONSTRAINT ck_settings_currency
  CHECK (currency ~ '^[A-Z]{3}$');

ALTER TABLE privacy_requests ADD CONSTRAINT ck_privacy_type
  CHECK (type IN ('EXPORT', 'ERASURE', 'RECTIFICATION', 'OTHER'));
ALTER TABLE privacy_requests ADD CONSTRAINT ck_privacy_status
  CHECK (status IN ('PENDING', 'COMPLETED', 'REJECTED'));

ALTER TABLE categories ADD CONSTRAINT ck_categories_type
  CHECK (type IN ('INCOME', 'EXPENSE'));

ALTER TABLE budgets ADD CONSTRAINT ck_budget_type
  CHECK (type IN ('INCOME', 'EXPENSE'));
ALTER TABLE budgets ADD CONSTRAINT ck_budget_scope
  CHECK (scope IN ('GLOBAL', 'CATEGORY'));
ALTER TABLE budgets ADD CONSTRAINT ck_budget_scope_category CHECK (
  (scope = 'GLOBAL' AND category_id IS NULL) OR
  (scope = 'CATEGORY' AND category_id IS NOT NULL));
ALTER TABLE budgets ADD CONSTRAINT ck_budget_first_of_month
  CHECK (EXTRACT(DAY FROM effective_from) = 1);
ALTER TABLE budgets ADD CONSTRAINT ck_budget_amount
  CHECK (amount_limit IS NULL OR amount_limit >= 0);

ALTER TABLE transactions ADD CONSTRAINT ck_tx_amount
  CHECK (amount > 0);
ALTER TABLE transactions ADD CONSTRAINT ck_tx_planned
  CHECK (planned_amount IS NULL OR planned_amount >= 0);
ALTER TABLE transactions ADD CONSTRAINT ck_tx_recurring_due
  CHECK (recurring_transaction_id IS NULL OR recurring_due_date IS NOT NULL);

ALTER TABLE recurring_transactions ADD CONSTRAINT ck_rec_amount
  CHECK (amount > 0);
ALTER TABLE recurring_transactions ADD CONSTRAINT ck_rec_unit
  CHECK (interval_unit IN ('DAY', 'WEEK', 'MONTH', 'YEAR'));
ALTER TABLE recurring_transactions ADD CONSTRAINT ck_rec_interval
  CHECK (interval_count > 0);
ALTER TABLE recurring_transactions ADD CONSTRAINT ck_rec_dates
  CHECK (next_due_date >= start_date AND (end_date IS NULL OR end_date >= start_date));

ALTER TABLE audit_log ADD CONSTRAINT ck_audit_action
  CHECK (action IN ('INSERT', 'UPDATE', 'DELETE'));

CREATE UNIQUE INDEX uq_categories_active_name
  ON categories (user_id, type, name) WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX uq_budget_global_month
  ON budgets (user_id, type, effective_from) WHERE scope = 'GLOBAL';

CREATE FUNCTION prevent_category_type_change() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.type <> OLD.type THEN
    RAISE EXCEPTION 'categories.type is immutable (category id %)', OLD.id;
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER trg_category_type_immutable
  BEFORE UPDATE OF type ON categories
  FOR EACH ROW EXECUTE FUNCTION prevent_category_type_change();

CREATE FUNCTION set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  NEW.updated_at := now();
  RETURN NEW;
END $$;

CREATE FUNCTION audit_row_change() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
  v_old jsonb := CASE WHEN TG_OP IN ('UPDATE', 'DELETE') THEN to_jsonb(OLD) END;
  v_new jsonb := CASE WHEN TG_OP IN ('INSERT', 'UPDATE') THEN to_jsonb(NEW) END;
  v_row jsonb := COALESCE(v_new, v_old);
BEGIN
  IF TG_OP = 'UPDATE' AND (v_old - 'updated_at') = (v_new - 'updated_at') THEN
    RETURN NULL;
  END IF;

  INSERT INTO audit_log (user_id, entity_type, entity_id, action, old_values, new_values)
  VALUES (
    CASE WHEN TG_TABLE_NAME = 'users' THEN (v_row->>'id')::uuid
         ELSE (v_row->>'user_id')::uuid END,
    TG_TABLE_NAME,
    COALESCE(v_row->>'id', v_row->>'user_id'),
    TG_OP,
    v_old,
    v_new);
  RETURN NULL;
END $$;

DO $$
DECLARE t text;
BEGIN
  FOREACH t IN ARRAY ARRAY['users', 'user_settings', 'category_groups', 'categories',
                           'budgets', 'recurring_transactions', 'transactions']
  LOOP
    EXECUTE format('CREATE TRIGGER trg_updated_at BEFORE UPDATE ON %I
                    FOR EACH ROW EXECUTE FUNCTION set_updated_at()', t);
    EXECUTE format('CREATE TRIGGER trg_audit AFTER INSERT OR UPDATE OR DELETE ON %I
                    FOR EACH ROW EXECUTE FUNCTION audit_row_change()', t);
  END LOOP;
END $$;

-- Down Migration

DROP TRIGGER IF EXISTS trg_updated_at ON users;
DROP TRIGGER IF EXISTS trg_audit ON users;
DROP TRIGGER IF EXISTS trg_updated_at ON user_settings;
DROP TRIGGER IF EXISTS trg_audit ON user_settings;
DROP TRIGGER IF EXISTS trg_updated_at ON category_groups;
DROP TRIGGER IF EXISTS trg_audit ON category_groups;
DROP TRIGGER IF EXISTS trg_updated_at ON categories;
DROP TRIGGER IF EXISTS trg_audit ON categories;
DROP TRIGGER IF EXISTS trg_category_type_immutable ON categories;
DROP TRIGGER IF EXISTS trg_updated_at ON budgets;
DROP TRIGGER IF EXISTS trg_audit ON budgets;
DROP TRIGGER IF EXISTS trg_updated_at ON recurring_transactions;
DROP TRIGGER IF EXISTS trg_audit ON recurring_transactions;
DROP TRIGGER IF EXISTS trg_updated_at ON transactions;
DROP TRIGGER IF EXISTS trg_audit ON transactions;

DROP FUNCTION IF EXISTS audit_row_change();
DROP FUNCTION IF EXISTS set_updated_at();
DROP FUNCTION IF EXISTS prevent_category_type_change();

DROP INDEX IF EXISTS uq_budget_global_month;
DROP INDEX IF EXISTS uq_categories_active_name;

ALTER TABLE audit_log DROP CONSTRAINT IF EXISTS ck_audit_action;
ALTER TABLE recurring_transactions DROP CONSTRAINT IF EXISTS ck_rec_dates;
ALTER TABLE recurring_transactions DROP CONSTRAINT IF EXISTS ck_rec_interval;
ALTER TABLE recurring_transactions DROP CONSTRAINT IF EXISTS ck_rec_unit;
ALTER TABLE recurring_transactions DROP CONSTRAINT IF EXISTS ck_rec_amount;
ALTER TABLE transactions DROP CONSTRAINT IF EXISTS ck_tx_recurring_due;
ALTER TABLE transactions DROP CONSTRAINT IF EXISTS ck_tx_planned;
ALTER TABLE transactions DROP CONSTRAINT IF EXISTS ck_tx_amount;
ALTER TABLE budgets DROP CONSTRAINT IF EXISTS ck_budget_amount;
ALTER TABLE budgets DROP CONSTRAINT IF EXISTS ck_budget_first_of_month;
ALTER TABLE budgets DROP CONSTRAINT IF EXISTS ck_budget_scope_category;
ALTER TABLE budgets DROP CONSTRAINT IF EXISTS ck_budget_scope;
ALTER TABLE budgets DROP CONSTRAINT IF EXISTS ck_budget_type;
ALTER TABLE categories DROP CONSTRAINT IF EXISTS ck_categories_type;
ALTER TABLE privacy_requests DROP CONSTRAINT IF EXISTS ck_privacy_status;
ALTER TABLE privacy_requests DROP CONSTRAINT IF EXISTS ck_privacy_type;
ALTER TABLE user_settings DROP CONSTRAINT IF EXISTS ck_settings_currency;
