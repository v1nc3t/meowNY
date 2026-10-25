-- The dates of a recurring item that were skipped one by one (a due payment of the list can be passed over without the ones before it).
--
-- `next_due_date` stays the oldest occurrence that is neither paid nor skipped. A later occurrence that is paid has its transaction (the
-- unique `recurring_due_date`, 0001); one that is skipped has no transaction, so its date is kept here: a JSON array of 'YYYY-MM-DD', sorted,
-- without repeats, and only dates after `next_due_date` (the item drops a date when it moves past it). Items that exist have none.
--
-- A skip is a change of the row like any other: the two triggers that list the columns they watch (the sync stamp, 0008, and `updated_at`,
-- 0009) are made again with `skipped_dates` among them, their bodies as 0012 left them. The audit trigger is left alone, as in 0014.

ALTER TABLE recurring_transactions ADD COLUMN skipped_dates TEXT NOT NULL DEFAULT '[]';

DROP TRIGGER trg_recurring_transactions_updated_at;
DROP TRIGGER trg_recurring_transactions_sync_update;

CREATE TRIGGER trg_recurring_transactions_updated_at
AFTER UPDATE OF id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, skipped_dates, end_date, is_active, split_my_weight, created_at, updated_at ON recurring_transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_sync_update
AFTER UPDATE OF category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, skipped_dates, end_date, is_active, split_my_weight ON recurring_transactions
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE recurring_transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;
