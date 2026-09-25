//! Servidor MCP de AgentBoard (transporte stdio, JSON-RPC 2.0).
//!
//! Escanea los logs de todos los agentes a una base SQLite en memoria al arrancar (reutilizando
//! `providers` + `ingest`) y expone por MCP las mismas consultas que muestra el dashboard. No
//! necesita que la app esté abierta ni escribe nada en disco.
//!
//! Registro (ejemplo Claude Code):
//!   claude mcp add agentboard -- /ruta/a/agentboard-mcp

use agentboard_lib::{db, ingest, providers, queries};
use anyhow::{anyhow, bail, Result};
use chrono::{Offset, Timelike};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const DAY_MS: i64 = 86_400_000;
const PROTOCOL: &str = "2025-06-18";

fn main() {
    let conn = match scan() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("agentboard-mcp: no se pudo leer el historial: {e:#}");
            std::process::exit(1);
        }
    };

    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF: el cliente cerró
            Ok(_) => {}
            Err(_) => break,
        }
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(&conn, &req),
            Err(_) => Some(error(Value::Null, -32700, "Parse error")),
        };
        if let Some(resp) = resp {
            write_msg(&mut out, &resp);
        }
    }
}

/// Lee el historial del disco a una base en memoria (una sola vez, al arrancar).
fn scan() -> Result<Connection> {
    let mut conn = db::open_in_memory()?;
    ingest::scan_all(&mut conn, &providers::all())?;
    Ok(conn)
}

/// Enruta un mensaje JSON-RPC. Devuelve `None` para notificaciones (sin `id`, sin respuesta).
fn handle(conn: &Connection, req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => Some(result(id, initialize(req))),
        "ping" => Some(result(id, json!({}))),
        "tools/list" => Some(result(id, json!({ "tools": tools_list() }))),
        "tools/call" => Some(tools_call(conn, id, req)),
        // Notificaciones conocidas: no se responde.
        m if m.starts_with("notifications/") => None,
        // Método desconocido: error si trae `id`; si no, es una notificación y no se responde.
        _ => id.map(|id| error(id, -32601, "Method not found")),
    }
}

fn initialize(req: &Value) -> Value {
    // Devolvemos la versión de protocolo que pida el cliente si viene; si no, la nuestra.
    let pv = req["params"]["protocolVersion"]
        .as_str()
        .unwrap_or(PROTOCOL);
    json!({
        "protocolVersion": pv,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "agentboard", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Datos de uso (coste, tokens, sesiones, actividad, herramientas, modelos, proyectos) de tus agentes de código, leídos de sus logs locales. Todas las herramientas aceptan un filtro opcional: period (7d, 30d, 60d, 90d, all), agents (ids) y projects (ids)."
    })
}

fn tools_call(conn: &Connection, id: Option<Value>, req: &Value) -> Value {
    let name = req["params"]["name"].as_str().unwrap_or("");
    let args = &req["params"]["arguments"];
    match run_tool(conn, name, args) {
        // Éxito: el resultado va como texto JSON dentro de `content`.
        Ok(v) => result(
            id,
            json!({ "content": [{ "type": "text", "text": v.to_string() }] }),
        ),
        // Error de herramienta: resultado con isError, sin cerrar la conexión.
        Err(e) => result(
            id,
            json!({ "content": [{ "type": "text", "text": format!("Error: {e}") }], "isError": true }),
        ),
    }
}

/// Ejecuta una herramienta y devuelve su JSON de resultado.
fn run_tool(conn: &Connection, name: &str, args: &Value) -> Result<Value> {
    let f = || filter_from(args);
    let by = |g: &str| -> Result<Value> {
        Ok(serde_json::to_value(queries::breakdown(conn, &f()?, g)?)?)
    };
    match name {
        "get_summary" => Ok(serde_json::to_value(queries::summary(
            conn,
            &f()?,
            ingest::now_ms(),
        )?)?),
        "get_cost_by_agent" => by("agent"),
        "get_cost_by_model" => by("model"),
        "get_cost_by_project" => by("project"),
        "get_cost_by_branch" => by("branch"),
        "get_activity" => by("activity"),
        "get_tools" => by("tool"),
        "get_shell_commands" => by("command"),
        "get_skills" => by("skill"),
        "get_mcp_servers" => by("mcp"),
        "get_agent_types" => by("agent_type"),
        "get_daily" => Ok(serde_json::to_value(queries::timeseries(
            conn,
            &f()?,
            "day",
            tz_offset(),
        )?)?),
        "list_agents" => Ok(serde_json::to_value(queries::list_agents(conn, &f()?)?)?),
        "list_projects" => Ok(serde_json::to_value(queries::list_projects(conn, &f()?)?)?),
        "get_data_info" => Ok(serde_json::to_value(queries::data_info(conn)?)?),
        other => bail!("herramienta desconocida: {other}"),
    }
}

/// Construye el `Filter` a partir de los argumentos de la herramienta.
fn filter_from(args: &Value) -> Result<queries::Filter> {
    let (from, to) = match args.get("period").and_then(Value::as_str) {
        Some(p) => period_range(p)?,
        None => (None, None),
    };
    let agents = args["agents"].as_array().map(|a| {
        a.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    });
    let projects = args["projects"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect());
    Ok(queries::Filter {
        from,
        to,
        agents,
        projects,
    })
}

