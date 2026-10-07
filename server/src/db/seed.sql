-- Demo rows for local development. This user has no password and cannot sign in.
BEGIN;

DELETE FROM users WHERE id = '00000000-0000-4000-8000-000000000001'::uuid;
DELETE FROM audit_log WHERE user_id = '00000000-0000-4000-8000-000000000001'::uuid;

INSERT INTO users (id, name, email, email_verified)
VALUES ('00000000-0000-4000-8000-000000000001'::uuid, 'Demo', 'demo@example.com', true);

INSERT INTO user_settings (user_id, currency, locale, timezone)
VALUES ('00000000-0000-4000-8000-000000000001'::uuid, 'EUR', 'en', 'Europe/Amsterdam');

WITH housing AS (
  INSERT INTO category_groups (user_id, name)
  VALUES ('00000000-0000-4000-8000-000000000001'::uuid, 'Housing')
  RETURNING id
), rent AS (
  INSERT INTO categories (user_id, category_group_id, type, name)
  SELECT '00000000-0000-4000-8000-000000000001'::uuid, housing.id, 'EXPENSE', 'Rent'
  FROM housing
  RETURNING id
), food AS (
  INSERT INTO categories (user_id, type, name)
  VALUES ('00000000-0000-4000-8000-000000000001'::uuid, 'EXPENSE', 'Food')
  RETURNING id
), salary AS (
  INSERT INTO categories (user_id, type, name)
  VALUES ('00000000-0000-4000-8000-000000000001'::uuid, 'INCOME', 'Salary')
  RETURNING id
), global_budget AS (
  INSERT INTO budgets (user_id, type, scope, amount_limit, effective_from)
  VALUES (
    '00000000-0000-4000-8000-000000000001'::uuid,
    'EXPENSE',
    'GLOBAL',
    2000.00,
    DATE '2026-03-01'
  )
  RETURNING id
), rent_budget AS (
  INSERT INTO budgets (user_id, type, scope, category_id, amount_limit, effective_from)
  SELECT
    '00000000-0000-4000-8000-000000000001'::uuid,
    'EXPENSE',
    'CATEGORY',
    rent.id,
    800.00,
    DATE '2026-03-01'
  FROM rent
  RETURNING id
)
INSERT INTO transactions (user_id, category_id, name, amount, payment_date)
SELECT
  '00000000-0000-4000-8000-000000000001'::uuid,
  rent.id,
  'March rent',
  800.00,
  DATE '2026-03-02'
FROM rent
JOIN global_budget ON true
JOIN rent_budget ON true
UNION ALL
SELECT
  '00000000-0000-4000-8000-000000000001'::uuid,
  food.id,
  'Groceries',
  42.50,
  DATE '2026-03-04'
FROM food
UNION ALL
SELECT
  '00000000-0000-4000-8000-000000000001'::uuid,
  salary.id,
  'March salary',
  3200.00,
  DATE '2026-03-01'
FROM salary;

COMMIT;
