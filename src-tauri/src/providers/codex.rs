//! Codex CLI: `~/.codex/sessions/YYYY/MM/DD/rollout-<fecha>-<thread>.jsonl` (y `archived_sessions/`).
//! Cada línea es `{"timestamp","type","payload"}`; el tipo va en `type`: `session_meta`,
//! `turn_context`, `token_usage_record`, `response_item`, `event_msg`, `compacted`…
//!
//! A diferencia de Claude Code, el modelo y la carpeta no van en cada línea: se guardan por
//! archivo en `state` mientras se lee.

use super::json::{Bool, Int, List, NotNull, Obj, OptInt, Seen, Str};
use super::{
    parse_ts, prompt_intent, CallRec, EventRec, Provider, Record, SessionRec, ToolUseRec, TurnRec,
};
use anyhow::Result;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Codex {
    roots: Option<Vec<PathBuf>>,
    state: Mutex<HashMap<PathBuf, FileState>>,
}

/// Lo que se sabe de una sesión tras leer sus primeras líneas.
#[derive(Default, Clone)]
struct FileState {
    thread_id: String,
    cwd: Option<String>,
    branch: Option<String>,
    model: String,
    is_subagent: bool,
    turn_id: Option<String>,
    /// Intención del último prompt humano, a la espera de su `turn_context` (el texto no se
    /// guarda).
    pending_intent: Option<&'static str>,
    /// Con `token_usage_record` presentes se ignoran los `token_count` (formato antiguo).
    has_usage_records: bool,
    /// Con eventos `item_completed` (formato nuevo) las herramientas salen de ellos y se ignoran
    /// los `function_call` de `response_item`, para no contarlas dos veces.
    has_items: bool,
}

impl Codex {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots: Some(roots),
            state: Mutex::default(),
        }
    }

    fn home() -> Option<PathBuf> {
        match std::env::var_os("CODEX_HOME") {
            Some(h) if !h.is_empty() => Some(PathBuf::from(h)),
            _ => dirs::home_dir().map(|h| h.join(".codex")),
        }
    }
}

/// Nombres de herramienta de Codex → los de Claude Code (misma clasificación y 1-shot).
fn canonical_tool(name: &str) -> &str {
    match name {
        "shell" | "shell_command" | "local_shell" | "exec_command" | "container.exec" => "Bash",
        "apply_patch" => "Edit",
        "write_file" | "create_file" => "Write",
        "read_file" | "view_image" => "Read",
        "grep_files" | "search" => "Grep",
        "list_dir" => "LS",
        "web_search" => "WebSearch",
        "fetch_url" | "web_fetch" => "WebFetch",
        "update_plan" => "TodoWrite",
        "spawn_agent" | "delegate" => "Agent",
        "request_user_input" => "AskUserQuestion",
        other => other,
    }
}

/// Argumentos de una herramienta: los campos que dan su objetivo y su rol de subagente.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Args<'a> {
    #[serde(borrow)]
    command: Command<'a>,
    #[serde(borrow)]
    cmd: Str<'a>,
    #[serde(borrow)]
    input: Str<'a>,
    #[serde(borrow)]
    patch: Str<'a>,
    #[serde(borrow)]
    path: Str<'a>,
    #[serde(borrow)]
    file_path: Str<'a>,
    #[serde(borrow)]
    pattern: Str<'a>,
    #[serde(borrow)]
    query: Str<'a>,
    #[serde(borrow)]
    url: Str<'a>,
    #[serde(borrow)]
    description: Str<'a>,
    #[serde(borrow)]
    agent_type: Str<'a>,
    #[serde(borrow)]
    agent_role: Str<'a>,
    #[serde(borrow)]
    role: Str<'a>,
}

/// `command`: lista de argumentos, cadena u otra cosa.
#[derive(Debug, Default)]
enum Command<'a> {
    Parts(Vec<Str<'a>>),
    Text(Cow<'a, str>),
    #[default]
    Other,
}

impl<'de: 'a, 'a> Deserialize<'de> for Command<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        command_from(d)
    }
}

fn command_from<'de: 'a, 'a, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Command<'a>, D::Error> {
    use serde::de::{self, value::SeqAccessDeserializer, IgnoredAny, Visitor};
    struct V<'a>(std::marker::PhantomData<&'a ()>);
    impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
        type Value = Command<'a>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("cualquier valor JSON")
        }
        fn visit_borrowed_str<E: de::Error>(
            self,
            v: &'de str,
        ) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Text(Cow::Borrowed(v)))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Text(Cow::Owned(v.to_owned())))
        }
        fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Text(Cow::Owned(v)))
        }
        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            seq: A,
        ) -> std::result::Result<Command<'a>, A::Error> {
            let List(parts) = List::deserialize(SeqAccessDeserializer::new(seq))?;
            Ok(Command::Parts(parts))
        }
        fn visit_map<A: de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Command<'a>, A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(Command::Other)
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Other)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Other)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Other)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Other)
        }
        fn visit_unit<E: de::Error>(self) -> std::result::Result<Command<'a>, E> {
            Ok(Command::Other)
        }
    }
    d.deserialize_any(V(std::marker::PhantomData))
}