/// Traduce un periodo a rango `[from, to)` en epoch ms (últimos N días incluido hoy, hora local).
fn period_range(p: &str) -> Result<(Option<i64>, Option<i64>)> {
    let days = match p {
        "7d" => 7,
        "30d" => 30,
        "60d" => 60,
        "90d" => 90,
        "all" => return Ok((None, None)),
        other => bail!("periodo desconocido: {other} (usa 7d, 30d, 60d, 90d o all)"),
    };
    let now = chrono::Local::now();
    let start = now
        .with_hour(0)
        .and_then(|d| d.with_minute(0))
        .and_then(|d| d.with_second(0))
        .and_then(|d| d.with_nanosecond(0))
        .ok_or_else(|| anyhow!("no se pudo calcular el inicio del día"))?;
    Ok((Some(start.timestamp_millis() - (days - 1) * DAY_MS), None))
}

/// Minutos a sumar a UTC para la hora local (para agrupar la serie diaria por día local).
fn tz_offset() -> i64 {
    chrono::Local::now().offset().fix().local_minus_utc() as i64 / 60
}

/// Esquema del filtro común a casi todas las herramientas.
fn filter_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "period": { "type": "string", "enum": ["7d", "30d", "60d", "90d", "all"], "description": "Periodo; por defecto, todo." },
            "agents": { "type": "array", "items": { "type": "string" }, "description": "IDs de agente a incluir; por defecto, todos." },
            "projects": { "type": "array", "items": { "type": "integer" }, "description": "IDs de proyecto a incluir; por defecto, todos." }
        }
    })
}

fn tools_list() -> Vec<Value> {
    let f = |name: &str, desc: &str| json!({ "name": name, "description": desc, "inputSchema": filter_schema() });
    vec![
        f("get_summary", "Resumen del periodo: coste, llamadas, sesiones, cache hit, ahorro por caché, burn rate y modelos sin precio."),
        f("get_cost_by_agent", "Coste, llamadas y sesiones por agente (Claude Code, Codex, Gemini, OpenCode…)."),
        f("get_cost_by_model", "Coste, llamadas y cache hit por modelo."),
        f("get_cost_by_project", "Coste, llamadas y overhead de contexto por proyecto."),
        f("get_cost_by_branch", "Coste y llamadas por rama de git."),
        f("get_activity", "Reparto por tipo de actividad (coding, testing, debugging, exploración…)."),
        f("get_tools", "Uso y número de errores por herramienta nativa (Bash, Read, Edit…)."),
        f("get_shell_commands", "Comandos de shell más ejecutados."),
        f("get_skills", "Skills y subagentes invocados."),
        f("get_mcp_servers", "Servidores MCP usados y su actividad."),
        f("get_agent_types", "Tipos de subagente de Claude Code y su coste."),
        f("get_daily", "Serie diaria: coste, llamadas, sesiones y tokens por día."),
        f("list_agents", "Agentes detectados en esta máquina, con su coste y carpeta de logs."),
        f("list_projects", "Proyectos detectados, con su coste."),
        json!({ "name": "get_data_info", "description": "Rango de fechas y totales del historial cargado.", "inputSchema": { "type": "object", "properties": {} } }),
    ]
}

// --- utilidades JSON-RPC ---

fn result(id: Option<Value>, value: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": value })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn write_msg(out: &mut impl Write, msg: &Value) {
    let mut s = msg.to_string();
    s.push('\n');
    let _ = out.write_all(s.as_bytes());
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodo_se_traduce() {
        assert_eq!(period_range("all").unwrap(), (None, None));
        let (from, to) = period_range("7d").unwrap();
        assert!(from.is_some() && to.is_none());
        assert!(period_range("xx").is_err());
    }

    #[test]
    fn lista_todas_las_herramientas() {
        let tools = tools_list();
        assert!(tools.len() >= 15);
        assert!(tools.iter().any(|t| t["name"] == "get_summary"));
        assert!(tools.iter().all(|t| t["inputSchema"].is_object()));
    }

    #[test]
    fn enruta_metodos() {
        let conn = db::open_in_memory().unwrap();
        // tools/list responde con la lista
        let resp = handle(
            &conn,
            &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }),
        )
        .unwrap();
        assert!(resp["result"]["tools"].is_array());
        // método desconocido con id → error JSON-RPC
        let resp = handle(
            &conn,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "no_existe" }),
        )
        .unwrap();
        assert_eq!(resp["error"]["code"], -32601);
        // notificación → sin respuesta
        assert!(handle(
            &conn,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
        )
        .is_none());
    }

    #[test]
    fn herramienta_con_periodo_invalido_da_error_controlado() {
        let conn = db::open_in_memory().unwrap();
        let resp = tools_call(
            &conn,
            Some(json!(3)),
            &json!({ "params": { "name": "get_summary", "arguments": { "period": "año" } } }),
        );
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn herramienta_desconocida_da_error_controlado() {
        let conn = db::open_in_memory().unwrap();
        let resp = tools_call(
            &conn,
            Some(json!(4)),
            &json!({ "params": { "name": "no_existe", "arguments": {} } }),
        );
        assert_eq!(resp["result"]["isError"], true);
    }
}
