-- Up Migration

CREATE FUNCTION effective_budgets(p_user uuid, p_month date)
RETURNS TABLE (type varchar, scope varchar, category_id bigint, amount_limit numeric)
LANGUAGE sql STABLE AS $$
  SELECT x.type, x.scope, x.category_id, x.amount_limit
  FROM (
    SELECT DISTINCT ON (b.type, b.scope, b.category_id)
           b.type, b.scope, b.category_id, b.amount_limit
    FROM budgets b
    WHERE b.user_id = p_user
      AND b.effective_from <= date_trunc('month', p_month)::date
    ORDER BY b.type, b.scope, b.category_id, b.effective_from DESC
  ) x
  WHERE x.amount_limit IS NOT NULL
$$;

CREATE FUNCTION budget_vs_actual(p_user uuid, p_month date)
RETURNS TABLE (
  type varchar,
  category_id bigint,
  category_name varchar,
  budgeted numeric,
  actual numeric,
  remaining numeric,
  pct_used numeric
)
LANGUAGE sql STABLE AS $$
  WITH act AS (
    SELECT t.category_id, SUM(t.amount) AS actual
    FROM transactions t
    WHERE t.user_id = p_user
      AND t.payment_date >= date_trunc('month', p_month)::date
      AND t.payment_date < date_trunc('month', p_month)::date + INTERVAL '1 month'
    GROUP BY t.category_id
  )
  SELECT c.type, c.id, c.name,
         eb.amount_limit,
         COALESCE(act.actual, 0),
         eb.amount_limit - COALESCE(act.actual, 0),
         ROUND(100 * COALESCE(act.actual, 0) / NULLIF(eb.amount_limit, 0), 1)
  FROM categories c
  LEFT JOIN effective_budgets(p_user, p_month) eb
         ON eb.scope = 'CATEGORY' AND eb.category_id = c.id
  LEFT JOIN act ON act.category_id = c.id
  WHERE c.user_id = p_user
    AND (c.deleted_at IS NULL OR act.category_id IS NOT NULL)
  ORDER BY c.type, c.name
$$;

CREATE FUNCTION category_trend(p_user uuid, p_category bigint, p_from date, p_to date)
RETURNS TABLE (month_start date, budgeted numeric, actual numeric)
LANGUAGE sql STABLE AS $$
  SELECT m::date,
         b.amount_limit,
         COALESCE(a.actual, 0)
  FROM generate_series(date_trunc('month', p_from)::date,
                       date_trunc('month', p_to)::date,
                       INTERVAL '1 month') m
  LEFT JOIN LATERAL (
    SELECT bb.amount_limit
    FROM budgets bb
    WHERE bb.user_id = p_user AND bb.category_id = p_category
      AND bb.effective_from <= m::date
    ORDER BY bb.effective_from DESC
    LIMIT 1
  ) b ON true
  LEFT JOIN LATERAL (
    SELECT SUM(t.amount) AS actual
    FROM transactions t
    WHERE t.user_id = p_user AND t.category_id = p_category
      AND t.payment_date >= m::date
      AND t.payment_date < (m + INTERVAL '1 month')::date
  ) a ON true
  ORDER BY 1
$$;

CREATE FUNCTION monthly_overview(p_user uuid, p_month date)
RETURNS TABLE (
  type varchar,
  budgeted numeric,
  allocated numeric,
  unallocated numeric,
  actual numeric,
  remaining numeric
)
LANGUAGE sql STABLE AS $$
  WITH eb AS (
    SELECT * FROM effective_budgets(p_user, p_month)
  ), act AS (
    SELECT c.type, SUM(t.amount) AS actual
    FROM transactions t
    JOIN categories c ON c.id = t.category_id
    WHERE t.user_id = p_user
      AND t.payment_date >= date_trunc('month', p_month)::date
      AND t.payment_date < date_trunc('month', p_month)::date + INTERVAL '1 month'
    GROUP BY c.type
  )
  SELECT s.type, s.budgeted, s.allocated,
         s.budgeted - s.allocated,
         s.actual,
         s.budgeted - s.actual
  FROM (
    SELECT ty.type,
           (SELECT g.amount_limit FROM eb g
             WHERE g.type = ty.type AND g.scope = 'GLOBAL') AS budgeted,
           (SELECT COALESCE(SUM(a.amount_limit), 0) FROM eb a
             WHERE a.type = ty.type AND a.scope = 'CATEGORY') AS allocated,
           COALESCE((SELECT x.actual FROM act x WHERE x.type = ty.type), 0) AS actual
    FROM (VALUES ('INCOME'::varchar), ('EXPENSE'::varchar)) AS ty(type)
  ) s
