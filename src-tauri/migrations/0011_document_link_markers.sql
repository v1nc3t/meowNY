-- A link needs a deletion marker only when a person removes it (documents-design.md, section 10).
--
-- Migration 0010 made a marker for every link that left the table, including the ones the foreign key removed because their
-- document, transaction or recurring item was deleted. That is wrong for a sync: the parent's own marker already says the link
-- is gone, and a marker for a link that only this device knew would make the two devices disagree about a row the other one
-- cannot even know exists. So a link now leaves a marker only while its parent is still there, that is, when it was unlinked.
-- (When the foreign key removes a link, the parent has already been deleted by the time this trigger runs, so none of the three
-- conditions below holds for it.)

DROP TRIGGER trg_document_links_sync_delete;

CREATE TRIGGER trg_document_links_sync_delete
AFTER DELETE ON document_links
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND EXISTS (SELECT 1 FROM documents WHERE id = OLD.document_id)
  AND (OLD.transaction_id IS NULL OR EXISTS (SELECT 1 FROM transactions WHERE id = OLD.transaction_id))
  AND (OLD.recurring_transaction_id IS NULL OR EXISTS (SELECT 1 FROM recurring_transactions WHERE id = OLD.recurring_transaction_id))
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'document_links', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

-- updated_at follows a real edit, as the stamp does. Migration 0010 made the stamp skip a write that changes nothing but left
-- updated_at to move on it, so two devices that held the same stamp could hold different updated_at (the random test of the sync
-- found it): a write that names a column and gives it the value it has is not an edit, for either.
DROP TRIGGER trg_documents_updated_at;

CREATE TRIGGER trg_documents_updated_at
AFTER UPDATE OF id, name, media_type, size_bytes, sha256, document_date, keep_on_phone, created_at, updated_at ON documents
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.id IS NOT OLD.id OR NEW.name IS NOT OLD.name OR NEW.document_date IS NOT OLD.document_date OR NEW.keep_on_phone IS NOT OLD.keep_on_phone OR NEW.created_at IS NOT OLD.created_at)
BEGIN
  UPDATE documents SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;
