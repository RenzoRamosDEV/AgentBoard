-- Índices revisados con `EXPLAIN QUERY PLAN` (test `consultas_usan_indices`): ninguna consulta
-- busca llamadas por `agent_id` (el desglose por tipo de subagente filtra `calls` por `ts` y une
-- el resto con un índice automático), así que solo encarecía cada inserción.

DROP INDEX calls_agent;
