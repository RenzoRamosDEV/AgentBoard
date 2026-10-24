-- Coste, ahorro por caché y «tiene precio» de cada llamada, guardados con el precio vigente en su
-- fecha (misma fórmula y orden de operaciones que la vista de 0004, en `derived.rs` y en
-- `pricing::PriceBook`). El escritor de la ingesta los calcula antes de insertar. Una llamada que
-- cambia de valores o un cambio de precios los deja pendientes (`has_price` nulo) y
-- `derived::refresh` los recalcula al final de la misma transacción, solo para esas filas. No hay
-- disparadores de inserción en `calls`: harían que cada inserción del escritor abriera un
-- subdiario de sentencia. Sin índice por modelo: un cambio de precios (raro) recorre `calls`.

ALTER TABLE calls ADD COLUMN has_price INTEGER;
ALTER TABLE calls ADD COLUMN cost_usd REAL NOT NULL DEFAULT 0;
ALTER TABLE calls ADD COLUMN cache_savings_usd REAL NOT NULL DEFAULT 0;
CREATE INDEX calls_cost_pending ON calls(has_price) WHERE has_price IS NULL;

CREATE TRIGGER calls_cost_stale
AFTER UPDATE OF ts, model, input_tokens, output_tokens, cache_read, cache_write, cache_write_1h, cost_reported
ON calls
WHEN NEW.ts IS NOT OLD.ts OR NEW.model IS NOT OLD.model OR NEW.input_tokens IS NOT OLD.input_tokens
  OR NEW.output_tokens IS NOT OLD.output_tokens OR NEW.cache_read IS NOT OLD.cache_read
  OR NEW.cache_write IS NOT OLD.cache_write OR NEW.cache_write_1h IS NOT OLD.cache_write_1h
  OR NEW.cost_reported IS NOT OLD.cost_reported
BEGIN
  UPDATE calls SET has_price = NULL WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER prices_cost_insert AFTER INSERT ON prices BEGIN
  UPDATE calls SET has_price = NULL WHERE model = NEW.model;
END;
CREATE TRIGGER prices_cost_delete AFTER DELETE ON prices BEGIN
  UPDATE calls SET has_price = NULL WHERE model = OLD.model;
END;
CREATE TRIGGER prices_cost_update AFTER UPDATE ON prices BEGIN
  UPDATE calls SET has_price = NULL WHERE model IN (OLD.model, NEW.model);
END;

DROP VIEW call_costs;
CREATE VIEW call_costs AS
SELECT message_id, session_id, ts, model, input_tokens, output_tokens, cache_read, cache_write,
  cache_write_1h, reasoning_tokens, activity, turn_id, is_sidechain, agent_id, cost_reported,
  has_price, cost_usd, cache_savings_usd
FROM calls;
