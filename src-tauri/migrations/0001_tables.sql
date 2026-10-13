-- Tables, indexes and foreign keys. Ported from schema v3 (Postgres) with user ids, auth,
-- consent/privacy tables and category groups removed: one local profile.
--
-- Conventions:
--   * Money is integer minor units (cents) in *_minor columns. Never floats.
--   * Dates are 'YYYY-MM-DD' text; timestamps are ISO 8601 UTC text.
--   * Ids are INTEGER PRIMARY KEY (rowid aliases).
-- Money columns are bounded to 0..999_999_999_999 minor units (the old numeric(12,2) range,
-- and well inside JS's safe integer range). Triggers are in 0002_triggers.sql.
-- SQLite cannot add CHECKs to an existing table, so they live in the table definitions.

CREATE TABLE settings (
  id         INTEGER PRIMARY KEY,
  currency   TEXT NOT NULL CONSTRAINT ck_settings_currency CHECK (currency GLOB '[A-Z][A-Z][A-Z]'),
  locale     TEXT NOT NULL DEFAULT 'en',
  timezone   TEXT NOT NULL DEFAULT 'UTC',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE categories (
  id         INTEGER PRIMARY KEY,
  type       TEXT NOT NULL CONSTRAINT ck_categories_type CHECK (type IN ('INCOME', 'EXPENSE')),
  name       TEXT NOT NULL,
  deleted_at TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- Target of the composite foreign key from budgets (budget type must match category type).
CREATE UNIQUE INDEX uq_categories_id_type ON categories (id, type);
CREATE INDEX idx_categories_active ON categories (deleted_at);
CREATE UNIQUE INDEX uq_categories_active_name ON categories (type, name) WHERE deleted_at IS NULL;

CREATE TABLE budgets (
  id               INTEGER PRIMARY KEY,
  type             TEXT NOT NULL CONSTRAINT ck_budget_type CHECK (type IN ('INCOME', 'EXPENSE')),
  scope            TEXT NOT NULL CONSTRAINT ck_budget_scope CHECK (scope IN ('GLOBAL', 'CATEGORY')),
  category_id      INTEGER,
  amount_limit_minor INTEGER CONSTRAINT ck_budget_amount
    CHECK (amount_limit_minor IS NULL OR amount_limit_minor BETWEEN 0 AND 999999999999),
  effective_from   TEXT NOT NULL CONSTRAINT ck_budget_first_of_month
    CHECK (effective_from GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-01' AND date(effective_from) IS effective_from),
  created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_budget_scope_category CHECK (
    (scope = 'GLOBAL' AND category_id IS NULL) OR (scope = 'CATEGORY' AND category_id IS NOT NULL)),
  CONSTRAINT fk_budgets_category_type
    FOREIGN KEY (category_id, type) REFERENCES categories (id, type) ON DELETE CASCADE
) STRICT;

-- GLOBAL rows have NULL category_id, so they are covered by the partial index below.
CREATE UNIQUE INDEX uq_budget_global_month ON budgets (type, effective_from) WHERE scope = 'GLOBAL';
CREATE UNIQUE INDEX uq_budget_category_month ON budgets (category_id, effective_from);
CREATE INDEX idx_budget_lookup ON budgets (type, scope, category_id, effective_from);
CREATE INDEX idx_budgets_category ON budgets (category_id);

CREATE TABLE recurring_transactions (
  id             INTEGER PRIMARY KEY,
  category_id    INTEGER NOT NULL REFERENCES categories (id) ON DELETE NO ACTION,
  name           TEXT NOT NULL,
  description    TEXT,
  amount_minor   INTEGER NOT NULL CONSTRAINT ck_rec_amount CHECK (amount_minor BETWEEN 1 AND 999999999999),
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

CREATE INDEX idx_recurring_due ON recurring_transactions (is_active, next_due_date);
CREATE INDEX idx_recurring_category ON recurring_transactions (category_id);

CREATE TABLE transactions (
  id                       INTEGER PRIMARY KEY,
  category_id              INTEGER NOT NULL REFERENCES categories (id) ON DELETE NO ACTION,
  recurring_transaction_id INTEGER REFERENCES recurring_transactions (id) ON DELETE SET NULL,
  recurring_due_date       TEXT,
  client_uuid              TEXT,
  name                     TEXT NOT NULL,
  amount_minor             INTEGER NOT NULL CONSTRAINT ck_tx_amount CHECK (amount_minor BETWEEN 1 AND 999999999999),
  planned_amount_minor     INTEGER CONSTRAINT ck_tx_planned
    CHECK (planned_amount_minor IS NULL OR planned_amount_minor BETWEEN 0 AND 999999999999),
  payment_date             TEXT NOT NULL,
  description              TEXT,
  created_at               TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at               TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_tx_recurring_due CHECK (recurring_transaction_id IS NULL OR recurring_due_date IS NOT NULL)
) STRICT;

CREATE INDEX idx_tx_date ON transactions (payment_date DESC);
CREATE INDEX idx_tx_category_date ON transactions (category_id, payment_date DESC);
CREATE UNIQUE INDEX uq_tx_recurring_occurrence
  ON transactions (recurring_transaction_id, recurring_due_date);
CREATE UNIQUE INDEX uq_tx_client_uuid ON transactions (client_uuid);

CREATE TABLE audit_log (
  id         INTEGER PRIMARY KEY,
  entity_type TEXT NOT NULL,
  entity_id  TEXT NOT NULL,
  action     TEXT NOT NULL CONSTRAINT ck_audit_action CHECK (action IN ('INSERT', 'UPDATE', 'DELETE')),
  old_values TEXT,
  new_values TEXT,
  changed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX idx_audit_time ON audit_log (changed_at DESC);
CREATE INDEX idx_audit_entity ON audit_log (entity_type, entity_id, changed_at DESC);
