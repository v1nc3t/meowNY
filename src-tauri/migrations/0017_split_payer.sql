-- A split bill that a friend paid (docs/debts-design.md, sections 2 to 5).
--
-- Until now the person paid every split bill: the transaction holds the whole bill, the lines hold the other people's parts, your part (the
-- cost) is the amount minus the lines, and every friend's line meant "they owe you". When a friend paid, the bill is the same and so is your
-- part, but the debts turn round: you owe the payer your part, and the other friends on the bill owe the payer, not you.
--
-- `transactions.paid_by` is the friend who paid (NULL: you), and `recurring_transactions.split_paid_by` is the payer of the template, copied
-- into each payment. The payer's own part is a line like any other and makes no debt (nobody owes themselves). There is one payer for a bill,
-- and a bill with a payer is an expense like any split bill (`splits::check` looks at the finished state; the trigger below stops it here).
--
-- A change of the payer is a change of the row like any other: the two triggers of each table that list the columns they watch (the sync
-- stamp, 0008 and 0016, and `updated_at`, 0009 and 0015) are made again with the new column among them, their bodies as 0015 and 0016 left
-- them. The audit triggers are left alone, as in 0014 and 0015.
--
-- `person_balances` is made again: a line counts for the friend only on a bill you paid, and a bill the friend paid takes your part (its
-- cost) off what they owe you.

ALTER TABLE transactions ADD COLUMN paid_by INTEGER REFERENCES people (id) ON DELETE NO ACTION;
ALTER TABLE recurring_transactions ADD COLUMN split_paid_by INTEGER REFERENCES people (id) ON DELETE NO ACTION;

-- the view of balances looks up the bills a friend paid
CREATE INDEX idx_transactions_paid_by ON transactions (paid_by) WHERE paid_by IS NOT NULL;

DROP TRIGGER trg_transactions_updated_at;
DROP TRIGGER trg_transactions_sync_update;
DROP TRIGGER trg_transactions_split_category;
DROP TRIGGER trg_recurring_transactions_updated_at;
DROP TRIGGER trg_recurring_transactions_sync_update;

CREATE TRIGGER trg_transactions_updated_at
AFTER UPDATE OF id, category_id, recurring_due_date, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, paid_by, created_at, updated_at ON transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_sync_update
AFTER UPDATE OF category_id, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, paid_by ON transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- The bill stays an expense (0012), now also when it has a payer and no parts yet.
CREATE TRIGGER trg_transactions_split_category
BEFORE UPDATE OF category_id ON transactions
WHEN NEW.category_id IS NOT OLD.category_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (EXISTS (SELECT 1 FROM transaction_split_lines WHERE transaction_id = NEW.id) OR NEW.paid_by IS NOT NULL)
  AND (SELECT type FROM categories WHERE id = NEW.category_id) = 'INCOME'
BEGIN
  SELECT RAISE(ABORT, 'only an expense can be split');
END;

CREATE TRIGGER trg_recurring_transactions_updated_at
AFTER UPDATE OF id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, skipped_dates, end_date, is_active, split_my_weight, split_paid_by, created_at, updated_at ON recurring_transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_sync_update
AFTER UPDATE OF category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, skipped_dates, end_date, is_active, split_my_weight, split_paid_by ON recurring_transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER) + offset_ms) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE recurring_transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- Who owes whom (positive: they owe you), as in 0012 with the payer. A part of a bill counts like a LENT entry dated the day of the bill, but
-- only on a bill you paid: on a bill another friend paid, it is a debt to that friend. On a bill this friend paid, your part (its cost) is a
-- debt to them, like a BORROWED entry, and their own part counts for nothing. A repayment is not matched to a bill, so this is a running total.
DROP VIEW person_balances;

CREATE VIEW person_balances AS
SELECT p.id AS person_id,
       COALESCE((SELECT SUM(l.amount_minor) FROM transaction_split_lines l JOIN transactions t ON t.id = l.transaction_id WHERE l.person_id = p.id AND t.paid_by IS NULL), 0)
       - COALESCE((SELECT SUM(k.cost_minor) FROM transaction_costs k JOIN transactions t ON t.id = k.transaction_id WHERE t.paid_by = p.id), 0)
       + COALESCE((SELECT SUM(CASE e.kind WHEN 'LENT' THEN e.amount_minor WHEN 'REPAID_BY_ME' THEN e.amount_minor ELSE -e.amount_minor END)
                   FROM ledger_entries e WHERE e.person_id = p.id), 0) AS balance_minor
FROM people p;
