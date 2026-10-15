-- Two things found while testing sync (phase 8). First, the updated_at triggers, again (they were made in 0002 and 0008), with two changes:
--
--   * They stand aside while `sync_clock.applying` is on. A sync, a restore and a currency conversion write rows that
--     come with the right updated_at (or whose updated_at is not a change at all), and two devices that each stamp "now"
--     into the same row would never hold equal rows.
--   * A transaction's link to its recurring item (`recurring_transaction_id`) no longer counts: deleting the item clears
--     it on both devices by itself (ON DELETE SET NULL), and that is not an edit of the transaction.

DROP TRIGGER trg_settings_updated_at;
DROP TRIGGER trg_categories_updated_at;
DROP TRIGGER trg_budgets_updated_at;
DROP TRIGGER trg_recurring_transactions_updated_at;
DROP TRIGGER trg_transactions_updated_at;

CREATE TRIGGER trg_settings_updated_at
AFTER UPDATE OF id, currency, locale, timezone, created_at, updated_at, language ON settings
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE settings SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_categories_updated_at
AFTER UPDATE OF id, type, name, deleted_at, created_at, updated_at ON categories
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE categories SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_budgets_updated_at
AFTER UPDATE OF id, type, scope, category_id, amount_limit_minor, effective_from, created_at, updated_at ON budgets
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE budgets SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_recurring_transactions_updated_at
AFTER UPDATE OF id, category_id, name, description, amount_minor, start_date, interval_unit, interval_count, next_due_date, end_date, is_active, created_at, updated_at ON recurring_transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE recurring_transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

CREATE TRIGGER trg_transactions_updated_at
AFTER UPDATE OF id, category_id, recurring_due_date, client_uuid, name, amount_minor, planned_amount_minor, payment_date, description, created_at, updated_at ON transactions
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE transactions SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

-- Deleting a recurring item gives the payments that were linked to it the stamp of the deletion marker. Their link is cleared
-- (ON DELETE SET NULL), which is a change to them: if the other device brings the item back with a newer edit, the payments
-- must not be left with a link on one device and without on the other. The marker takes the same stamp (one tick for both).
-- A device that learns of the deletion from a sync does the same (see `markers` in sync/engine.rs).
DROP TRIGGER trg_recurring_transactions_sync_delete;

CREATE TRIGGER trg_recurring_transactions_sync_unlink
BEFORE DELETE ON recurring_transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE transactions SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE recurring_transaction_id = OLD.id;
END;

CREATE TRIGGER trg_recurring_transactions_sync_delete
AFTER DELETE ON recurring_transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'recurring_transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;
