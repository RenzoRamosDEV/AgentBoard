-- Caché fiel a los logs presentes: cada archivo leído tiene un id estable (el `rowid` implícito de
-- antes cambia con `VACUUM`) y `session_files` dice a qué sesiones aportó, para retirar sus datos
-- y rehacer esas sesiones con sus otros archivos cuando desaparece, se sustituye o encoge.

CREATE TABLE file_state_new (
  id        INTEGER PRIMARY KEY,
  path      TEXT NOT NULL UNIQUE,
  agent_id  TEXT NOT NULL,
  file_id   TEXT NOT NULL,
  size      INTEGER NOT NULL,
  offset    INTEGER NOT NULL,
  mtime     INTEGER NOT NULL,
  last_scan INTEGER NOT NULL
);
INSERT INTO file_state_new (path, agent_id, file_id, size, offset, mtime, last_scan)
  SELECT path, agent_id, file_id, size, offset, mtime, last_scan FROM file_state;
DROP TABLE file_state;
ALTER TABLE file_state_new RENAME TO file_state;

CREATE TABLE session_files (
  session_id TEXT NOT NULL,
  file       INTEGER NOT NULL,
  PRIMARY KEY (session_id, file)
) WITHOUT ROWID;
CREATE INDEX session_files_file ON session_files(file);
