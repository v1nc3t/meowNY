-- Demo data for development. Dates are relative to today, so the data always looks current.
-- Run through `cargo run --example seed` (see examples/seed.rs). Never part of a release build.
-- Money is integer minor units (cents).

INSERT INTO settings (id, currency, locale, timezone) VALUES (1, 'EUR', 'en-IE', 'Europe/Dublin');

INSERT INTO categories (id, type, name) VALUES
  (1, 'INCOME',  'Salary'),
  (2, 'INCOME',  'Freelance'),
  (3, 'EXPENSE', 'Rent'),
  (4, 'EXPENSE', 'Groceries'),
  (5, 'EXPENSE', 'Transport'),
  (6, 'EXPENSE', 'Eating out'),
  (7, 'EXPENSE', 'Entertainment'),
  (8, 'EXPENSE', 'Subscriptions');

-- Soft-deleted category: its name is free to reuse, its history stays.
INSERT INTO categories (id, type, name, deleted_at) VALUES (9, 'EXPENSE', 'Gym', strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-30 days'));

-- Budgets, effective from two months ago. Later changes would be new rows with a later effective_from.
INSERT INTO budgets (type, scope, category_id, amount_limit_minor, effective_from) VALUES
  ('EXPENSE', 'GLOBAL',   NULL, 220000, date('now', 'start of month', '-2 months')),
  ('INCOME',  'GLOBAL',   NULL, 350000, date('now', 'start of month', '-2 months')),
  ('EXPENSE', 'CATEGORY', 3,    90000,  date('now', 'start of month', '-2 months')),
  ('EXPENSE', 'CATEGORY', 4,    45000,  date('now', 'start of month', '-2 months')),
  ('EXPENSE', 'CATEGORY', 6,    20000,  date('now', 'start of month', '-2 months')),
  ('EXPENSE', 'CATEGORY', 7,    NULL,   date('now', 'start of month', '-2 months')),
  -- Groceries limit raised last month.
  ('EXPENSE', 'CATEGORY', 4,    50000,  date('now', 'start of month', '-1 month'));

INSERT INTO recurring_transactions
  (id, category_id, name, amount_minor, start_date, interval_unit, interval_count, next_due_date) VALUES
  (1, 3, 'Rent',    90000, date('now', 'start of month', '-2 months'), 'MONTH', 1, date('now', 'start of month', '+1 month')),
  (2, 8, 'Netflix', 1299,  date('now', 'start of month', '-2 months'), 'MONTH', 1, date('now', 'start of month', '+1 month'));

INSERT INTO transactions
  (category_id, recurring_transaction_id, recurring_due_date, name, amount_minor, planned_amount_minor, payment_date, description) VALUES
  (1, NULL, NULL, 'Salary', 320000, NULL, date('now', 'start of month', '-2 months', '+0 days'), 'Monthly pay'),
  (3, 1, date('now', 'start of month', '-2 months'), 'Rent', 90000, 90000, date('now', 'start of month', '-2 months', '+0 days'), NULL),
  (8, 2, date('now', 'start of month', '-2 months'), 'Netflix', 1299, 1299, date('now', 'start of month', '-2 months', '+2 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 8420, NULL, date('now', 'start of month', '-2 months', '+1 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 6150, NULL, date('now', 'start of month', '-2 months', '+8 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 9780, NULL, date('now', 'start of month', '-2 months', '+15 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 7310, NULL, date('now', 'start of month', '-2 months', '+22 days'), NULL),
  (5, NULL, NULL, 'Monthly transit pass', 6000, NULL, date('now', 'start of month', '-2 months', '+3 days'), NULL),
  (6, NULL, NULL, 'Dinner out', 4850, NULL, date('now', 'start of month', '-2 months', '+6 days'), NULL),
  (7, NULL, NULL, 'Cinema', 1800, NULL, date('now', 'start of month', '-2 months', '+12 days'), NULL),
  (1, NULL, NULL, 'Salary', 320000, NULL, date('now', 'start of month', '-1 months', '+0 days'), 'Monthly pay'),
  (3, 1, date('now', 'start of month', '-1 months'), 'Rent', 90000, 90000, date('now', 'start of month', '-1 months', '+0 days'), NULL),
  (8, 2, date('now', 'start of month', '-1 months'), 'Netflix', 1299, 1299, date('now', 'start of month', '-1 months', '+2 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 8420, NULL, date('now', 'start of month', '-1 months', '+1 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 6150, NULL, date('now', 'start of month', '-1 months', '+8 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 9780, NULL, date('now', 'start of month', '-1 months', '+15 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 7310, NULL, date('now', 'start of month', '-1 months', '+22 days'), NULL),
  (5, NULL, NULL, 'Monthly transit pass', 6000, NULL, date('now', 'start of month', '-1 months', '+3 days'), NULL),
  (6, NULL, NULL, 'Dinner out', 4550, NULL, date('now', 'start of month', '-1 months', '+6 days'), NULL),
  (7, NULL, NULL, 'Cinema', 1800, NULL, date('now', 'start of month', '-1 months', '+12 days'), NULL),
  (1, NULL, NULL, 'Salary', 320000, NULL, date('now', 'start of month', '+0 days'), 'Monthly pay'),
  (3, 1, date('now', 'start of month'), 'Rent', 90000, 90000, date('now', 'start of month', '+0 days'), NULL),
  (8, 2, date('now', 'start of month'), 'Netflix', 1299, 1299, date('now', 'start of month', '+2 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 8420, NULL, date('now', 'start of month', '+1 days'), NULL),
  (4, NULL, NULL, 'Supermarket', 6150, NULL, date('now', 'start of month', '+8 days'), NULL),
  (5, NULL, NULL, 'Monthly transit pass', 6000, NULL, date('now', 'start of month', '+3 days'), NULL),
  (6, NULL, NULL, 'Dinner out', 4250, NULL, date('now', 'start of month', '+6 days'), NULL),
  (2, NULL, NULL, 'Logo design', 45000, NULL, date('now', 'start of month', '-1 months', '+17 days'), 'One-off client job');