/// Comando de shell legible: `bash -lc "<cmd>"` da el último argumento.
fn shell_target(command: &Command, cmd: Option<&str>) -> String {
    match command {
        Command::Parts(parts) => {
            let parts: Vec<&str> = parts.iter().filter_map(|p| p.as_deref()).collect();
            if parts.len() >= 3 && parts[1].starts_with('-') && parts[1].contains('c') {
                parts[2].to_string()
            } else {
                parts.join(" ")
            }
        }
        Command::Text(s) => s.to_string(),
        Command::Other => cmd.unwrap_or("").to_string(),
    }
}

/// Objetivo legible de una llamada: comando de shell o archivo del parche.
fn tool_target(name: &str, args: &Args) -> Option<String> {
    let s = match name {
        "Bash" => shell_target(&args.command, args.cmd.as_deref()),
        "Edit" => {
            let patch = args
                .input
                .as_deref()
                .or(args.patch.as_deref())
                .unwrap_or("");
            patch
                .lines()
                .find_map(|l| {
                    l.strip_prefix("*** Update File: ")
                        .or(l.strip_prefix("*** Add File: "))
                        .or(l.strip_prefix("*** Delete File: "))
                })
                .unwrap_or("")
                .to_string()
        }
        _ => [
            &args.path,
            &args.file_path,
            &args.pattern,
            &args.query,
            &args.url,
            &args.description,
        ]
        .iter()
        .find_map(|k| k.as_deref())
        .unwrap_or("")
        .to_string(),
    };
    (!s.is_empty()).then(|| s.chars().take(500).collect())
}

/// Línea útil de Codex: solo su `payload` (la fecha y el tipo vienen de la sonda).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Line<'a> {
    #[serde(borrow)]
    payload: Obj<Payload<'a>>,
}

