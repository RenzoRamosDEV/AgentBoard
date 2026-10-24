//! Gemini CLI: `~/.gemini/tmp/<proyecto>/chats/session-<fecha>-<id>.jsonl` (`GEMINI_CLI_HOME` lo cambia).
//! Primera línea: metadatos `{sessionId, projectHash, startTime, kind, directories?}`; después un
//! mensaje por línea (`type: user | gemini | info…`, con `tokens`, `model` y `toolCalls` en los de
//! Gemini) y actualizaciones `{"$set": {...}}`. Los subagentes van en `chats/<sesión padre>/<id>.jsonl`.
//! Las versiones antiguas escribían un único `.json` con el mismo contenido.

use super::json::{Int, List, Obj, Present, Seen, Str};
use super::{parse_ts, prompt_intent, CallRec, Provider, Record, SessionRec, ToolUseRec, TurnRec};
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Default)]
pub struct Gemini {
    roots: Option<Vec<PathBuf>>,
    state: Mutex<HashMap<PathBuf, FileState>>,
}

#[derive(Default)]
struct FileState {
    session_id: String,
    cwd: Option<String>,
    is_subagent: bool,
    turns: u32,
    /// Último modelo visto, para mensajes sin él.
    model: String,
}

impl Gemini {
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        Self {
            roots: Some(roots),
            state: Mutex::default(),
        }
    }

    fn home() -> Option<PathBuf> {
        match std::env::var_os("GEMINI_CLI_HOME") {
            Some(h) if !h.is_empty() => Some(PathBuf::from(h).join(".gemini")),
            _ => dirs::home_dir().map(|h| h.join(".gemini")),
        }
    }

    /// Carpeta del proyecto de un archivo de sesión. Las versiones recientes de Gemini CLI no la
    /// escriben en el log: está en `tmp/<proyecto>/.project_root` o, si falta, en
    /// `~/.gemini/projects.json` (`{"projects": {"<ruta>": "<proyecto>"}}`).
    pub fn project_root_of(path: &Path) -> Option<String> {
        for dir in path.ancestors().skip(1) {
            if let Ok(root) = std::fs::read_to_string(dir.join(".project_root")) {
                let root = root.trim();
                if !root.is_empty() {
                    return Some(root.to_string());
                }
            }
            let (Some(name), Some(parent)) = (dir.file_name(), dir.parent()) else {
                break;
            };
            if parent.file_name().is_some_and(|n| n == "tmp") {
                // `dir` es `tmp/<proyecto>`: se busca en projects.json, junto a `tmp/`.
                let map: Value = std::fs::read_to_string(parent.parent()?.join("projects.json"))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok())?;
                return map["projects"]
                    .as_object()?
                    .iter()
                    .find_map(|(root, v)| (v.as_str() == name.to_str()).then(|| root.clone()));
            }
        }
        None
    }

    fn apply_meta(st: &mut FileState, meta: &Meta) {
        if let Some(id) = meta.session_id.as_deref() {
            st.session_id = id.to_string();
        }
        if let Some(dir) = meta.directories.0.first().and_then(|d| d.as_deref()) {
            st.cwd = Some(dir.to_string());
        }
        if meta.kind.as_deref() == Some("subagent") {
            st.is_subagent = true;
        }
    }

    fn records_for_message<C: MessageText>(
        st: &mut FileState,
        m: &Message<C>,
        out: &mut Vec<Record>,
    ) {
        let Some(ts) = m.timestamp.as_deref().and_then(parse_ts) else {
            return;
        };
        let id = m.id.as_deref().unwrap_or("").to_string();
        match m.kind.as_deref() {
            Some("user") if !st.is_subagent => {
                st.turns += 1;
                let text = m.content.text();
                out.push(Record::Turn(TurnRec {
                    id: if id.is_empty() {
                        format!("{}:{}", st.session_id, st.turns)
                    } else {
                        id
                    },
                    session_id: st.session_id.clone(),
                    ts,
                    intent: prompt_intent(&text),
                }));
            }
            Some("gemini") => {
                if let Some(model) = m.model.as_deref() {
                    st.model = model.to_string();
                }
                if let (Some(t), false) = (m.tokens.0.as_ref(), id.is_empty()) {
                    let cached = t.cached.0;
                    out.push(Record::Call(CallRec {
                        message_id: id.clone(),
                        session_id: st.session_id.clone(),
                        ts,
                        model: crate::pricing::normalize_model(&st.model),
                        // `input` (promptTokenCount) incluye lo servido desde caché.
                        input_tokens: (t.input.0 - cached).max(0),
                        output_tokens: t.output.0 + t.thoughts.0,
                        cache_read: cached,
                        cache_write: 0,
                        cache_write_1h: 0,
                        reasoning_tokens: t.thoughts.0,
                        activity: None,
                        is_sidechain: st.is_subagent,
                        agent_id: st.is_subagent.then(|| st.session_id.clone()),
                        cost_reported: None,
                    }));
                }
                let no_args = Args::default();
                for c in m.tool_calls.0.iter().filter_map(|c| c.0.as_ref()) {
                    let Some(call_id) = c.id.as_deref() else {
                        continue;
                    };
                    let raw = c.name.as_deref().unwrap_or("?");
                    let tool = canonical_tool(raw).to_string();
                    let args = c.args.0.as_ref().unwrap_or(&no_args);
                    let target = [
                        &args.command,
                        &args.file_path,
                        &args.absolute_path,
                        &args.path,
                        &args.pattern,
                        &args.query,
                        &args.url,
                        &args.prompt,
                    ]
                    .iter()
                    .find_map(|k| k.as_deref())
                    .map(|s| s.chars().take(500).collect::<String>());
                    let detail = match tool.as_str() {
                        "Agent" => Some(
                            args.agent_name
                                .as_deref()
                                .or(args.subagent_type.as_deref())
                                .or(args.agent.as_deref())
                                .or(args.name.as_deref())
                                .unwrap_or("general-purpose")
                                .to_string(),
                        ),
                        "Skill" => args
                            .name
                            .as_deref()
                            .or(args.skill.as_deref())
                            .map(str::to_string),
                        _ => None,
                    };
                    let cts = c.timestamp.as_deref().and_then(parse_ts).unwrap_or(ts);
                    out.push(Record::ToolUse(ToolUseRec {
                        call_id: call_id.to_string(),
                        message_id: id.clone(),
                        session_id: st.session_id.clone(),
                        ts: cts,
                        tool,
                        target,
                        detail,
                    }));
                    if let Some(status) = c.status.as_deref() {
                        if status != "executing"
                            && status != "scheduled"
                            && status != "validating"
                            && status != "awaiting_approval"
                        {
                            out.push(Record::ToolResult {
                                call_id: call_id.to_string(),
                                ts: cts,
                                is_error: matches!(status, "error" | "cancelled"),
                                agent_id: c.agent_id.as_deref().map(str::to_string),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Qué es una línea, según qué claves tiene (en el orden en que se comprueban).
#[derive(Debug, Clone, PartialEq)]
enum LineKind {
    /// `{"$set": {...}}`: actualización de metadatos.
    Set,
    /// Documento completo del formato antiguo, con `messages`.
    Document,
    /// Metadatos de la sesión (primera línea).
    Meta,
    /// Un mensaje, con su `type` si es una cadena.
    Message(Option<String>),
    Other,
}

/// Sonda: qué claves de envoltura tiene la línea, sin leer su contenido.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Probe<'a> {
    #[serde(rename = "$set")]
    set: Seen,
    messages: Seen,
    #[serde(rename = "sessionId")]
    session_id: Seen,
    #[serde(rename = "type", borrow)]
    kind: Present<Str<'a>>,
}

impl Probe<'_> {
    fn kind(&self) -> LineKind {
        match &self.kind.0 {
            _ if self.set.0 => LineKind::Set,
            _ if self.messages.0 => LineKind::Document,
            None if self.session_id.0 => LineKind::Meta,
            Some(t) => LineKind::Message(t.as_deref().map(str::to_string)),
            None => LineKind::Other,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Meta<'a> {
    #[serde(rename = "sessionId", borrow)]
    session_id: Str<'a>,
    #[serde(borrow)]
    directories: List<Str<'a>>,
    #[serde(borrow)]
    kind: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SetLine<'a> {
    #[serde(rename = "$set", borrow)]
    set: Obj<Meta<'a>>,
}

/// Documento del formato antiguo: metadatos y todos los mensajes (formato poco frecuente: el
/// texto de todos sus mensajes se lee).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Document<'a> {
    #[serde(rename = "sessionId", borrow)]
    session_id: Str<'a>,
    #[serde(borrow)]
    directories: List<Str<'a>>,
    #[serde(borrow)]
    kind: Str<'a>,
    #[serde(borrow)]
    messages: List<Obj<Message<'a, Content<'a>>>>,
}

/// Un mensaje. `C` decide si se lee `content` (solo hace falta en los del usuario).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Message<'a, C> {
    #[serde(borrow)]
    timestamp: Str<'a>,
    #[serde(borrow)]
    id: Str<'a>,
    #[serde(rename = "type", borrow)]
    kind: Str<'a>,
    content: C,
    #[serde(borrow)]
    model: Str<'a>,
    tokens: Obj<Tokens>,
    #[serde(rename = "toolCalls", borrow)]
    tool_calls: List<Obj<ToolCall<'a>>>,
}

trait MessageText {
    fn text(&self) -> String;
}

/// `content` que no se lee (respuestas del modelo, mensajes informativos).
#[derive(Debug, Default)]
struct Skipped;

impl<'de> Deserialize<'de> for Skipped {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(d).map(|_| Skipped)
    }
}

impl MessageText for Skipped {
    fn text(&self) -> String {
        String::new()
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Tokens {
    input: Int,
    cached: Int,
    output: Int,
    thoughts: Int,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ToolCall<'a> {
    #[serde(borrow)]
    id: Str<'a>,
    #[serde(borrow)]
    name: Str<'a>,
    #[serde(borrow)]
    args: Obj<Args<'a>>,
    #[serde(borrow)]
    timestamp: Str<'a>,
    #[serde(borrow)]
    status: Str<'a>,
    #[serde(rename = "agentId", borrow)]
    agent_id: Str<'a>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Args<'a> {
    #[serde(borrow)]
    command: Str<'a>,
    #[serde(borrow)]
    file_path: Str<'a>,
    #[serde(borrow)]
    absolute_path: Str<'a>,
    #[serde(borrow)]
    path: Str<'a>,
    #[serde(borrow)]
    pattern: Str<'a>,
    #[serde(borrow)]
    query: Str<'a>,
    #[serde(borrow)]
    url: Str<'a>,
    #[serde(borrow)]
    prompt: Str<'a>,
    #[serde(borrow)]
    agent_name: Str<'a>,
    #[serde(borrow)]
    subagent_type: Str<'a>,
    #[serde(borrow)]
    agent: Str<'a>,
    #[serde(borrow)]
    name: Str<'a>,
    #[serde(borrow)]
    skill: Str<'a>,
}

/// `content` de Gemini: cadena, lista de partes (`{text}` o cadenas) u objeto `{text}`.
#[derive(Debug, Default)]
enum Content<'a> {
    Text(Cow<'a, str>),
    Parts(Vec<Part<'a>>),
    #[default]
    Other,
}

impl MessageText for Content<'_> {
    /// Texto plano, con las partes unidas por espacios.
    fn text(&self) -> String {
        match self {
            Content::Text(t) => t.to_string(),
            Content::Parts(parts) => parts
                .iter()
                .filter_map(|p| p.0.as_deref())
                .collect::<Vec<_>>()
                .join(" "),
            Content::Other => String::new(),
        }
    }
}

/// Texto de una parte: la propia cadena o su campo `text`.
#[derive(Debug, Default)]
struct Part<'a>(Option<Cow<'a, str>>);

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TextField<'a> {
    #[serde(borrow)]
    text: Str<'a>,
}

/// Visitante común de `Content` y `Part`: cadena, lista de partes u objeto `{text}`.
struct TextVisitor<'a, const PARTS: bool>(std::marker::PhantomData<&'a ()>);

impl<'de: 'a, 'a, const PARTS: bool> serde::de::Visitor<'de> for TextVisitor<'a, PARTS> {
    type Value = Content<'a>;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("cualquier valor JSON")
    }
    fn visit_borrowed_str<E: serde::de::Error>(
        self,
        v: &'de str,
    ) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Text(Cow::Borrowed(v)))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Text(Cow::Owned(v.to_owned())))
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Text(Cow::Owned(v)))
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(
        self,
        mut seq: A,
    ) -> std::result::Result<Content<'a>, A::Error> {
        if !PARTS {
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            return Ok(Content::Other);
        }
        let mut parts = Vec::new();
        while let Some(p) = seq.next_element::<Part<'a>>()? {
            parts.push(p);
        }
        Ok(Content::Parts(parts))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(
        self,
        map: A,
    ) -> std::result::Result<Content<'a>, A::Error> {
        let f = TextField::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
        Ok(f.text.0.map_or(Content::Other, Content::Text))
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Other)
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Other)
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Other)
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Other)
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Content<'a>, E> {
        Ok(Content::Other)
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Content<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        d.deserialize_any(TextVisitor::<'a, true>(std::marker::PhantomData))
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for Part<'a> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        // Una parte es la cadena o el `text` de un objeto; una lista anidada no cuenta.
        Ok(
            match d.deserialize_any(TextVisitor::<'a, false>(std::marker::PhantomData))? {
                Content::Text(t) => Part(Some(t)),
                _ => Part(None),
            },
        )
    }
}

