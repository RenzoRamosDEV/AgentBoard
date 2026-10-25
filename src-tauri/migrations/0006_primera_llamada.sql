-- Primera llamada principal de cada sesión (menor `ts`, desempate por orden de escritura), guardada
-- en la sesión y recalculada por `derived::refresh` solo para las sesiones que cambiaron: las que
-- anota el escritor al insertar, las de filas pendientes (`has_price` nulo) y las que anotan los
-- disparadores de actualización y borrado.

ALTER TABLE sessions ADD COLUMN first_call_id TEXT;
CREATE INDEX calls_first ON calls(session_id, is_sidechain, ts);
-- Prefijo del anterior: cada inserción mantendría un índice más sin que lo use ninguna consulta.
DROP INDEX calls_session;

CREATE TABLE dirty_sessions (session_id TEXT PRIMARY KEY) WITHOUT ROWID;

-- Sustituye al de 0005: además de dejar pendiente el coste, anota la sesión de antes.
DROP TRIGGER calls_cost_stale;
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
  UPDATE calls SET has_price = NULL WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER calls_deleted AFTER DELETE ON calls BEGIN
  INSERT OR IGNORE INTO dirty_sessions VALUES (OLD.session_id);
END;

INSERT INTO dirty_sessions SELECT id FROM sessions;
