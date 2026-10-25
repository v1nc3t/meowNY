-- The order of the categories, chosen by the person (the category list can be reordered by hand).
--
-- `position` counts from 0 inside one type, and the lists sort by `position, id` (the id only settles a tie, which a sync can make: two
-- devices that each moved categories). The categories that exist keep the order they have today, which is the order of their ids; a new
-- category goes to the end of its type.
--
-- A move is a change of the row like any other: the two triggers that list the columns they watch (the sync stamp, 0008, and `updated_at`,
-- 0009) are made again with `position` among them, their bodies as they were. They are made after the order is filled in, so that filling
-- it in is not an edit. The audit trigger is left alone: it logs what a category is, and does not list `merged_into` either.

ALTER TABLE categories ADD COLUMN position INTEGER NOT NULL DEFAULT 0;

UPDATE categories SET position = (SELECT COUNT(*) FROM categories c2 WHERE c2.type = categories.type AND c2.id < categories.id);

DROP TRIGGER trg_categories_sync_update;
DROP TRIGGER trg_categories_updated_at;

CREATE TRIGGER trg_categories_sync_update
AFTER UPDATE OF type, name, deleted_at, merged_into, position ON categories
WHEN NEW.stamp IS OLD.stamp AND NEW.sync_id IS OLD.sync_id AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE sync_clock SET
    last_counter = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN 0 WHEN last_counter >= 65535 THEN 0 ELSE last_counter + 1 END,
    last_ms = CASE WHEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) > last_ms THEN (CAST(strftime('%s', 'now') AS INTEGER) * 1000 + CAST(substr(strftime('%f', 'now'), 4) AS INTEGER)) WHEN last_counter >= 65535 THEN last_ms + 1 ELSE last_ms END
  WHERE id = 1;
  UPDATE categories SET stamp = (SELECT printf('%013d-%04x-%s', last_ms, last_counter, substr(device_id, 1, 8)) FROM sync_clock WHERE id = 1) WHERE id = NEW.id;
END;

CREATE TRIGGER trg_categories_updated_at
AFTER UPDATE OF id, type, name, deleted_at, created_at, updated_at, position ON categories
WHEN NEW.updated_at IS OLD.updated_at AND (SELECT applying FROM sync_clock WHERE id = 1) = 0
BEGIN
  UPDATE categories SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = NEW.id;
END;
