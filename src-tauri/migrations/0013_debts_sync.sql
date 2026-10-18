-- Debts, the sync (phase 10, batch 2).
--
-- A ledger entry that was linked to an expense loses the link when the expense is deleted (the foreign key does it, 0012). That was
-- written as "no stamp", the way a payment loses its link to a recurring item; but 0009 already found that a link cleared with no stamp
-- is not enough: if the other device brings the deleted row back with a newer edit, the entry ends linked on one device and not on the
-- other, and nothing in the lists says so. So, as for the payments of a recurring item, the entries of a deleted expense take the stamp
-- of the deletion marker (one tick for both: the trigger before the delete ticks the clock, the one after it makes the marker with the
-- same stamp). A device that learns of the deletion from a sync does the same (see `markers` in sync/engine.rs).

DROP TRIGGER trg_transactions_sync_delete;

CREATE TRIGGER trg_transactions_sync_unlink
BEFORE DELETE ON transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE ledger_entries SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE transaction_id = OLD.id;
END;

CREATE TRIGGER trg_transactions_sync_delete
AFTER DELETE ON transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;
