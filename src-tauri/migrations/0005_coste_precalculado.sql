-- Coste, ahorro por caché y «tiene precio» de cada llamada, guardados al escribirla con el precio
-- vigente en su fecha (misma fórmula y orden de operaciones que la vista de 0004). Los
-- disparadores los mantienen en todos los caminos de escritura; un cambio de precios recalcula
-- solo las llamadas de ese modelo.

ALTER TABLE calls ADD COLUMN has_price INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calls ADD COLUMN cost_usd REAL NOT NULL DEFAULT 0;
ALTER TABLE calls ADD COLUMN cache_savings_usd REAL NOT NULL DEFAULT 0;
CREATE INDEX calls_model_ts ON calls(model, ts);

-- La fórmula se repite en cada disparador (SQLite no tiene funciones de usuario en SQL); los
-- tests comparan todos los caminos con la fórmula de 0004.
CREATE TRIGGER calls_cost_insert AFTER INSERT ON calls BEGIN
  UPDATE calls SET (has_price, cost_usd, cache_savings_usd) = (
    SELECT p.model IS NOT NULL OR calls.cost_reported IS NOT NULL,
      CASE WHEN p.model IS NOT NULL THEN (
        calls.input_tokens * p.input +
        calls.output_tokens * p.output +
        calls.cache_read * p.cache_read +
        (calls.cache_write - calls.cache_write_1h) * p.cache_write +
        calls.cache_write_1h * p.cache_write_1h
      ) / 1e6 ELSE COALESCE(calls.cost_reported, 0) END,
      COALESCE(calls.cache_read * (p.input - p.cache_read) / 1e6, 0)
    FROM (SELECT 1) LEFT JOIN prices p ON p.model = calls.model
     AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                         WHERE model = calls.model AND valid_from <= calls.ts))
  WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER calls_cost_update
AFTER UPDATE OF ts, model, input_tokens, output_tokens, cache_read, cache_write, cache_write_1h, cost_reported
ON calls BEGIN
  UPDATE calls SET (has_price, cost_usd, cache_savings_usd) = (
    SELECT p.model IS NOT NULL OR calls.cost_reported IS NOT NULL,
      CASE WHEN p.model IS NOT NULL THEN (
        calls.input_tokens * p.input +
        calls.output_tokens * p.output +
        calls.cache_read * p.cache_read +
        (calls.cache_write - calls.cache_write_1h) * p.cache_write +
        calls.cache_write_1h * p.cache_write_1h
      ) / 1e6 ELSE COALESCE(calls.cost_reported, 0) END,
      COALESCE(calls.cache_read * (p.input - p.cache_read) / 1e6, 0)
    FROM (SELECT 1) LEFT JOIN prices p ON p.model = calls.model
     AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                         WHERE model = calls.model AND valid_from <= calls.ts))
  WHERE rowid = NEW.rowid;
END;

CREATE TRIGGER prices_cost_insert AFTER INSERT ON prices BEGIN
  UPDATE calls SET (has_price, cost_usd, cache_savings_usd) = (
    SELECT p.model IS NOT NULL OR calls.cost_reported IS NOT NULL,
      CASE WHEN p.model IS NOT NULL THEN (
        calls.input_tokens * p.input +
        calls.output_tokens * p.output +
        calls.cache_read * p.cache_read +
        (calls.cache_write - calls.cache_write_1h) * p.cache_write +
        calls.cache_write_1h * p.cache_write_1h
      ) / 1e6 ELSE COALESCE(calls.cost_reported, 0) END,
      COALESCE(calls.cache_read * (p.input - p.cache_read) / 1e6, 0)
    FROM (SELECT 1) LEFT JOIN prices p ON p.model = calls.model
     AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                         WHERE model = calls.model AND valid_from <= calls.ts))
  WHERE model = NEW.model;
END;

CREATE TRIGGER prices_cost_delete AFTER DELETE ON prices BEGIN
  UPDATE calls SET (has_price, cost_usd, cache_savings_usd) = (
    SELECT p.model IS NOT NULL OR calls.cost_reported IS NOT NULL,
      CASE WHEN p.model IS NOT NULL THEN (
        calls.input_tokens * p.input +
        calls.output_tokens * p.output +
        calls.cache_read * p.cache_read +
        (calls.cache_write - calls.cache_write_1h) * p.cache_write +
        calls.cache_write_1h * p.cache_write_1h
      ) / 1e6 ELSE COALESCE(calls.cost_reported, 0) END,
      COALESCE(calls.cache_read * (p.input - p.cache_read) / 1e6, 0)
    FROM (SELECT 1) LEFT JOIN prices p ON p.model = calls.model
     AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                         WHERE model = calls.model AND valid_from <= calls.ts))
  WHERE model = OLD.model;
END;

CREATE TRIGGER prices_cost_update AFTER UPDATE ON prices BEGIN
  UPDATE calls SET (has_price, cost_usd, cache_savings_usd) = (
    SELECT p.model IS NOT NULL OR calls.cost_reported IS NOT NULL,
      CASE WHEN p.model IS NOT NULL THEN (
        calls.input_tokens * p.input +
        calls.output_tokens * p.output +
        calls.cache_read * p.cache_read +
        (calls.cache_write - calls.cache_write_1h) * p.cache_write +
        calls.cache_write_1h * p.cache_write_1h
      ) / 1e6 ELSE COALESCE(calls.cost_reported, 0) END,
      COALESCE(calls.cache_read * (p.input - p.cache_read) / 1e6, 0)
    FROM (SELECT 1) LEFT JOIN prices p ON p.model = calls.model
     AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                         WHERE model = calls.model AND valid_from <= calls.ts))
  WHERE model IN (OLD.model, NEW.model);
END;

-- Filas ya guardadas (bases en archivo de versiones anteriores).
UPDATE calls SET (has_price, cost_usd, cache_savings_usd) = (
  SELECT p.model IS NOT NULL OR calls.cost_reported IS NOT NULL,
    CASE WHEN p.model IS NOT NULL THEN (
      calls.input_tokens * p.input +
      calls.output_tokens * p.output +
      calls.cache_read * p.cache_read +
      (calls.cache_write - calls.cache_write_1h) * p.cache_write +
      calls.cache_write_1h * p.cache_write_1h
    ) / 1e6 ELSE COALESCE(calls.cost_reported, 0) END,
    COALESCE(calls.cache_read * (p.input - p.cache_read) / 1e6, 0)
  FROM (SELECT 1) LEFT JOIN prices p ON p.model = calls.model
   AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                       WHERE model = calls.model AND valid_from <= calls.ts));

DROP VIEW call_costs;
CREATE VIEW call_costs AS
SELECT message_id, session_id, ts, model, input_tokens, output_tokens, cache_read, cache_write,
  cache_write_1h, reasoning_tokens, activity, turn_id, is_sidechain, agent_id, cost_reported,
  has_price, cost_usd, cache_savings_usd
FROM calls;