/// Campos usados de todos los `payload` útiles; los demás (instrucciones, textos del modelo…)
/// se saltan sin copiarlos.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Payload<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    id: Str<'a>,
    #[serde(borrow)]
    session_id: Str<'a>,
    #[serde(borrow)]
    cwd: Str<'a>,
    #[serde(borrow)]
    git: Obj<Git<'a>>,
    #[serde(borrow)]
    parent_thread_id: Str<'a>,
    #[serde(borrow)]
    thread_source: Str<'a>,
    source: Obj<Source>,
    #[serde(borrow)]
    model: Str<'a>,
    #[serde(borrow)]
    turn_id: Str<'a>,
    #[serde(borrow)]
    response_id: Str<'a>,
    usage: Obj<TokenUsage>,
    #[serde(borrow)]
    message: Str<'a>,
    #[serde(borrow)]
    item: Obj<Item<'a>>,
    info: Obj<Info>,
    #[serde(borrow)]
    name: Str<'a>,
    #[serde(borrow)]
    namespace: Str<'a>,
    #[serde(borrow)]
    action: Obj<Args<'a>>,
    #[serde(borrow)]
    input: Str<'a>,
    #[serde(borrow)]
    arguments: Str<'a>,
    #[serde(borrow)]
    call_id: Str<'a>,
    #[serde(borrow)]
    output: Output<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Git<'a> {
    #[serde(borrow)]
    branch: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Source {
    sub_agent: Seen,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Info {
    last_token_usage: Obj<TokenUsage>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TokenUsage {
    input_tokens: Int,
    cached_input_tokens: Int,
    output_tokens: Int,
    cache_write_input_tokens: Int,
    reasoning_output_tokens: Int,
}

/// Un item de `item_completed` (formato nuevo de Codex).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Item<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    id: Str<'a>,
    #[serde(borrow)]
    status: Str<'a>,
    duration: Obj<Duration>,
    #[serde(borrow)]
    command: Command<'a>,
    exit_code: OptInt,
    #[serde(borrow)]
    changes: Obj<BTreeMap<Cow<'a, str>, Obj<Change<'a>>>>,
    #[serde(borrow)]
    server: Str<'a>,
    #[serde(borrow)]
    tool: Str<'a>,
    error: NotNull,
    #[serde(borrow)]
    query: Str<'a>,
    #[serde(rename = "kind", borrow)]
    sub_kind: Str<'a>,
    #[serde(borrow)]
    action: Obj<ItemAction<'a>>,
    #[serde(borrow)]
    receiver_agents: List<Obj<Receiver<'a>>>,
    #[serde(borrow)]
    model: Str<'a>,
    #[serde(borrow)]
    prompt: Str<'a>,
    #[serde(borrow)]
    agent_thread_id: Str<'a>,
    #[serde(borrow)]
    content: List<Obj<TextPart<'a>>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Duration {
    secs: OptInt,
    nanos: Int,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Change<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ItemAction<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    url: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Receiver<'a> {
    #[serde(borrow)]
    role: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TextPart<'a> {
    #[serde(borrow)]
    text: Str<'a>,
}

/// Duración `{"secs", "nanos"}` de un item, en ms.
fn duration_ms(d: &Obj<Duration>) -> Option<i64> {
    let d = d.0.as_ref()?;
    Some(d.secs.0? * 1000 + d.nanos.0 / 1_000_000)
}

/// Una herramienta sacada de un `item_completed` (formato nuevo de Codex).
struct ItemTool {
    call_id: String,
    tool: String,
    target: Option<String>,
    detail: Option<String>,
    is_error: bool,
    duration_ms: Option<i64>,
}

/// Herramientas de un `item_completed`. Un `FileChange` da una por archivo.
fn item_tools(it: &Item) -> Vec<ItemTool> {
    let t = |call_id: String,
             tool: &str,
             target: Option<String>,
             detail: Option<String>,
             is_error: bool,
             duration_ms: Option<i64>| ItemTool {
        call_id,
        tool: tool.to_string(),
        target,
        detail,
        is_error,
        duration_ms,
    };
    let id = it.id.as_deref().unwrap_or("").to_string();
    let failed = matches!(it.status.as_deref(), Some("failed" | "declined"));
    let dur = duration_ms(&it.duration);
    match it.kind.as_deref() {
        // `source` es `unified_exec_startup` también en los comandos del modelo: cuentan todos.
        Some("CommandExecution") => {
            let target = shell_target(&it.command, None);
            let target = (!target.is_empty()).then(|| target.chars().take(500).collect());
            let err = failed || it.exit_code.0.is_some_and(|c| c != 0);
            vec![t(id, "Bash", target, None, err, dur)]
        }
        Some("FileChange") => it
            .changes
            .0
            .iter()
            .flatten()
            .enumerate()
            .map(|(i, (path, change))| {
                let tool = match change.0.as_ref().and_then(|c| c.kind.as_deref()) {
                    Some("add") => "Write",
                    Some("delete") => "Delete",
                    _ => "Edit",
                };
                t(
                    format!("{id}:{i}"),
                    tool,
                    Some(path.chars().take(500).collect()),
                    None,
                    failed,
                    None,
                )
            })
            .collect(),
        Some("McpToolCall") => {
            let (server, tool) = (
                it.server.as_deref().unwrap_or("mcp"),
                it.tool.as_deref().unwrap_or("tool"),
            );
            let err = failed || it.error.0;
            vec![t(
                id,
                &format!("mcp__{server}__{tool}"),
                None,
                None,
                err,
                dur,
            )]
        }
        Some("WebSearch") => vec![t(
            id,
            "WebSearch",
            it.query.as_deref().map(str::to_string),
            None,
            false,
            None,
        )],
        Some("Extension") if it.sub_kind.as_deref() == Some("web.search") => {
            let action = it.action.0.as_ref();
            let open = action.and_then(|a| a.kind.as_deref()) == Some("openPage");
            let (tool, target) = if open {
                (
                    "WebFetch",
                    action
                        .and_then(|a| a.url.as_deref())
                        .or(it.query.as_deref()),
                )
            } else {
                ("WebSearch", it.query.as_deref())
            };
            vec![t(id, tool, target.map(str::to_string), None, false, None)]
        }
        Some("CollabAgentToolCall") if it.tool.as_deref().is_some_and(|t| t.contains("spawn")) => {
            let detail = it
                .receiver_agents
                .0
                .first()
                .and_then(|r| r.0.as_ref())
                .and_then(|r| r.role.as_deref())
                .or(it.model.as_deref())
                .unwrap_or("general-purpose");
            vec![t(
                id,
                "Agent",
                it.prompt.as_deref().map(|p| p.chars().take(200).collect()),
                Some(detail.to_string()),
                failed,
                None,
            )]
        }
        _ => vec![],
    }
}

/// Texto de un `UserMessage` del formato nuevo.
fn item_text(it: &Item) -> String {
    it.content
        .0
        .iter()
        .filter_map(|c| c.0.as_ref().and_then(|c| c.text.as_deref()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `function_call_output.output`: texto u objeto `{"content","success"}`.
#[derive(Debug, Default)]
enum Output<'a> {
    Text(Cow<'a, str>),
    Object(Option<bool>),
    #[default]
    Other,
}

impl<'de: 'a, 'a> Deserialize<'de> for Output<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        use serde::de::{self, value::MapAccessDeserializer, IgnoredAny, Visitor};
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Fields {
            success: Bool,
        }
        struct V<'a>(std::marker::PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for V<'a> {
            type Value = Output<'a>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("cualquier valor JSON")
            }
            fn visit_borrowed_str<E: de::Error>(
                self,
                v: &'de str,
            ) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Text(Cow::Borrowed(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Text(Cow::Owned(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Text(Cow::Owned(v)))
            }
            fn visit_map<A: de::MapAccess<'de>>(
                self,
                map: A,
            ) -> std::result::Result<Output<'a>, A::Error> {
                let f = Fields::deserialize(MapAccessDeserializer::new(map))?;
                Ok(Output::Object(f.success.0))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Output<'a>, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Output::Other)
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Other)
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Other)
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Other)
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Other)
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Output<'a>, E> {
                Ok(Output::Other)
            }
        }
        d.deserialize_any(V(std::marker::PhantomData))
    }
}

