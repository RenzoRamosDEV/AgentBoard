-- Turnos (un prompt del usuario y lo que sigue), detalle de herramientas y subagentes.

CREATE TABLE turns (
  id         TEXT PRIMARY KEY,           -- promptId de Claude Code
  session_id TEXT NOT NULL REFERENCES sessions(id),
  ts         INTEGER NOT NULL,
  intent     TEXT                        -- debug, feature, brainstorm (del texto, que no se guarda)
);
CREATE INDEX turns_session_ts ON turns(session_id, ts);

ALTER TABLE calls ADD COLUMN turn_id TEXT;
ALTER TABLE calls ADD COLUMN is_sidechain INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calls ADD COLUMN agent_id TEXT;
CREATE INDEX calls_turn ON calls(turn_id);
CREATE INDEX calls_agent ON calls(agent_id);

ALTER TABLE tool_calls ADD COLUMN turn_id TEXT;
ALTER TABLE tool_calls ADD COLUMN detail TEXT;     -- skill o tipo de subagente
ALTER TABLE tool_calls ADD COLUMN agent_id TEXT;   -- agentId del subagente lanzado
CREATE INDEX tool_calls_turn ON tool_calls(turn_id);
CREATE INDEX tool_calls_agent ON tool_calls(agent_id);

-- Fuerza una relectura completa para rellenar las columnas nuevas (los upserts no duplican).
DELETE FROM file_state;
