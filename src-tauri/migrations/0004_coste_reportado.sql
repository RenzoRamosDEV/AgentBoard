-- Coste calculado por el propio agente (OpenCode lo trae por mensaje): se usa si el modelo
-- no tiene precio en la tabla.

ALTER TABLE calls ADD COLUMN cost_reported REAL;

DROP VIEW call_costs;
CREATE VIEW call_costs AS
SELECT c.*,
  (p.model IS NOT NULL OR c.cost_reported IS NOT NULL) AS has_price,
  CASE WHEN p.model IS NOT NULL THEN (
    c.input_tokens * p.input +
    c.output_tokens * p.output +
    c.cache_read * p.cache_read +
    (c.cache_write - c.cache_write_1h) * p.cache_write +
    c.cache_write_1h * p.cache_write_1h
  ) / 1e6 ELSE COALESCE(c.cost_reported, 0) END AS cost_usd,
  COALESCE(c.cache_read * (p.input - p.cache_read) / 1e6, 0) AS cache_savings_usd
FROM calls c
LEFT JOIN prices p ON p.model = c.model
 AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                     WHERE model = c.model AND valid_from <= c.ts);
