import type { MigrationBuilder } from 'node-pg-migrate';

const auditedTables = [
  'users',
  'user_settings',
  'category_groups',
  'categories',
  'budgets',
  'recurring_transactions',
  'transactions',
];

export function up(pgm: MigrationBuilder): void {
  pgm.addConstraint('user_settings', 'ck_settings_currency', {
    check: "currency ~ '^[A-Z]{3}$'",
  });

  pgm.addConstraint('privacy_requests', 'ck_privacy_type', {
    check: "type IN ('EXPORT', 'ERASURE', 'RECTIFICATION', 'OTHER')",
  });
  pgm.addConstraint('privacy_requests', 'ck_privacy_status', {
    check: "status IN ('PENDING', 'COMPLETED', 'REJECTED')",
  });

  pgm.addConstraint('categories', 'ck_categories_type', {
    check: "type IN ('INCOME', 'EXPENSE')",
  });

  pgm.addConstraint('budgets', 'ck_budget_type', {
    check: "type IN ('INCOME', 'EXPENSE')",
  });
  pgm.addConstraint('budgets', 'ck_budget_scope', {
    check: "scope IN ('GLOBAL', 'CATEGORY')",
  });
  pgm.addConstraint('budgets', 'ck_budget_scope_category', {
    check:
      "(scope = 'GLOBAL' AND category_id IS NULL) OR (scope = 'CATEGORY' AND category_id IS NOT NULL)",
  });
  pgm.addConstraint('budgets', 'ck_budget_first_of_month', {
    check: 'EXTRACT(DAY FROM effective_from) = 1',
  });
  pgm.addConstraint('budgets', 'ck_budget_amount', {
    check: 'amount_limit IS NULL OR amount_limit >= 0',
  });

  pgm.addConstraint('transactions', 'ck_tx_amount', { check: 'amount > 0' });
  pgm.addConstraint('transactions', 'ck_tx_planned', {
    check: 'planned_amount IS NULL OR planned_amount >= 0',
  });
  pgm.addConstraint('transactions', 'ck_tx_recurring_due', {
    check: 'recurring_transaction_id IS NULL OR recurring_due_date IS NOT NULL',
  });

  pgm.addConstraint('recurring_transactions', 'ck_rec_amount', { check: 'amount > 0' });
  pgm.addConstraint('recurring_transactions', 'ck_rec_unit', {
    check: "interval_unit IN ('DAY', 'WEEK', 'MONTH', 'YEAR')",
  });
  pgm.addConstraint('recurring_transactions', 'ck_rec_interval', {
    check: 'interval_count > 0',
  });
  pgm.addConstraint('recurring_transactions', 'ck_rec_dates', {
    check: 'next_due_date >= start_date AND (end_date IS NULL OR end_date >= start_date)',
  });

  pgm.addConstraint('audit_log', 'ck_audit_action', {
    check: "action IN ('INSERT', 'UPDATE', 'DELETE')",
  });

  // Partial unique indexes stay SQL so the WHERE predicate is not rewritten.
  pgm.sql(`
CREATE UNIQUE INDEX uq_categories_active_name
  ON categories (user_id, type, name) WHERE deleted_at IS NULL;

CREATE UNIQUE INDEX uq_budget_global_month
  ON budgets (user_id, type, effective_from) WHERE scope = 'GLOBAL';
`);

  // Triggers and plpgsql stay SQL. createTrigger emits EXECUTE PROCEDURE, and createFunction rewrites the body.
  pgm.sql(`
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
`);
}

export function down(pgm: MigrationBuilder): void {
  for (const table of auditedTables) {
    pgm.dropTrigger(table, 'trg_updated_at', { ifExists: true });
    pgm.dropTrigger(table, 'trg_audit', { ifExists: true });
  }
  pgm.dropTrigger('categories', 'trg_category_type_immutable', { ifExists: true });

  pgm.dropFunction('audit_row_change', [], { ifExists: true });
  pgm.dropFunction('set_updated_at', [], { ifExists: true });
  pgm.dropFunction('prevent_category_type_change', [], { ifExists: true });

  pgm.dropIndex('budgets', ['user_id', 'type', 'effective_from'], {
    name: 'uq_budget_global_month',
    ifExists: true,
  });
  pgm.dropIndex('categories', ['user_id', 'type', 'name'], {
    name: 'uq_categories_active_name',
    ifExists: true,
  });

  pgm.dropConstraint('audit_log', 'ck_audit_action', { ifExists: true });
  pgm.dropConstraint('recurring_transactions', 'ck_rec_dates', { ifExists: true });
  pgm.dropConstraint('recurring_transactions', 'ck_rec_interval', { ifExists: true });
  pgm.dropConstraint('recurring_transactions', 'ck_rec_unit', { ifExists: true });
  pgm.dropConstraint('recurring_transactions', 'ck_rec_amount', { ifExists: true });
  pgm.dropConstraint('transactions', 'ck_tx_recurring_due', { ifExists: true });
  pgm.dropConstraint('transactions', 'ck_tx_planned', { ifExists: true });
  pgm.dropConstraint('transactions', 'ck_tx_amount', { ifExists: true });
  pgm.dropConstraint('budgets', 'ck_budget_amount', { ifExists: true });
  pgm.dropConstraint('budgets', 'ck_budget_first_of_month', { ifExists: true });
  pgm.dropConstraint('budgets', 'ck_budget_scope_category', { ifExists: true });
  pgm.dropConstraint('budgets', 'ck_budget_scope', { ifExists: true });
  pgm.dropConstraint('budgets', 'ck_budget_type', { ifExists: true });
  pgm.dropConstraint('categories', 'ck_categories_type', { ifExists: true });
  pgm.dropConstraint('privacy_requests', 'ck_privacy_status', { ifExists: true });
  pgm.dropConstraint('privacy_requests', 'ck_privacy_type', { ifExists: true });
  pgm.dropConstraint('user_settings', 'ck_settings_currency', { ifExists: true });
}