$$;

CREATE FUNCTION erase_user(p_user uuid) RETURNS void
LANGUAGE plpgsql AS $$
DECLARE v_email text;
BEGIN
  SELECT email INTO v_email FROM users WHERE id = p_user;

  DELETE FROM verifications
   WHERE identifier = v_email OR value = p_user::text;

  DELETE FROM users WHERE id = p_user;

  DELETE FROM audit_log WHERE user_id = p_user;

  INSERT INTO privacy_requests (user_id, type, status, completed_at, note)
  VALUES (p_user, 'ERASURE', 'COMPLETED', now(), 'account erased');
END $$;

CREATE FUNCTION run_scheduled_erasures(p_grace interval DEFAULT interval '7 days')
RETURNS integer
LANGUAGE plpgsql AS $$
DECLARE r record; n integer := 0;
BEGIN
  FOR r IN SELECT id FROM users
            WHERE deletion_requested_at IS NOT NULL
              AND deletion_requested_at < now() - p_grace
  LOOP
    PERFORM erase_user(r.id);
    n := n + 1;
  END LOOP;
  RETURN n;
END $$;

CREATE FUNCTION export_user_data(p_user uuid) RETURNS jsonb
LANGUAGE sql STABLE AS $$
  SELECT jsonb_build_object(
    'exported_at', now(),
    'profile', (SELECT to_jsonb(u) FROM users u WHERE u.id = p_user),
    'settings', (SELECT to_jsonb(s) FROM user_settings s WHERE s.user_id = p_user),
    'linked_logins', COALESCE((
        SELECT jsonb_agg(jsonb_build_object('provider', a.provider_id, 'linked_at', a.created_at))
        FROM accounts a WHERE a.user_id = p_user), '[]'::jsonb),
    'sessions', COALESCE((
        SELECT jsonb_agg(jsonb_build_object(
          'created_at', x.created_at,
          'expires_at', x.expires_at,
          'ip_address', x.ip_address,
          'user_agent', x.user_agent))
        FROM sessions x WHERE x.user_id = p_user), '[]'::jsonb),
    'consents', COALESCE((SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id)
                          FROM user_consents c WHERE c.user_id = p_user), '[]'::jsonb),
    'category_groups', COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.id)
                                 FROM category_groups g WHERE g.user_id = p_user), '[]'::jsonb),
    'categories', COALESCE((SELECT jsonb_agg(to_jsonb(k) ORDER BY k.id)
                            FROM categories k WHERE k.user_id = p_user), '[]'::jsonb),
    'budgets', COALESCE((SELECT jsonb_agg(to_jsonb(b) ORDER BY b.id)
                         FROM budgets b WHERE b.user_id = p_user), '[]'::jsonb),
    'recurring_transactions', COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id)
                                        FROM recurring_transactions r WHERE r.user_id = p_user), '[]'::jsonb),
    'transactions', COALESCE((SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id)
                              FROM transactions t WHERE t.user_id = p_user), '[]'::jsonb),
    'history', COALESCE((SELECT jsonb_agg(to_jsonb(h) ORDER BY h.id)
                         FROM audit_log h WHERE h.user_id = p_user), '[]'::jsonb)
  )
$$;

-- Down Migration

DROP FUNCTION IF EXISTS export_user_data(uuid);
DROP FUNCTION IF EXISTS run_scheduled_erasures(interval);
DROP FUNCTION IF EXISTS erase_user(uuid);
DROP FUNCTION IF EXISTS monthly_overview(uuid, date);
DROP FUNCTION IF EXISTS category_trend(uuid, bigint, date, date);
DROP FUNCTION IF EXISTS budget_vs_actual(uuid, date);
DROP FUNCTION IF EXISTS effective_budgets(uuid, date);
