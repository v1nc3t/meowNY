-- The language the app is shown in. It is separate from `locale`, which only decides how numbers
-- and dates are written, so someone can read the app in one language and use another country's formats.

ALTER TABLE settings ADD COLUMN language TEXT NOT NULL DEFAULT 'en'
  CONSTRAINT ck_settings_language CHECK (length(language) BETWEEN 2 AND 35);

-- The audit triggers list their columns, so they are recreated to include the new one.
DROP TRIGGER trg_settings_audit_insert;
DROP TRIGGER trg_settings_audit_update;
DROP TRIGGER trg_settings_audit_delete;

CREATE TRIGGER trg_settings_audit_insert
AFTER INSERT ON settings
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('settings', NEW.id, 'INSERT', NULL, json_object('id', NEW.id, 'currency', NEW.currency, 'locale', NEW.locale, 'timezone', NEW.timezone, 'language', NEW.language, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_settings_audit_update
AFTER UPDATE ON settings
WHEN OLD.id IS NOT NEW.id OR OLD.currency IS NOT NEW.currency OR OLD.locale IS NOT NEW.locale OR OLD.timezone IS NOT NEW.timezone OR OLD.language IS NOT NEW.language OR OLD.created_at IS NOT NEW.created_at
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('settings', NEW.id, 'UPDATE',
    json_object('id', OLD.id, 'currency', OLD.currency, 'locale', OLD.locale, 'timezone', OLD.timezone, 'language', OLD.language, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at),
    json_object('id', NEW.id, 'currency', NEW.currency, 'locale', NEW.locale, 'timezone', NEW.timezone, 'language', NEW.language, 'created_at', NEW.created_at, 'updated_at', NEW.updated_at));
END;

CREATE TRIGGER trg_settings_audit_delete
AFTER DELETE ON settings
BEGIN
  INSERT INTO audit_log (entity_type, entity_id, action, old_values, new_values)
  VALUES ('settings', OLD.id, 'DELETE', json_object('id', OLD.id, 'currency', OLD.currency, 'locale', OLD.locale, 'timezone', OLD.timezone, 'language', OLD.language, 'created_at', OLD.created_at, 'updated_at', OLD.updated_at), NULL);
END;
