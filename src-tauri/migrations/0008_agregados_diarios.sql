-- Agregados por día local, sesión y modelo (llamadas) o herramienta, que `derived::refresh`
-- rehace al final de cada transacción solo para las claves `(sesión, día)` anotadas en
-- `dirty_keys`: las que anota el escritor al insertar (sin disparadores de inserción), las de
-- filas pendientes y las que anotan los disparadores de actualización y borrado. El día 0 anota
-- solo la sesión (filas que aún no tenían día).

CREATE TABLE daily_stats (
  day               INTEGER NOT NULL,
  session_id        TEXT NOT NULL,
  model             TEXT NOT NULL,
  calls             INTEGER NOT NULL,
  cost_usd          REAL NOT NULL,
  cache_savings_usd REAL NOT NULL,
  input_tokens      INTEGER NOT NULL,
  output_tokens     INTEGER NOT NULL,
  cache_read        INTEGER NOT NULL,
  cache_write       INTEGER NOT NULL,
  unpriced_calls    INTEGER NOT NULL,
  first_calls       INTEGER NOT NULL,  -- 1 si la primera llamada de la sesión cae en esta fila
  first_ctx_tokens  INTEGER NOT NULL,  -- contexto (entrada + caché) de esa primera llamada
  min_ts            INTEGER NOT NULL,
  max_ts            INTEGER NOT NULL,
  PRIMARY KEY (day, session_id, model)
) WITHOUT ROWID;
CREATE INDEX daily_stats_session ON daily_stats(session_id);

CREATE TABLE tool_stats (
  day        INTEGER NOT NULL,
  session_id TEXT NOT NULL,
  tool       TEXT NOT NULL,
  uses       INTEGER NOT NULL,
  errors     INTEGER NOT NULL,
  min_ts     INTEGER NOT NULL,
  PRIMARY KEY (day, session_id, tool)
) WITHOUT ROWID;
CREATE INDEX tool_stats_session ON tool_stats(session_id);

CREATE TABLE dirty_keys (
  session_id TEXT NOT NULL,
  day        INTEGER NOT NULL,
  PRIMARY KEY (session_id, day)
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
  INSERT OR IGNORE INTO dirty_keys VALUES (OLD.session_id, IFNULL(OLD.day, 0));
  UPDATE calls SET has_price = NULL,
    day = CASE WHEN NEW.ts IS NOT OLD.ts THEN NULL ELSE day END
  WHERE rowid = NEW.rowid;
END;

DROP TRIGGER calls_deleted;
CREATE TRIGGER calls_deleted AFTER DELETE ON calls BEGIN
  INSERT OR IGNORE INTO dirty_keys VALUES (OLD.session_id, IFNULL(OLD.day, 0));
END;

-- Sin `is_error`: el escritor lo actualiza en cada resultado de herramienta y anota él la clave
-- (un disparador haría que esa sentencia abriera un subdiario en cada resultado).
DROP TRIGGER tool_calls_ts;
CREATE TRIGGER tool_calls_ts
AFTER UPDATE OF ts, session_id, tool ON tool_calls
WHEN NEW.ts IS NOT OLD.ts OR NEW.session_id IS NOT OLD.session_id OR NEW.tool IS NOT OLD.tool
BEGIN
  INSERT OR IGNORE INTO dirty_keys VALUES (OLD.session_id, IFNULL(OLD.day, 0));
  UPDATE tool_calls SET day = NULL WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER tool_calls_deleted AFTER DELETE ON tool_calls BEGIN
  INSERT OR IGNORE INTO dirty_keys VALUES (OLD.session_id, IFNULL(OLD.day, 0));
END;

INSERT OR IGNORE INTO dirty_keys SELECT session_id, day FROM calls WHERE day IS NOT NULL;
INSERT OR IGNORE INTO dirty_keys SELECT session_id, day FROM tool_calls WHERE day IS NOT NULL;
INSERT OR IGNORE INTO dirty_keys SELECT session_id, 0 FROM dirty_sessions;
DROP TABLE dirty_sessions;
