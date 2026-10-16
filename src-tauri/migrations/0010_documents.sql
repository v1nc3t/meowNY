-- Documents (phase 9, see docs/documents-design.md).
--
-- A document is a file kept next to the database (`documents/`, sealed; see src/documents/seal.rs). The database holds only what is
-- needed to list, find, link and check it: a name, a type, the size and the SHA-256 of the contents, the date of the paper and
-- the "keep on phone" switch. Where the file lives is not stored: its name is worked out from `sync_id` and the key.
--
-- The contents never change. To change a file, delete the document and add another, so one `sync_id` always means the same bytes
-- on every device and a hash in a row is something a receiver can check.
--
-- Rows are shared with the phone like the other tables (a random `sync_id`, a `stamp` made by triggers, a marker when a row is
-- deleted for good); the sync itself learns the two tables in a later batch of the phase. The triggers follow 0008 and 0009: stamps
-- are made by `UPDATE OF` triggers on the content columns, and stand aside while `sync_clock.applying` is on.

-- The markers of hard-deleted rows get two more kinds. SQLite cannot loosen a CHECK in place, so the table is rebuilt. The three
-- triggers that write to it go first and come back below, unchanged, because a table that triggers name must not be dropped from
-- under them. (The runner turns foreign keys off around a migration; nothing refers to this table.)

DROP TRIGGER trg_budgets_sync_delete;
DROP TRIGGER trg_recurring_transactions_sync_delete;
DROP TRIGGER trg_transactions_sync_delete;

CREATE TABLE sync_deleted_new (
  id      INTEGER PRIMARY KEY,
  sync_id TEXT NOT NULL CONSTRAINT ck_sync_deleted_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  kind    TEXT NOT NULL CONSTRAINT ck_sync_deleted_kind CHECK (kind IN ('budgets', 'recurring_transactions', 'transactions', 'documents', 'document_links')),
  stamp   TEXT NOT NULL
) STRICT;
INSERT INTO sync_deleted_new (id, sync_id, kind, stamp) SELECT id, sync_id, kind, stamp FROM sync_deleted;
DROP TABLE sync_deleted;
ALTER TABLE sync_deleted_new RENAME TO sync_deleted;
CREATE UNIQUE INDEX uq_sync_deleted_sync_id ON sync_deleted (sync_id);

CREATE TRIGGER trg_budgets_sync_delete
AFTER DELETE ON budgets
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'budgets', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

-- As in 0009: the BEFORE DELETE trigger (trg_recurring_transactions_sync_unlink) has already ticked the clock for the payments.
CREATE TRIGGER trg_recurring_transactions_sync_delete
AFTER DELETE ON recurring_transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'recurring_transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_transactions_sync_delete
AFTER DELETE ON transactions
WHEN OLD.sync_id IS NOT NULL AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'transactions', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

-- documents ---------------------------------------------------------------
CREATE TABLE documents (
  id            INTEGER PRIMARY KEY,
  sync_id       TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_documents_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp         TEXT,
  name          TEXT NOT NULL CONSTRAINT ck_documents_name CHECK (length(name) BETWEEN 1 AND 120),
  media_type    TEXT NOT NULL CONSTRAINT ck_documents_media_type CHECK (media_type IN (
    'image/jpeg', 'image/png', 'image/gif', 'image/webp', 'application/pdf',
    'text/plain', 'text/html', 'text/csv', 'application/octet-stream')),
  -- 1 byte to 25 MiB (MAX_DOCUMENT_BYTES)
  size_bytes    INTEGER NOT NULL CONSTRAINT ck_documents_size CHECK (size_bytes BETWEEN 1 AND 26214400),
  sha256        TEXT NOT NULL CONSTRAINT ck_documents_sha256 CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-f]*'),
  document_date TEXT NOT NULL CONSTRAINT ck_documents_date
    CHECK (document_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]' AND date(document_date) IS document_date),
  keep_on_phone INTEGER NOT NULL DEFAULT 0 CONSTRAINT ck_documents_keep CHECK (keep_on_phone IN (0, 1)),
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE UNIQUE INDEX uq_documents_sync_id ON documents (sync_id);
CREATE INDEX idx_documents_date ON documents (document_date DESC, id DESC);
-- adding a file that is already in the library finds it by its hash
CREATE INDEX idx_documents_sha256 ON documents (sha256);

-- document_links ---------------------------------------------------------------
-- Real foreign keys, so the database keeps the links consistent by itself: deleting a transaction or a recurring item removes
-- its links and never the document. Phase 10 adds a third target (a ledger entry) by rebuilding this table.
CREATE TABLE document_links (
  id                       INTEGER PRIMARY KEY,
  sync_id                  TEXT NOT NULL DEFAULT (lower(hex(randomblob(16))))
    CONSTRAINT ck_document_links_sync_id CHECK (length(sync_id) = 32 AND sync_id NOT GLOB '*[^0-9a-f]*'),
  stamp                    TEXT,
  document_id              INTEGER NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
  transaction_id           INTEGER REFERENCES transactions (id) ON DELETE CASCADE,
  recurring_transaction_id INTEGER REFERENCES recurring_transactions (id) ON DELETE CASCADE,
  created_at               TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  CONSTRAINT ck_document_links_one_target CHECK ((transaction_id IS NOT NULL) + (recurring_transaction_id IS NOT NULL) = 1)
) STRICT;

CREATE UNIQUE INDEX uq_document_links_sync_id ON document_links (sync_id);
CREATE UNIQUE INDEX uq_document_links_transaction ON document_links (document_id, transaction_id) WHERE transaction_id IS NOT NULL;
CREATE UNIQUE INDEX uq_document_links_recurring ON document_links (document_id, recurring_transaction_id) WHERE recurring_transaction_id IS NOT NULL;
-- the documents of one transaction or one recurring item
CREATE INDEX idx_document_links_transaction ON document_links (transaction_id) WHERE transaction_id IS NOT NULL;
CREATE INDEX idx_document_links_recurring ON document_links (recurring_transaction_id) WHERE recurring_transaction_id IS NOT NULL;

