//! Contrato para soportar un agente: dónde están sus logs y cómo se convierte cada línea en filas.

pub mod claude_code;
pub mod codex;
pub mod copilot;
pub mod cursor;
pub mod gemini;
pub mod opencode;

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Cómo guarda sus datos el agente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Archivos JSONL que se leen línea a línea desde un offset.
    Jsonl,
    /// Una base SQLite que se consulta desde un cursor de tiempo.
    Sqlite,
}

pub trait Provider: Send + Sync {
    /// Identificador estable que se guarda en la base (`claude-code`).
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn source(&self) -> Source {
        Source::Jsonl
    }
    /// ¿Está el agente en este ordenador? Por defecto, si existe alguna carpeta de logs.
    /// Un agente instalado aparece en la lista aunque aún no tenga sesiones.
    fn installed(&self) -> bool {
        self.log_roots().iter().any(|r| r.is_dir())
    }
    /// El archivo se va a leer desde el principio: descarta el estado que se guardara de él.
    fn reset(&self, _path: &Path) {}
    /// Solo para `Source::Sqlite`: registros modificados después de `since` y el nuevo cursor.
    fn read_db(&self, _path: &Path, since: i64) -> Result<(Vec<Record>, i64)> {
        Ok((vec![], since))
    }
    /// Carpetas a vigilar en este sistema (ya resueltas con `dirs`).
    fn log_roots(&self) -> Vec<PathBuf>;
    /// ¿Este archivo es una sesión de este agente?
    fn matches(&self, path: &Path) -> bool;
    /// Convierte una línea en filas para la base.
    fn parse_line(&self, path: &Path, line: &str) -> Result<Vec<Record>>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Session(SessionRec),
    Call(CallRec),
    ToolUse(ToolUseRec),
    ToolResult { call_id: String, ts: i64, is_error: bool, agent_id: Option<String> },
    Event(EventRec),
    Turn(TurnRec),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnRec {
    pub id: String,
    pub session_id: String,
    pub ts: i64,
    pub intent: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRec {
    pub id: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub ts: i64,
    pub is_subagent: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CallRec {
    pub message_id: String,
    pub session_id: String,
    pub ts: i64,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub cache_write_1h: i64,
    pub reasoning_tokens: i64,
    pub activity: Option<String>,
    pub is_sidechain: bool,
    pub agent_id: Option<String>,
    /// Coste calculado por el propio agente, si lo trae (OpenCode).
    pub cost_reported: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseRec {
    pub call_id: String,
    pub message_id: String,
    pub session_id: String,
    pub ts: i64,
    pub tool: String,
    pub target: Option<String>,
    /// Skill invocada o tipo de subagente lanzado.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventRec {
    pub session_id: String,
    pub ts: i64,
    pub kind: String,
    pub payload_json: Option<String>,
}

/// Todos los agentes soportados.
pub fn all() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(claude_code::ClaudeCode::default()),
        Box::new(codex::Codex::default()),
        Box::new(copilot::Copilot::default()),
        Box::new(cursor::Cursor::default()),
        Box::new(gemini::Gemini::default()),
        Box::new(opencode::OpenCode::default()),
    ]
}

/// ¿Hay un ejecutable con ese nombre en el PATH?
pub fn on_path(bin: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else { return false };
    std::env::split_paths(&path).any(|dir| {
        let p = dir.join(bin);
        p.is_file() || (cfg!(windows) && (dir.join(format!("{bin}.exe")).is_file() || dir.join(format!("{bin}.cmd")).is_file()))
    })
}

/// RFC 3339 → epoch ms UTC.
pub fn parse_ts(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp_millis())
}

/// Actividad aproximada según la herramienta usada.
pub fn classify_tool(tool: &str, target: Option<&str>) -> &'static str {
    let t = tool.to_ascii_lowercase();
    let target = target.unwrap_or("").to_ascii_lowercase();
    let is_test = ["test", "pytest", "jest", "vitest", "cargo t", "go t", "spec"]
        .iter()
        .any(|k| target.contains(k));
    match t.as_str() {
        "edit" | "write" | "multiedit" | "notebookedit" | "apply_patch" | "replace" | "write_file" => "coding",
        "bash" | "shell" | "exec_command" | "run_shell_command" if is_test => "testing",
        "bash" | "shell" | "exec_command" | "run_shell_command" => "shell",
        "read" | "grep" | "glob" | "ls" | "read_file" | "search_file_content" | "list_directory" => "exploration",
        "webfetch" | "websearch" | "web_fetch" | "google_web_search" => "research",
        "task" | "agent" => "delegation",
        _ => "other",
    }
}

/// Intención de un prompt por palabras clave (español e inglés). Solo se guarda la etiqueta.
pub fn prompt_intent(text: &str) -> Option<&'static str> {
    let t = text.to_lowercase();
    let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let has_word = |list: &[&str]| words.iter().any(|w| list.contains(w));
    let has_prefix = |list: &[&str]| words.iter().any(|w| list.iter().any(|p| w.starts_with(p)));
    let has_phrase = |list: &[&str]| list.iter().any(|p| t.contains(p));

    if has_word(&["fix", "bug", "bugs", "error", "errors", "crash", "broken", "debug", "fails", "failing", "roto", "rota", "bug"])
        || has_prefix(&["arregl", "falla", "fallo", "depur", "corrig", "exception", "excepci"])
        || has_phrase(&["no funciona", "doesn't work", "not working", "why does", "por qué falla"])
    {
        return Some("debug");
    }
    if has_word(&["add", "implement", "create", "build", "haz", "hazlo", "nueva", "nuevo", "feature", "support"])
        || has_prefix(&["añad", "agreg", "implement", "crea", "constru", "desarroll"])
        || has_phrase(&["quiero que", "i want", "new feature", "make it"])
    {
        return Some("feature");
    }
    if has_word(&["idea", "ideas", "brainstorm", "opinas", "opinion", "alternatives", "pros", "compare", "deberíamos", "should"])
        || has_prefix(&["alternativ", "propon", "propuest", "compar", "pienso", "piensa"])
        || has_phrase(&["what if", "qué te parece", "que te parece", "what do you think"])
    {
        return Some("brainstorm");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intencion_del_prompt() {
        assert_eq!(prompt_intent("Fix the failing test"), Some("debug"));
        assert_eq!(prompt_intent("Arregla el error de la ingesta"), Some("debug"));
        assert_eq!(prompt_intent("Añade un filtro por proyecto"), Some("feature"));
        assert_eq!(prompt_intent("¿Qué te parece usar Svelte?"), Some("brainstorm"));
        assert_eq!(prompt_intent("Please address the review"), None, "address no es add");
        assert_eq!(prompt_intent("gracias"), None);
    }

    #[test]
    fn fecha_utc_a_epoch() {
        assert_eq!(parse_ts("2026-09-25T09:57:51.467Z"), Some(1_790_330_271_467));
        assert_eq!(parse_ts("no es fecha"), None);
    }

    #[test]
    fn clasifica_herramientas() {
        assert_eq!(classify_tool("Edit", None), "coding");
        assert_eq!(classify_tool("Bash", Some("cargo test")), "testing");
        assert_eq!(classify_tool("Bash", Some("ls -la")), "shell");
        assert_eq!(classify_tool("Grep", None), "exploration");
    }
}
