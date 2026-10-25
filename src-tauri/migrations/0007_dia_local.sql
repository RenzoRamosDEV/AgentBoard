-- Día local (`YYYYMMDD` en la zona de la caché, `cache_meta.tz`) de cada llamada y herramienta:
-- lo escribe el escritor al insertar; una fila sin él (SQL directo, `ts` cambiada o zona
-- cambiada) queda pendiente (`day` nulo) y `derived::refresh` lo calcula.

ALTER TABLE calls ADD COLUMN day INTEGER;
ALTER TABLE tool_calls ADD COLUMN day INTEGER;
CREATE INDEX calls_day_pending ON calls(ts) WHERE day IS NULL;
CREATE INDEX tool_calls_day_pending ON tool_calls(ts) WHERE day IS NULL;

CREATE TABLE cache_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

DROP TRIGGER calls_stale;
CREATE TRIGGER calls_stale
AFTER UPDATE OF ts, model, input_tokens, output_tokens, cache_read, cache_write, cache_write_1h,
  cost_reported, is_sidechain, session_id
ON calls
WHEN NEW.ts IS NOT OLD.ts OR NEW.model IS NOT OLD.model OR NEW.input_tokens IS NOT OLD.input_tokens
  OR NEW.output_tokens IS NOT OLD.output_tokens OR NEW.cache_read IS NOT OLD.cache_read
  OR NEW.cache_write IS NOT OLD.cache_write OR NEW.cache_write_1h IS NOT OLD.cache_write_1h
  OR NEW.cost_reported IS NOT OLD.cost_reported OR NEW.is_sidechain IS NOT OLD.is_sidechain
  OR NEW.session_id IS NOT OLD.session_id
BEGIN
  INSERT OR IGNORE INTO dirty_sessions VALUES (OLD.session_id);
  UPDATE calls SET has_price = NULL,
    day = CASE WHEN NEW.ts IS NOT OLD.ts THEN NULL ELSE day END
  WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER tool_calls_ts AFTER UPDATE OF ts ON tool_calls WHEN NEW.ts IS NOT OLD.ts BEGIN
  UPDATE tool_calls SET day = NULL WHERE rowid = NEW.rowid;
END;