-- Nothing about a file or a link may change after it is made, except what a person edits (name, date, switch). The first thing a
-- damaged or hostile row from another device would do is name other contents under an id that already means something.
CREATE TRIGGER trg_documents_contents_immutable
BEFORE UPDATE OF sync_id, media_type, size_bytes, sha256 ON documents
WHEN NEW.sync_id IS NOT OLD.sync_id OR NEW.media_type IS NOT OLD.media_type OR NEW.size_bytes IS NOT OLD.size_bytes OR NEW.sha256 IS NOT OLD.sha256
BEGIN
  SELECT RAISE(ABORT, 'the contents of a document never change');
END;

CREATE TRIGGER trg_document_links_immutable
BEFORE UPDATE OF sync_id, document_id, transaction_id, recurring_transaction_id ON document_links
WHEN NEW.sync_id IS NOT OLD.sync_id OR NEW.document_id IS NOT OLD.document_id OR NEW.transaction_id IS NOT OLD.transaction_id OR NEW.recurring_transaction_id IS NOT OLD.recurring_transaction_id
BEGIN
  SELECT RAISE(ABORT, 'a link is made or removed, never changed');
END;

-- updated_at, as for every table (see 0009): not while a sync or a restore is writing.
CREATE TRIGGER trg_documents_updated_at
AFTER UPDATE OF id, name, media_type, size_bytes, sha256, document_date, keep_on_phone, created_at, updated_at ON documents
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE documents SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;

-- A new row gets its stamp. A row that arrives from a sync or a restore has one already (and its sync_id: the column has a default).
CREATE TRIGGER trg_documents_sync_insert
AFTER INSERT ON documents
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE documents SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_document_links_sync_insert
AFTER INSERT ON document_links
WHEN NEW.stamp IS NULL
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE document_links SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- What a person edits gives the row a new stamp (the immutable columns are not listed: they cannot change). A statement that names
-- a column and gives it the value it has is not an edit, so it makes no stamp (the settings trigger of 0008 does the same).
CREATE TRIGGER trg_documents_sync_update
AFTER UPDATE OF name, document_date, keep_on_phone ON documents
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
  AND (NEW.name IS NOT OLD.name OR NEW.document_date IS NOT OLD.document_date OR NEW.keep_on_phone IS NOT OLD.keep_on_phone)
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE documents SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

-- A deleted row leaves a marker with a stamp. A link removed by the foreign key (its document, transaction or recurring item
-- was deleted) leaves one too.
CREATE TRIGGER trg_documents_sync_delete
AFTER DELETE ON documents
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'documents', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

CREATE TRIGGER trg_document_links_sync_delete
AFTER DELETE ON document_links
WHEN (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  INSERT OR REPLACE INTO sync_deleted (sync_id, kind, stamp) VALUES (OLD.sync_id, 'document_links', (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1));
END;

-- The history (audit_log): adding, renaming, re-dating, switching and deleting a document, and making and removing a link. It
-- holds the name and never the contents, and not the hash either.
CREATE TRIGGER trg_documents_audit_insert
AFTER INSERT ON documents
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('documents', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'name', NEW.name, 'media_type', NEW.media_type, 'size_bytes', NEW.size_bytes, 'document_date', NEW.document_date, 'keep_on_phone', NEW.keep_on_phone, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_documents_audit_update
AFTER UPDATE ON documents
WHEN OLD.id IS NOT NEW.id OR OLD.name IS NOT NEW.name OR OLD.document_date IS NOT NEW.document_date OR OLD.keep_on_phone IS NOT NEW.keep_on_phone OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('documents', NEW.id, 'UPDATE',
    json_object('id', OLD.id, 'name', OLD.name, 'media_type', OLD.media_type, 'size_bytes', OLD.size_bytes, 'document_date', OLD.document_date, 'keep_on_phone', OLD.keep_on_phone, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at),
    json_object('id', NEW.id, 'name', NEW.name, 'media_type', NEW.media_type, 'size_bytes', NEW.size_bytes, 'document_date', NEW.document_date, 'keep_on_phone', NEW.keep_on_phone, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_documents_audit_delete
AFTER DELETE ON documents
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('documents', OLD.id, 'DELETE', json_object('id', OLD.id, 'name', OLD.name, 'media_type', OLD.media_type, 'size_bytes', OLD.size_bytes, 'document_date', OLD.document_date, 'keep_on_phone', OLD.keep_on_phone, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;

CREATE TRIGGER trg_document_links_audit_insert
AFTER INSERT ON document_links
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('document_links', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'document_id', NEW.document_id, 'transaction_id', NEW.transaction_id, 'recurring_transaction_id', NEW.recurring_transaction_id, 'created_at', NEW.created_at));
END;

CREATE TRIGGER trg_document_links_audit_delete
AFTER DELETE ON document_links
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('document_links', OLD.id, 'DELETE', json_object('id', OLD.id, 'document_id', OLD.document_id, 'transaction_id', OLD.transaction_id, 'recurring_transaction_id', OLD.recurring_transaction_id, 'created_at', OLD.created_at), NULL);
END;

-- Scheduled backups remember whether to take the documents along (it can be a lot of data; off until the person says so).
ALTER TABLE backup_schedule ADD COLUMN include_documents INTEGER NOT NULL DEFAULT 0
  CONSTRAINT ck_backup_schedule_documents CHECK (include_documents IN (0, 1));
