//! Contrato para soportar un agente: dónde están sus logs y cómo se convierte cada línea en filas.

pub mod claude_code;

use anyhow::Result;
use std::path::{Path, PathBuf};

pub trait Provider: Send + Sync {
    /// Identificador estable que se guarda en la base (`claude-code`).
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
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
    ToolResult { call_id: String, ts: i64, is_error: bool },
    Event(EventRec),
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolUseRec {
    pub call_id: String,
    pub message_id: String,
    pub session_id: String,
    pub ts: i64,
    pub tool: String,
    pub target: Option<String>,
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
    vec![Box::new(claude_code::ClaudeCode::default())]
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

#[cfg(test)]
mod tests {
    use super::*;

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