/// Metadatos de la salida antigua de shell (un JSON dentro del texto).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ShellOutput {
    metadata: Obj<ShellMetadata>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ShellMetadata {
    exit_code: OptInt,
}

/// ¿La salida de una herramienta indica error? Objeto con `success`, JSON con
/// `metadata.exit_code` o texto «Process exited with code N».
fn output_is_error(output: &Output) -> bool {
    match output {
        Output::Object(Some(ok)) => !ok,
        Output::Text(text) => {
            if let Ok(Obj(Some(v))) = serde_json::from_str::<Obj<ShellOutput>>(text) {
                if let Some(code) = v.metadata.0.and_then(|m| m.exit_code.0) {
                    return code != 0;
                }
            }
            // Salida antigua en texto plano: «Process exited with code N\nFinal output:…».
            if let Some(rest) = text.strip_prefix("Process exited with code ") {
                let code: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '-')
                    .collect();
                if let Ok(n) = code.parse::<i64>() {
                    return n != 0;
                }
            }
            false
        }
        _ => false,
    }
}

/// Envoltura de una línea: fecha, tipo y tipo del `payload`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Probe<'a> {
    #[serde(borrow)]
    timestamp: Str<'a>,
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    payload: Obj<PayloadProbe<'a>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PayloadProbe<'a> {
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
}

impl Probe<'_> {
    /// ¿Puede la línea producir filas o cambiar el estado del archivo? (Derivado de `parse_line`.)
    fn is_useful(&self) -> bool {
        let payload = self.payload.0.as_ref().and_then(|p| p.kind.as_deref());
        match self.kind.as_deref() {
            Some("session_meta" | "turn_context" | "token_usage_record" | "compacted") => true,
            Some("event_msg") => matches!(
                payload,
                Some("user_message" | "item_completed" | "token_count" | "turn_aborted")
            ),
            Some("response_item") => matches!(
                payload,
                Some(
                    "function_call"
                        | "custom_tool_call"
                        | "local_shell_call"
                        | "function_call_output"
                        | "custom_tool_call_output"
                )
            ),
            _ => false,
        }
    }
}

