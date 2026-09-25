-- Preferencias del usuario (valor en JSON) y ahorro por caché en la vista de costes.

CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

DROP VIEW call_costs;
CREATE VIEW call_costs AS
SELECT c.*,
  (p.model IS NOT NULL) AS has_price,
  COALESCE((
    c.input_tokens * p.input +
    c.output_tokens * p.output +
    c.cache_read * p.cache_read +
    (c.cache_write - c.cache_write_1h) * p.cache_write +
    c.cache_write_1h * p.cache_write_1h
  ) / 1e6, 0) AS cost_usd,
  -- Lo que habría costado leer esos tokens sin caché, menos lo que costó leerlos de ella.
  COALESCE(c.cache_read * (p.input - p.cache_read) / 1e6, 0) AS cache_savings_usd
FROM calls c
LEFT JOIN prices p ON p.model = c.model
 AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                     WHERE model = c.model AND valid_from <= c.ts);
