-- Tables, indexes and foreign keys. Ported from schema v3 (Postgres) with user ids, auth,
-- consent/privacy tables and category groups removed: one local profile.
--
-- Conventions:
--   * Money is integer minor units (cents) in *_minor columns. Never floats.
--   * Dates are 'YYYY-MM-DD' text; timestamps are ISO 8601 UTC text.
--   * Ids are INTEGER PRIMARY KEY (rowid aliases).
-- CHECK constraints, partial unique indexes and triggers come in later migrations.

CREATE TABLE settings (
  id         INTEGER PRIMARY KEY,
  currency   TEXT NOT NULL,
  locale     TEXT NOT NULL DEFAULT 'en',
  timezone   TEXT NOT NULL DEFAULT 'UTC',
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE categories (
  id         INTEGER PRIMARY KEY,
  type       TEXT NOT NULL,
  name       TEXT NOT NULL,
  deleted_at TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- Target of the composite foreign key from budgets (budget type must match category type).
CREATE UNIQUE INDEX uq_categories_id_type ON categories (id, type);
CREATE INDEX idx_categories_active ON categories (deleted_at);

CREATE TABLE budgets (
  id               INTEGER PRIMARY KEY,
  type             TEXT NOT NULL,
  scope            TEXT NOT NULL,
  category_id      INTEGER,
  amount_limit_minor INTEGER,
  effective_from   TEXT NOT NULL,
  created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT fk_budgets_category_type
    FOREIGN KEY (category_id, type) REFERENCES categories (id, type) ON DELETE CASCADE
) STRICT;

-- GLOBAL rows have NULL category_id and are covered by a partial index later.
CREATE UNIQUE INDEX uq_budget_category_month ON budgets (category_id, effective_from);
CREATE INDEX idx_budget_lookup ON budgets (type, scope, category_id, effective_from);
CREATE INDEX idx_budgets_category ON budgets (category_id);

CREATE TABLE recurring_transactions (
  id             INTEGER PRIMARY KEY,
  category_id    INTEGER NOT NULL REFERENCES categories (id) ON DELETE NO ACTION,
  name           TEXT NOT NULL,
  description    TEXT,
  amount_minor   INTEGER NOT NULL,
  start_date     TEXT NOT NULL,
  interval_unit  TEXT NOT NULL,
  interval_count INTEGER NOT NULL DEFAULT 1,
  next_due_date  TEXT NOT NULL,
  end_date       TEXT,
  is_active      INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
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
  amount_minor             INTEGER NOT NULL,
  planned_amount_minor     INTEGER,
  payment_date             TEXT NOT NULL,
  description              TEXT,
  created_at               TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at               TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
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
  action     TEXT NOT NULL,
  old_values TEXT,
  new_values TEXT,
  changed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX idx_audit_time ON audit_log (changed_at DESC);
CREATE INDEX idx_audit_entity ON audit_log (entity_type, entity_id, changed_at DESC);