impl Provider for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn name(&self) -> &'static str {
        "Codex CLI"
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        Self::home()
            .map(|h| vec![h.join("sessions"), h.join("archived_sessions")])
            .unwrap_or_default()
    }

    fn installed(&self) -> bool {
        Self::home().is_some_and(|h| h.is_dir()) || super::on_path("codex")
    }

    fn matches(&self, path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "jsonl")
            && path
                .file_name()
                .and_then(|f| f.to_str())
                .is_some_and(|f| f.starts_with("rollout-"))
            && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn knows(&self, path: &Path) -> bool {
        self.state.lock().is_ok_and(|s| s.contains_key(path))
    }

    fn reset(&self, path: &Path) {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(path);
    }

    fn parse_line(&self, path: &Path, line: &str) -> Result<Vec<Record>> {
        // Sonda: tipo y fecha; las líneas que no producen filas ni cambian el estado (razonamiento,
        // mensajes del modelo…) se descartan sin construir el `Value` de todo su contenido.
        let Obj(Some(probe)) = serde_json::from_str::<Obj<Probe>>(line)? else {
            return Ok(vec![]);
        };
        let Some(ts) = probe.timestamp.as_deref().and_then(parse_ts) else {
            return Ok(vec![]);
        };
        let mut states = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let st = states.entry(path.to_path_buf()).or_default();
        if !probe.is_useful() {
            return Ok(vec![]);
        }
        let Obj(Some(parsed)) = serde_json::from_str::<Obj<Line>>(line)? else {
            return Ok(vec![]);
        };
        let empty = Payload::default();
        let payload = parsed.payload.0.as_ref().unwrap_or(&empty);
        let payload_kind = payload.kind.as_deref();
        let mut out = Vec::new();

        match probe.kind.as_deref() {
            Some("session_meta") => {
                st.thread_id = payload
                    .id
                    .as_deref()
                    .or(payload.session_id.as_deref())
                    .unwrap_or("")
                    .to_string();
                st.cwd = payload.cwd.as_deref().map(str::to_string);
                st.branch = payload
                    .git
                    .0
                    .as_ref()
                    .and_then(|g| g.branch.as_deref())
                    .map(str::to_string);
                st.is_subagent = payload.parent_thread_id.0.is_some()
                    || payload.thread_source.as_deref() == Some("subagent")
                    || payload.source.0.as_ref().is_some_and(|s| s.sub_agent.0);
            }
            Some("turn_context") => {
                if let Some(m) = payload.model.as_deref() {
                    st.model = m.to_string();
                }
                if let Some(c) = payload.cwd.as_deref() {
                    st.cwd = Some(c.to_string());
                }
                let id = payload
                    .turn_id
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{}:{ts}", st.thread_id));
                st.turn_id = Some(id.clone());
                if !st.is_subagent && !st.thread_id.is_empty() {
                    out.push(Record::Turn(TurnRec {
                        id,
                        session_id: st.thread_id.clone(),
                        ts,
                        intent: st.pending_intent.take(),
                    }));
                }
            }
            Some("token_usage_record") => {
                st.has_usage_records = true;
                push_call(
                    &mut out,
                    st,
                    payload.response_id.as_deref().unwrap_or("").to_string(),
                    payload.usage.0.as_ref().unwrap_or(&TokenUsage::default()),
                    ts,
                );
            }
            Some("event_msg") => {
                let item = payload.item.0.as_ref();
                match payload_kind {
                    Some("user_message") | Some("item_completed")
                        if payload_kind == Some("user_message")
                            || item.and_then(|i| i.kind.as_deref()) == Some("UserMessage") =>
                    {
                        if payload_kind == Some("item_completed") {
                            st.has_items = true;
                        }
                        // La intención se calcula ya: el texto del prompt no se guarda.
                        let intent = match item {
                            Some(it) if payload_kind == Some("item_completed") => {
                                prompt_intent(&item_text(it))
                            }
                            _ => prompt_intent(payload.message.as_deref().unwrap_or("")),
                        };
                        match &st.turn_id {
                            // El prompt llega tras su turn_context: completa la intención del turno.
                            Some(turn) if !st.is_subagent => out.push(Record::Turn(TurnRec {
                                id: turn.clone(),
                                session_id: st.thread_id.clone(),
                                ts,
                                intent,
                            })),
                            _ => st.pending_intent = intent,
                        }
                    }
                    Some("token_count") if !st.has_usage_records => {
                        if let Some(usage) = payload
                            .info
                            .0
                            .as_ref()
                            .and_then(|i| i.last_token_usage.0.as_ref())
                        {
                            let id = format!("tc:{}:{ts}", st.thread_id);
                            push_call(&mut out, st, id, usage, ts);
                        }
                    }
                    Some("item_completed") => {
                        st.has_items = true;
                        let empty_item = Item::default();
                        let it = item.unwrap_or(&empty_item);
                        let kind = it.kind.as_deref();
                        // Subagente arrancado: enlaza la llamada `spawn_agent` (mismo id) con su hilo.
                        if kind == Some("SubAgentActivity")
                            && it.sub_kind.as_deref() == Some("started")
                        {
                            if let (Some(call_id), Some(thread)) =
                                (it.id.as_deref(), it.agent_thread_id.as_deref())
                            {
                                out.push(Record::ToolResult {
                                    call_id: call_id.to_string(),
                                    ts,
                                    is_error: false,
                                    agent_id: Some(thread.to_string()),
                                });
                            }
                        }
                        if kind == Some("ContextCompaction") {
                            out.push(Record::Event(EventRec {
                                session_id: st.thread_id.clone(),
                                ts,
                                kind: "compaction".into(),
                                payload_json: None,
                            }));
                        }
                        for ItemTool {
                            call_id,
                            tool,
                            target,
                            detail,
                            is_error,
                            duration_ms: dur,
                        } in item_tools(it)
                        {
                            // El item llega al terminar: la herramienta empezó `dur` antes.
                            let start = ts - dur.unwrap_or(0);
                            out.push(Record::ToolUse(ToolUseRec {
                                call_id: call_id.clone(),
                                message_id: st.turn_id.clone().unwrap_or_default(),
                                session_id: st.thread_id.clone(),
                                ts: start,
                                tool,
                                target,
                                detail,
                            }));
                            out.push(Record::ToolResult {
                                call_id,
                                ts,
                                is_error,
                                agent_id: None,
                            });
                        }
                    }
                    Some("turn_aborted") => out.push(Record::Event(EventRec {
                        session_id: st.thread_id.clone(),
                        ts,
                        kind: "interruption".into(),
                        payload_json: None,
                    })),
                    _ => {}
                }
            }
            Some("compacted") => out.push(Record::Event(EventRec {
                session_id: st.thread_id.clone(),
                ts,
                kind: "compaction".into(),
                payload_json: None,
            })),
            Some("response_item") => match payload_kind {
                // Formato nuevo: las herramientas salen de `item_completed`, y `exec` es solo la
                // envoltura del modo código (el JavaScript que llama a las herramientas reales).
                // `spawn_agent` sí se lee siempre: el item del subagente no dice su tipo.
                Some("function_call") | Some("custom_tool_call") | Some("local_shell_call")
                    if (!st.has_items || payload.name.as_deref() == Some("spawn_agent"))
                        && payload.name.as_deref() != Some("exec") =>
                {
                    let raw = payload.name.as_deref().unwrap_or("local_shell");
                    // Los namespaces propios de Codex no son servidores MCP (p. ej. `spawn_agent`
                    // viene en `collaboration`).
                    let namespace = payload.namespace.as_deref().filter(|n| {
                        !matches!(*n, "functions" | "collaboration" | "multi_tool_use")
                    });
                    let tool = match namespace {
                        Some(ns) => format!("mcp__{ns}__{raw}"),
                        None => canonical_tool(raw).to_string(),
                    };
                    let no_args = Args::default();
                    let custom;
                    let from_text;
                    let args: &Args = match payload_kind {
                        Some("local_shell_call") => payload.action.0.as_ref().unwrap_or(&no_args),
                        Some("custom_tool_call") => {
                            custom = Args {
                                input: payload.input.clone(),
                                ..Default::default()
                            };
                            &custom
                        }
                        _ => {
                            from_text = payload
                                .arguments
                                .as_deref()
                                .and_then(|a| serde_json::from_str::<Obj<Args>>(a).ok())
                                .and_then(|o| o.0);
                            from_text.as_ref().unwrap_or(&no_args)
                        }
                    };
                    let detail = (tool == "Agent").then(|| {
                        // Sin rol pedido, Codex usa su rol por defecto: `default`.
                        args.agent_type
                            .as_deref()
                            .or(args.agent_role.as_deref())
                            .or(args.role.as_deref())
                            .unwrap_or("default")
                            .to_string()
                    });
                    let Some(call_id) = payload.call_id.as_deref().or(payload.id.as_deref()) else {
                        return Ok(out);
                    };
                    out.push(Record::ToolUse(ToolUseRec {
                        call_id: call_id.to_string(),
                        message_id: st.turn_id.clone().unwrap_or_default(),
                        session_id: st.thread_id.clone(),
                        ts,
                        target: tool_target(&tool, args),
                        tool,
                        detail,
                    }));
                }
                Some("function_call_output") | Some("custom_tool_call_output") => {
                    if let Some(call_id) = payload.call_id.as_deref() {
                        out.push(Record::ToolResult {
                            call_id: call_id.to_string(),
                            ts,
                            is_error: output_is_error(&payload.output),
                            agent_id: None,
                        });
                    }
                }
                _ => {}
            },
            _ => {}
        }

        if !out.is_empty() && !st.thread_id.is_empty() {
            out.insert(
                0,
                Record::Session(SessionRec {
                    id: st.thread_id.clone(),
                    cwd: st.cwd.clone(),
                    git_branch: st.branch.clone(),
                    ts,
                    is_subagent: st.is_subagent,
                }),
            );
        }
        Ok(out)
    }
}

