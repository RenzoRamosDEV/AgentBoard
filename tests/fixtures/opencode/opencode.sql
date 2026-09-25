-- Subconjunto anonimizado del esquema de OpenCode (~/.local/share/opencode/opencode.db).
CREATE TABLE project (id TEXT PRIMARY KEY, worktree TEXT NOT NULL, name TEXT, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
CREATE TABLE session (
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, parent_id TEXT, slug TEXT NOT NULL, directory TEXT NOT NULL,
  title TEXT NOT NULL, version TEXT NOT NULL, cost REAL DEFAULT 0 NOT NULL,
  tokens_input INTEGER DEFAULT 0 NOT NULL, tokens_output INTEGER DEFAULT 0 NOT NULL, tokens_reasoning INTEGER DEFAULT 0 NOT NULL,
  tokens_cache_read INTEGER DEFAULT 0 NOT NULL, tokens_cache_write INTEGER DEFAULT 0 NOT NULL,
  agent TEXT, model TEXT, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);

INSERT INTO project VALUES ('p1', '/home/dev/Proyectos/demo', NULL, 1790331245080, 1790331245080);
INSERT INTO session (id, project_id, parent_id, slug, directory, title, version, agent, model, time_created, time_updated) VALUES
  ('ses_main', 'p1', NULL, 'main', '/home/dev/Proyectos/demo', 'Arreglar tests', '1.0', 'build', '{"id":"big-pickle","providerID":"opencode"}', 1790331245080, 1790331400000),
  ('ses_child', 'p1', 'ses_main', 'child', '/home/dev/Proyectos/demo', 'Buscar', '1.0', 'explore', '{"id":"big-pickle","providerID":"opencode"}', 1790331300000, 1790331350000);

-- Turno 1: prompt de depuración, bash con error y edición.
INSERT INTO message VALUES ('msg_u1', 'ses_main', 1790331245100, 1790331245100, '{"role":"user","time":{"created":1790331245100},"agent":"build","model":{"providerID":"opencode","modelID":"big-pickle"}}');
INSERT INTO part VALUES ('prt_t1', 'msg_u1', 'ses_main', 1790331245110, 1790331245110, '{"type":"text","text":"Arregla el test que falla"}');
INSERT INTO message VALUES ('msg_a1', 'ses_main', 1790331246000, 1790331250000, '{"parentID":"msg_u1","role":"assistant","agent":"build","path":{"cwd":"/home/dev/Proyectos/demo","root":"/home/dev/Proyectos/demo"},"cost":0.0123,"tokens":{"total":8457,"input":6497,"output":168,"reasoning":10,"cache":{"write":0,"read":1792}},"modelID":"big-pickle","providerID":"opencode","time":{"created":1790331246000,"completed":1790331250000},"finish":"tool-calls"}');
INSERT INTO part VALUES ('prt_b1', 'msg_a1', 'ses_main', 1790331247000, 1790331248000, '{"type":"tool","tool":"bash","callID":"call_1","state":{"status":"error","input":{"command":"cargo test 2>&1 | tail -5"},"output":"test failed","time":{"start":1790331247000,"end":1790331248000}}}');
INSERT INTO message VALUES ('msg_a2', 'ses_main', 1790331251000, 1790331255000, '{"parentID":"msg_u1","role":"assistant","agent":"build","cost":0.02,"tokens":{"total":9000,"input":200,"output":300,"reasoning":0,"cache":{"write":100,"read":8400}},"modelID":"claude-sonnet-4-5","providerID":"anthropic","time":{"created":1790331251000,"completed":1790331255000},"finish":"stop"}');
INSERT INTO part VALUES ('prt_e1', 'msg_a2', 'ses_main', 1790331252000, 1790331252500, '{"type":"tool","tool":"edit","callID":"call_2","state":{"status":"completed","input":{"filePath":"/home/dev/Proyectos/demo/src/lib.rs","oldString":"a","newString":"b"},"output":"Edit applied successfully.","time":{"start":1790331252000,"end":1790331252500}}}');
INSERT INTO part VALUES ('prt_k1', 'msg_a2', 'ses_main', 1790331253000, 1790331253500, '{"type":"tool","tool":"task","callID":"call_3","state":{"status":"completed","input":{"description":"Buscar coste","subagent_type":"explore","prompt":"..."},"output":"ok","metadata":{"sessionId":"ses_child"},"time":{"start":1790331253000,"end":1790331253500}}}');

-- Sesión hija (subagente).
INSERT INTO message VALUES ('msg_c1', 'ses_child', 1790331300100, 1790331300100, '{"role":"user","time":{"created":1790331300100},"agent":"explore"}');
INSERT INTO message VALUES ('msg_c2', 'ses_child', 1790331301000, 1790331305000, '{"parentID":"msg_c1","role":"assistant","agent":"explore","cost":0.001,"tokens":{"total":500,"input":400,"output":100,"reasoning":0,"cache":{"write":0,"read":0}},"modelID":"big-pickle","providerID":"opencode","time":{"created":1790331301000,"completed":1790331305000},"finish":"stop"}');
INSERT INTO part VALUES ('prt_g1', 'msg_c2', 'ses_child', 1790331302000, 1790331302200, '{"type":"tool","tool":"grep","callID":"call_4","state":{"status":"completed","input":{"pattern":"cost"},"output":"pricing.rs","time":{"start":1790331302000,"end":1790331302200}}}');