/// Nombres de herramienta de Gemini CLI → los de Claude Code.
fn canonical_tool(name: &str) -> &str {
    match name {
        "run_shell_command" | "shell" => "Bash",
        "read_file" | "read_many_files" => "Read",
        "write_file" => "Write",
        "replace" | "edit" | "smart_edit" => "Edit",
        "search_file_content" | "grep_search" | "grep" => "Grep",
        "glob" | "list_directory" => "Glob",
        "web_fetch" => "WebFetch",
        "google_web_search" | "web_search" => "WebSearch",
        "write_todos" | "save_memory" => "TodoWrite",
        "invoke_agent" | "delegate_to_agent" | "subagent" | "task" => "Agent",
        "activate_skill" | "skill" => "Skill",
        "ask_user" => "AskUserQuestion",
        other => other,
    }
}

impl Provider for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn name(&self) -> &'static str {
        "Gemini CLI"
    }

    fn log_roots(&self) -> Vec<PathBuf> {
        if let Some(r) = &self.roots {
            return r.clone();
        }
        Self::home()
            .map(|h| vec![h.join("tmp")])
            .unwrap_or_default()
    }

    fn installed(&self) -> bool {
        Self::home().is_some_and(|h| h.is_dir()) || super::on_path("gemini")
    }

    fn matches(&self, path: &Path) -> bool {
        let in_chats = path.components().any(|c| c.as_os_str() == "chats");
        let ext = path.extension().and_then(|e| e.to_str());
        in_chats
            && matches!(ext, Some("jsonl") | Some("json"))
            && self.log_roots().iter().any(|r| path.starts_with(r))
    }

    fn whole_file(&self, path: &Path) -> bool {
        // El historial antiguo es un único documento `.json` con `messages`.
        path.extension().and_then(|e| e.to_str()) == Some("json")
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
        // Sonda: qué claves de envoltura tiene; el contenido se lee después según el tipo.
        let probe = serde_json::from_str::<Obj<Probe>>(line)?.0;
        let mut states = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let st = states
            .entry(path.to_path_buf())
            .or_insert_with(|| FileState {
                cwd: Self::project_root_of(path),
                ..Default::default()
            });
        let mut out = Vec::new();

        match probe.map(|p| p.kind()) {
            Some(LineKind::Set) => {
                if let Obj(Some(SetLine {
                    set: Obj(Some(meta)),
                })) = serde_json::from_str(line)?
                {
                    Self::apply_meta(st, &meta);
                }
            }
            Some(LineKind::Document) => {
                // Formato antiguo: todo el registro en un solo `.json`.
                if let Obj(Some(doc)) = serde_json::from_str::<Obj<Document>>(line)? {
                    let meta = Meta {
                        session_id: doc.session_id,
                        directories: doc.directories,
                        kind: doc.kind,
                    };
                    Self::apply_meta(st, &meta);
                    for m in doc.messages.0.iter().filter_map(|m| m.0.as_ref()) {
                        Self::records_for_message(st, m, &mut out);
                    }
                }
            }
            Some(LineKind::Meta) => {
                if let Obj(Some(meta)) = serde_json::from_str::<Obj<Meta>>(line)? {
                    Self::apply_meta(st, &meta);
                }
            }
            Some(LineKind::Message(kind)) if kind.as_deref() == Some("user") => {
                if let Obj(Some(m)) = serde_json::from_str::<Obj<Message<Content>>>(line)? {
                    Self::records_for_message(st, &m, &mut out);
                }
            }
            Some(LineKind::Message(_)) => {
                // La respuesta del modelo (`content`) no se lee.
                if let Obj(Some(m)) = serde_json::from_str::<Obj<Message<Skipped>>>(line)? {
                    Self::records_for_message(st, &m, &mut out);
                }
            }
            Some(LineKind::Other) | None => {}
        }

        if !out.is_empty() && !st.session_id.is_empty() {
            let ts = out
                .iter()
                .find_map(|r| match r {
                    Record::Call(c) => Some(c.ts),
                    Record::Turn(t) => Some(t.ts),
                    Record::ToolUse(t) => Some(t.ts),
                    _ => None,
                })
                .unwrap_or(0);
            out.insert(
                0,
                Record::Session(SessionRec {
                    id: st.session_id.clone(),
                    cwd: st.cwd.clone(),
                    git_branch: None,
                    ts,
                    is_subagent: st.is_subagent,
                }),
            );
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proyecto_desde_project_root_o_projects_json() {
        let dir = tempfile::tempdir().unwrap();
        let gem = dir.path().join(".gemini");
        let chats = gem.join("tmp").join("agentboard").join("chats");
        std::fs::create_dir_all(chats.join("padre")).unwrap();
        let file = chats.join("session-1.jsonl");
        let sub = chats.join("padre").join("sub.jsonl");
        // Sin pistas: sin proyecto.
        assert_eq!(Gemini::project_root_of(&file), None);
        // projects.json relaciona la ruta con la carpeta `agentboard`.
        std::fs::write(
            gem.join("projects.json"),
            r#"{"projects":{"/home/u/Proyectos/AgentBoard":"agentboard"}}"#,
        )
        .unwrap();
        assert_eq!(
            Gemini::project_root_of(&file).as_deref(),
            Some("/home/u/Proyectos/AgentBoard")
        );
        // .project_root manda, también para los subagentes.
        std::fs::write(
            gem.join("tmp").join("agentboard").join(".project_root"),
            "/var/home/u/Proyectos/AgentBoard\n",
        )
        .unwrap();
        assert_eq!(
            Gemini::project_root_of(&file).as_deref(),
            Some("/var/home/u/Proyectos/AgentBoard")
        );
        assert_eq!(
            Gemini::project_root_of(&sub).as_deref(),
            Some("/var/home/u/Proyectos/AgentBoard")
        );

        // Y la sesión sale con ese proyecto aunque el log no traiga `directories`.
        let p = Gemini::with_roots(vec![gem.join("tmp")]);
        let recs = p
            .parse_line(&file, r#"{"sessionId":"s1","projectHash":"x","startTime":"2026-09-27T17:04:51.916Z","kind":"main"}"#)
            .unwrap();
        assert!(recs.is_empty());
        let recs = p
            .parse_line(&file, r#"{"id":"u1","timestamp":"2026-09-27T17:05:00.000Z","type":"user","content":[{"text":"hola"}]}"#)
            .unwrap();
        let Some(Record::Session(s)) = recs.first() else {
            panic!("falta la sesión")
        };
        assert_eq!(s.cwd.as_deref(), Some("/var/home/u/Proyectos/AgentBoard"));
    }

    #[test]
    fn detecta_sesiones() {
        let p = Gemini::with_roots(vec![PathBuf::from("/h/.gemini/tmp")]);
        assert!(p.matches(Path::new(
            "/h/.gemini/tmp/abc/chats/session-2026-09-25T10-00-1234abcd.jsonl"
        )));
        assert!(p.matches(Path::new("/h/.gemini/tmp/abc/chats/parent/child.jsonl")));
        assert!(!p.matches(Path::new("/h/.gemini/tmp/abc/logs.json")));
    }

    fn content(json: &str) -> String {
        serde_json::from_str::<Content>(json).unwrap().text()
    }

    #[test]
    fn texto_de_partes() {
        assert_eq!(
            content(r#"[{ "text": "hola" }, { "text": "mundo" }]"#),
            "hola mundo"
        );
        assert_eq!(
            content(r#"["hola", {"otra":1}, 5, {"text": "mundo"}]"#),
            "hola mundo"
        );
        assert_eq!(content(r#""hola""#), "hola");
        assert_eq!(content(r#"{"text": "hola", "x": [1]}"#), "hola");
        assert_eq!(content(r#"{"x": 1}"#), "");
        assert_eq!(content("7"), "");
    }

    #[test]
    fn la_sonda_distingue_los_tipos_de_linea() {
        let kind = |l: &str| {
            serde_json::from_str::<Obj<Probe>>(l)
                .unwrap()
                .0
                .map(|p| p.kind())
        };
        assert_eq!(
            kind(r#"{"$set":{"x":1},"type":"user"}"#),
            Some(LineKind::Set)
        );
        assert_eq!(
            kind(r#"{"messages":null,"sessionId":"s"}"#),
            Some(LineKind::Document)
        );
        assert_eq!(
            kind(r#"{"sessionId":"s","kind":"main"}"#),
            Some(LineKind::Meta)
        );
        assert_eq!(
            kind(r#"{"sessionId":"s","type":null}"#),
            Some(LineKind::Message(None))
        );
        assert_eq!(
            kind(r#"{"type":"gemini","content":"x"}"#),
            Some(LineKind::Message(Some("gemini".into())))
        );
        assert_eq!(kind(r#"{"otra":1}"#), Some(LineKind::Other));
        assert_eq!(kind("[1]"), None);
    }

    #[test]
    fn la_respuesta_del_modelo_no_se_lee() {
        let p = Gemini::with_roots(vec![]);
        let path = Path::new("/h/tmp/p/chats/session-1.jsonl");
        p.parse_line(
            path,
            r#"{"sessionId":"s1","kind":"main","directories":["/w", 3]}"#,
        )
        .unwrap();
        let texto = "respuesta \\\"larga\\\"\\n".repeat(50_000);
        let l = format!(
            r#"{{"id":"g1","timestamp":"2026-09-27T17:05:00.000Z","type":"gemini","content":"{texto}","model":"gemini-2.5-pro","tokens":{{"input":100,"cached":30,"output":10,"thoughts":5}},"toolCalls":[{{"id":"c1","name":"run_shell_command","args":{{"command":"ls"}},"status":"error"}},{{"name":"sin_id"}},7]}}"#
        );
        let recs = p.parse_line(path, &l).unwrap();
        let Record::Session(s) = &recs[0] else {
            panic!("{recs:?}")
        };
        assert_eq!((s.id.as_str(), s.cwd.as_deref()), ("s1", Some("/w")));
        let Record::Call(c) = &recs[1] else {
            panic!("{recs:?}")
        };
        assert_eq!(
            (
                c.input_tokens,
                c.output_tokens,
                c.cache_read,
                c.reasoning_tokens
            ),
            (70, 15, 30, 5)
        );
        assert!(recs.iter().any(|r| matches!(r, Record::ToolUse(t) if t.tool == "Bash" && t.target.as_deref() == Some("ls"))));
        assert!(recs
            .iter()
            .any(|r| matches!(r, Record::ToolResult { is_error: true, .. })));
        assert_eq!(recs.len(), 4);
    }
}