fn push_call(
    out: &mut Vec<Record>,
    st: &FileState,
    message_id: String,
    usage: &TokenUsage,
    ts: i64,
) {
    if st.thread_id.is_empty() || message_id.is_empty() {
        return;
    }
    // `input_tokens` de Codex incluye los tokens leídos de caché.
    let cached = usage.cached_input_tokens.0;
    out.push(Record::Call(CallRec {
        message_id,
        session_id: st.thread_id.clone(),
        ts,
        model: crate::pricing::normalize_model(&st.model),
        input_tokens: (usage.input_tokens.0 - cached).max(0),
        output_tokens: usage.output_tokens.0,
        cache_read: cached,
        cache_write: usage.cache_write_input_tokens.0,
        cache_write_1h: 0,
        reasoning_tokens: usage.reasoning_output_tokens.0,
        activity: None,
        is_sidechain: st.is_subagent,
        agent_id: st.is_subagent.then(|| st.thread_id.clone()),
        cost_reported: None,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_sonda_solo_deja_pasar_los_tipos_que_producen_filas_o_estado() {
        let probe = |line: &str| -> bool {
            let Obj(Some(p)) = serde_json::from_str::<Obj<Probe>>(line).unwrap() else {
                return false;
            };
            p.is_useful()
        };
        let line = |t: &str, payload: &str| {
            format!(r#"{{"timestamp":"2026-09-24T09:00:00Z","type":"{t}","payload":{payload}}}"#)
        };
        for (t, pt) in [
            ("response_item", "reasoning"),
            ("response_item", "message"),
            ("response_item", "web_search_call"),
            ("event_msg", "agent_message"),
            ("event_msg", "agent_reasoning"),
            ("event_msg", "task_complete"),
            ("event_msg", "exec_command_end"),
            ("desconocido", "x"),
        ] {
            assert!(
                !probe(&line(t, &format!(r#"{{"type":"{pt}"}}"#))),
                "{t}/{pt}"
            );
        }
        for (t, pt) in [
            ("session_meta", "x"),
            ("turn_context", "x"),
            ("token_usage_record", "x"),
            ("compacted", "x"),
            ("event_msg", "user_message"),
            ("event_msg", "item_completed"),
            ("event_msg", "token_count"),
            ("event_msg", "turn_aborted"),
            ("response_item", "function_call"),
            ("response_item", "custom_tool_call"),
            ("response_item", "local_shell_call"),
            ("response_item", "function_call_output"),
            ("response_item", "custom_tool_call_output"),
        ] {
            assert!(
                probe(&line(t, &format!(r#"{{"type":"{pt}"}}"#))),
                "{t}/{pt}"
            );
        }
    }

    #[test]
    fn una_linea_descartada_no_produce_filas_pero_cuenta_como_leida() {
        let c = Codex::with_roots(vec![]);
        let path = Path::new("rollout-x.jsonl");
        let reasoning = r#"{"timestamp":"2026-09-24T09:00:00Z","type":"response_item","payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"pienso"}],"encrypted_content":"x"}}"#;
        assert!(c.parse_line(path, reasoning).unwrap().is_empty());
        assert!(c.knows(path), "como antes: el archivo ya se empezó a leer");
        assert!(c.parse_line(path, "[1]").unwrap().is_empty());
        assert!(c.parse_line(path, "{roto").is_err());
    }

    fn output(json: &str) -> Output<'_> {
        serde_json::from_str(json).unwrap()
    }

    fn args(json: &str) -> Args<'_> {
        serde_json::from_str::<Obj<Args>>(json).unwrap().0.unwrap()
    }

    #[test]
    fn error_de_shell_en_texto_plano() {
        assert!(output_is_error(&output(
            r#""Process exited with code 1\nFinal output:\nfailed""#
        )));
        assert!(!output_is_error(&output(
            r#""Process exited with code 0\nok""#
        )));
        assert!(output_is_error(&output(
            r#""Process exited with code -1\n""#
        )));
        assert!(!output_is_error(&output(
            r#""texto cualquiera con error dentro""#
        )));
        assert!(output_is_error(&output(r#"{"success": false}"#)));
    }

    #[test]
    fn objetivo_de_shell_y_parche() {
        let a = args(r#"{ "command": ["bash", "-lc", "git status | head"], "workdir": "/p" }"#);
        assert_eq!(
            tool_target("Bash", &a).as_deref(),
            Some("git status | head")
        );
        let a = args(r#"{ "command": ["ls", "-la"] }"#);
        assert_eq!(tool_target("Bash", &a).as_deref(), Some("ls -la"));
        let a = args(r#"{ "command": "make", "cmd": "otro" }"#);
        assert_eq!(tool_target("Bash", &a).as_deref(), Some("make"));
        let a = args(r#"{ "command": {"x": 1}, "cmd": "otro" }"#);
        assert_eq!(tool_target("Bash", &a).as_deref(), Some("otro"));
        let patch = args(
            r#"{ "input": "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-a\n+b\n*** End Patch" }"#,
        );
        assert_eq!(tool_target("Edit", &patch).as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn error_en_la_salida() {
        assert!(output_is_error(&output(
            r#"{ "content": "x", "success": false }"#
        )));
        assert!(!output_is_error(&output(
            r#"{ "content": "x", "success": true }"#
        )));
        assert!(!output_is_error(&output(r#"{ "content": "x" }"#)));
        assert!(output_is_error(&output(
            r#""{\"output\":\"boom\",\"metadata\":{\"exit_code\":1,\"duration_seconds\":0.1}}""#
        )));
        assert!(!output_is_error(&output(r#""todo bien""#)));
        assert!(!output_is_error(&output("[true]")));
    }

    #[test]
    fn detecta_rollouts() {
        let p = Codex::with_roots(vec![PathBuf::from("/h/.codex/sessions")]);
        assert!(p.matches(Path::new(
            "/h/.codex/sessions/2026/09/25/rollout-2026-09-25T10-00-00-abc.jsonl"
        )));
        assert!(!p.matches(Path::new("/h/.codex/sessions/2026/09/25/otro.jsonl")));
        assert!(!p.matches(Path::new("/h/.codex/history.jsonl")));
    }

    #[test]
    fn formato_nuevo_con_item_completed() {
        let p = Codex::with_roots(vec![PathBuf::from("/h/.codex/sessions")]);
        let f = PathBuf::from("/h/.codex/sessions/2026/09/27/rollout-x.jsonl");
        let lines = [
            r#"{"timestamp":"2026-09-27T16:00:00.000Z","type":"session_meta","payload":{"id":"t1","cwd":"/w","git":{"branch":"feat/v2"}}}"#,
            r#"{"timestamp":"2026-09-27T16:00:01.000Z","type":"turn_context","payload":{"turn_id":"tu1","model":"gpt-6-astra","cwd":"/w"}}"#,
            r#"{"timestamp":"2026-09-27T16:00:02.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","id":"u1","content":[{"type":"text","text":"arregla el test que falla"}]}}}"#,
            // Envoltura del modo código: no es una herramienta.
            r#"{"timestamp":"2026-09-27T16:00:03.000Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","call_id":"c1","input":"await tools.shell('ls')"}}"#,
            r#"{"timestamp":"2026-09-27T16:00:05.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"e1","command":["/bin/zsh","-lc","cargo test --lib"],"source":"unified_exec_startup","status":"failed","exit_code":101,"duration":{"secs":2,"nanos":500000000}}}}"#,
            r#"{"timestamp":"2026-09-27T16:00:06.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","id":"f1","changes":{"/w/a.rs":{"type":"update"},"/w/b.rs":{"type":"add"}},"status":"completed"}}}"#,
            r#"{"timestamp":"2026-09-27T16:00:07.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","id":"m1","server":"agentboard","tool":"get_summary","arguments":{},"status":"failed","error":{"message":"x"},"duration":{"secs":0,"nanos":180000000}}}}"#,
            r#"{"timestamp":"2026-09-27T16:00:08.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"Extension","kind":"web.search","id":"w1","query":"https://example.com","action":{"type":"openPage","url":"https://example.com"}}}}"#,
            r#"{"timestamp":"2026-09-27T16:00:09.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"ContextCompaction","id":"cc1"}}}"#,
            r#"{"timestamp":"2026-09-27T16:00:10.000Z","type":"response_item","payload":{"type":"function_call","name":"spawn_agent","namespace":"collaboration","call_id":"call_sa","arguments":"{\"task_name\":\"mates_01\",\"fork_turns\":\"none\"}"}}"#,
            r#"{"timestamp":"2026-09-27T16:00:11.000Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"SubAgentActivity","id":"call_sa","kind":"started","agent_thread_id":"hijo-1","agent_path":"/root/mates_01"}}}"#,
        ];
        let recs: Vec<Record> = lines
            .iter()
            .flat_map(|l| p.parse_line(&f, l).unwrap())
            .collect();
        let tools: Vec<(&str, Option<&str>)> = recs
            .iter()
            .filter_map(|r| match r {
                Record::ToolUse(t) => Some((t.tool.as_str(), t.target.as_deref())),
                _ => None,
            })
            .collect();
        assert_eq!(
            tools,
            vec![
                ("Bash", Some("cargo test --lib")),
                ("Edit", Some("/w/a.rs")),
                ("Write", Some("/w/b.rs")),
                ("mcp__agentboard__get_summary", None),
                ("WebFetch", Some("https://example.com")),
                ("Agent", None),
            ],
            "sin la envoltura exec"
        );
        // El subagente sin rol pedido es de tipo `default` y queda enlazado a su hilo.
        assert!(recs.iter().any(|r| matches!(r, Record::ToolUse(t) if t.tool == "Agent" && t.detail.as_deref() == Some("default"))));
        assert!(recs.iter().any(|r| matches!(r, Record::ToolResult { call_id, agent_id: Some(a), .. } if call_id == "call_sa" && a == "hijo-1")));
        let errors: Vec<bool> = recs
            .iter()
            .filter_map(|r| match r {
                Record::ToolResult { is_error, .. } => Some(*is_error),
                _ => None,
            })
            .collect();
        assert_eq!(
            errors,
            vec![true, false, false, true, false, false],
            "cargo y el MCP fallaron"
        );
        // El comando duró 2,5 s: empieza 2,5 s antes de su item.
        let bash = recs.iter().find_map(|r| match r {
            Record::ToolUse(t) if t.tool == "Bash" => Some(t.ts),
            _ => None,
        });
        assert_eq!(
            bash,
            Some(crate::providers::parse_ts("2026-09-27T16:00:02.500Z").unwrap())
        );
        assert!(recs
            .iter()
            .any(|r| matches!(r, Record::Event(e) if e.kind == "compaction")));
        // El prompt del UserMessage da la intención del turno.
        assert!(recs
            .iter()
            .any(|r| matches!(r, Record::Turn(t) if t.intent == Some("debug"))));
    }
}
