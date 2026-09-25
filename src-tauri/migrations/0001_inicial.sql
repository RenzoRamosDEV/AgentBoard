-- Esquema inicial de AgentBoard. Fechas en epoch ms UTC.

CREATE TABLE agents (
  id         TEXT PRIMARY KEY,           -- claude-code, codex, gemini
  name       TEXT NOT NULL,
  log_root   TEXT NOT NULL,
  first_seen INTEGER NOT NULL
);

CREATE TABLE projects (
  id        INTEGER PRIMARY KEY,
  name      TEXT NOT NULL,
  cwd       TEXT NOT NULL UNIQUE,
  repo_root TEXT NOT NULL
);

CREATE TABLE sessions (
  id          TEXT PRIMARY KEY,
  agent_id    TEXT NOT NULL REFERENCES agents(id),
  project_id  INTEGER REFERENCES projects(id),
  git_branch  TEXT,
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER NOT NULL,
  model       TEXT,
  is_subagent INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX sessions_project ON sessions(project_id);
CREATE INDEX sessions_started ON sessions(started_at);

CREATE TABLE calls (
  message_id       TEXT PRIMARY KEY,
  session_id       TEXT NOT NULL REFERENCES sessions(id),
  ts               INTEGER NOT NULL,
  model            TEXT NOT NULL,
  input_tokens     INTEGER NOT NULL DEFAULT 0,
  output_tokens    INTEGER NOT NULL DEFAULT 0,
  cache_read       INTEGER NOT NULL DEFAULT 0,
  cache_write      INTEGER NOT NULL DEFAULT 0,  -- total escrito en caché
  cache_write_1h   INTEGER NOT NULL DEFAULT 0,  -- parte de cache_write con TTL de 1 h
  reasoning_tokens INTEGER NOT NULL DEFAULT 0,  -- ya incluidos en output_tokens
  activity         TEXT                         -- coding, testing, exploration…
);
CREATE INDEX calls_ts ON calls(ts);
CREATE INDEX calls_session ON calls(session_id);

CREATE TABLE tool_calls (
  id          INTEGER PRIMARY KEY,
  call_id     TEXT NOT NULL UNIQUE,
  message_id  TEXT,
  session_id  TEXT NOT NULL REFERENCES sessions(id),
  ts          INTEGER NOT NULL,
  tool        TEXT NOT NULL,
  target      TEXT,
  is_error    INTEGER NOT NULL DEFAULT 0,
  duration_ms INTEGER
);
CREATE INDEX tool_calls_ts ON tool_calls(ts);
CREATE INDEX tool_calls_session ON tool_calls(session_id);

CREATE TABLE events (
  id           INTEGER PRIMARY KEY,
  session_id   TEXT NOT NULL REFERENCES sessions(id),
  ts           INTEGER NOT NULL,
  kind         TEXT NOT NULL,             -- compaction, interruption, rate_limit…
  payload_json TEXT,
  UNIQUE (session_id, ts, kind)
);

CREATE TABLE prices (
  model          TEXT NOT NULL,
  valid_from     INTEGER NOT NULL,
  input          REAL NOT NULL,           -- USD por millón de tokens
  output         REAL NOT NULL,
  cache_read     REAL NOT NULL DEFAULT 0,
  cache_write    REAL NOT NULL DEFAULT 0, -- escritura con TTL de 5 min
  cache_write_1h REAL NOT NULL DEFAULT 0,
  PRIMARY KEY (model, valid_from)
);

CREATE TABLE file_state (
  path      TEXT PRIMARY KEY,
  agent_id  TEXT NOT NULL,
  file_id   TEXT NOT NULL,
  size      INTEGER NOT NULL,
  offset    INTEGER NOT NULL,
  mtime     INTEGER NOT NULL,
  last_scan INTEGER NOT NULL
);

-- Coste de cada llamada con el precio vigente en su fecha; sin precio = 0.
CREATE VIEW call_costs AS
SELECT c.*,
  (p.model IS NOT NULL) AS has_price,
  COALESCE((
    c.input_tokens * p.input +
    c.output_tokens * p.output +
    c.cache_read * p.cache_read +
    (c.cache_write - c.cache_write_1h) * p.cache_write +
    c.cache_write_1h * p.cache_write_1h
  ) / 1e6, 0) AS cost_usd
FROM calls c
LEFT JOIN prices p ON p.model = c.model
 AND p.valid_from = (SELECT MAX(valid_from) FROM prices
                     WHERE model = c.model AND valid_from <= c.ts);
