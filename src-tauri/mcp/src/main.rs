//! Servidor MCP de AgentBoard (transporte stdio, JSON-RPC 2.0).
//!
//! Escanea los logs de todos los agentes a una base SQLite en memoria al arrancar (reutilizando
//! `providers` + `ingest`) y expone por MCP las mismas consultas que muestra el dashboard. No
//! necesita que la app esté abierta ni escribe nada en disco.
//!
//! Registro (ejemplo Claude Code):
//!   claude mcp add agentboard -- /ruta/a/agentboard-mcp

use agentboard_lib::{
    db, findings, ingest, pricing, providers, queries, sessions, settings, telemetry,
};
use anyhow::{anyhow, bail, Result};
use chrono::TimeZone;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::OnceLock;

const PROTOCOL: &str = "2025-06-18";

fn main() {
    telemetry::init();
    let conn = match scan() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("agentboard-mcp: no se pudo leer el historial: {e:#}");
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
    // Mismos precios que la app: los de por defecto más los que fijó el usuario en Ajustes.
    pricing::apply_overrides(&conn, price_overrides())?;
    ingest::scan_all(&mut conn, &providers::all())?;
    Ok(conn)
}

/// Precios fijados por el usuario en Ajustes, cargados una sola vez: `get_prices` etiqueta
/// como `edited` exactamente los mismos precios que se aplicaron a la base al arrancar.
static PRICE_OVERRIDES: OnceLock<Vec<settings::PriceOverride>> = OnceLock::new();

fn price_overrides() -> &'static [settings::PriceOverride] {
    PRICE_OVERRIDES.get_or_init(|| settings::load().price_overrides)
}

/// Enruta un mensaje JSON-RPC. Devuelve `None` para notificaciones (sin `id`, sin respuesta).
fn handle(conn: &Connection, req: &Value) -> Option<Value> {
    // Sin `id` (o con id null) es una notificación: JSON-RPC 2.0 prohíbe responderla,
    // sea cual sea el método (también un `ping` enviado como notificación).
    let id = req.get("id").filter(|v| !v.is_null()).cloned()?;
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => Some(result(Some(id), initialize(req))),
        "ping" => Some(result(Some(id), json!({}))),
        "tools/list" => Some(result(Some(id), json!({ "tools": tools_list() }))),
        "tools/call" => Some(tools_call(conn, Some(id), req)),
        // Método desconocido con id: error JSON-RPC.
        _ => Some(error(id, -32601, "Method not found")),
    }
}

/// Versiones del protocolo MCP que este servidor implementa.
const SUPPORTED: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

fn initialize(req: &Value) -> Value {
    // Negociación del ciclo de vida MCP: si el cliente pide una versión soportada se
    // responde esa; si no, la nuestra más reciente (y el cliente decide si le vale).
    let pv = match req["params"]["protocolVersion"].as_str() {
        Some(v) if SUPPORTED.contains(&v) => v,
        _ => PROTOCOL,
    };
    json!({
        "protocolVersion": pv,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "agentboard", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Datos de uso (coste, tokens, sesiones, actividad, herramientas, modelos, precios, proyectos) de tus agentes de código, leídos de sus logs locales. Casi todas las herramientas aceptan un filtro opcional: period (today, 7d, 30d, 60d, 90d, all), agents (ids), projects (ids) y no_project (solo sesiones sin proyecto)."
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
        // La misma clasificación por turnos que el apartado By Activity (con 1-shot).
        "get_activity" => Ok(serde_json::to_value(agentboard_lib::insights::activity(
            conn,
            &f()?,
        )?)?),
        "get_tools" => by("tool"),
        "get_shell_commands" => by("command"),
        "get_skills" => by("skill"),
        "get_mcp_servers" => by("mcp"),
        "get_agent_types" => by("agent_type"),
        "get_daily" => Ok(serde_json::to_value(queries::timeseries(
            conn,
            &f()?,
            "day",
            &tz_name(),
        )?)?),
        "get_hourly" => Ok(serde_json::to_value(queries::timeseries(
            conn,
            &f()?,
            "hour",
            &tz_name(),
        )?)?),
        "list_agents" => Ok(serde_json::to_value(queries::list_agents(conn, &f()?)?)?),
        "list_projects" => Ok(serde_json::to_value(queries::list_projects(conn, &f()?)?)?),
        "get_prices" => Ok(serde_json::to_value(pricing::list_prices(
            conn,
            &f()?,
            price_overrides(),
        )?)?),
        "get_data_info" => Ok(serde_json::to_value(queries::data_info(conn)?)?),
        "get_insights" => Ok(serde_json::to_value(findings::compute(
            conn,
            &f()?,
            ingest::now_ms(),
            &tz_name(),
        )?)?),
        "get_projects" => Ok(serde_json::to_value(sessions::list_projects(conn, &f()?)?)?),
        "get_project_detail" => {
            let key = args["key"]
                .as_str()
                .ok_or_else(|| anyhow!("falta el argumento key (de get_projects)"))?;
            Ok(serde_json::to_value(sessions::project_detail(
                conn,
                key,
                &f()?,
                &tz_name(),
            )?)?)
        }
        "get_sessions" => {
            let limit = args["limit"].as_u64().map(|n| n as usize).unwrap_or(50);
            Ok(serde_json::to_value(sessions::list_sessions(
                conn,
                &f()?,
                Some(limit),
            )?)?)
        }
        "get_session_detail" => {
            let id = args["id"]
                .as_str()
                .ok_or_else(|| anyhow!("falta el argumento id"))?;
            if !sessions::exists(conn, id)? {
                bail!("no existe la sesión {id}");
            }
            Ok(serde_json::to_value(sessions::session_detail(conn, id)?)?)
        }
        other => bail!("herramienta desconocida: {other}"),
    }
}

/// Construye el `Filter` a partir de los argumentos de la herramienta. Los tipos se
/// validan: un filtro mal escrito devuelve error, nunca amplía la consulta en silencio.
fn filter_from(args: &Value) -> Result<queries::Filter> {
    let (from, to) = match args.get("period") {
        None | Some(Value::Null) => (None, None),
        Some(p) => period_range(
            p.as_str()
                .ok_or_else(|| anyhow!("period debe ser texto: today, 7d, 30d, 60d, 90d o all"))?,
        )?,
    };
    let agents = match args.get("agents") {
        None | Some(Value::Null) => None,
        Some(a) => Some(
            a.as_array()
                .ok_or_else(|| anyhow!("agents debe ser una lista de ids de agente (texto)"))?
                .iter()
                .map(|x| {
                    x.as_str()
                        .map(String::from)
                        .ok_or_else(|| anyhow!("cada agente debe ser un id de texto"))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
    };
    let projects = match args.get("projects") {
        None | Some(Value::Null) => None,
        Some(p) => Some(
            p.as_array()
                .ok_or_else(|| anyhow!("projects debe ser una lista de ids numéricos"))?
                .iter()
                .map(|x| {
                    x.as_i64()
                        .ok_or_else(|| anyhow!("cada proyecto debe ser un id numérico"))
                })
                .collect::<Result<Vec<_>>>()?,
        ),
    };
    let no_project = match args.get("no_project") {
        None | Some(Value::Null) => false,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| anyhow!("no_project debe ser booleano"))?,
    };
    Ok(queries::Filter {
        from,
        to,
        agents,
        projects,
        no_project,
    })
}

/// Traduce un periodo a rango `[from, to)` en epoch ms (últimos N días incluido hoy, hora local).
fn period_range(p: &str) -> Result<(Option<i64>, Option<i64>)> {
    let days = match p {
        "today" => 1,
        "7d" => 7,
        "30d" => 30,
        "60d" => 60,
        "90d" => 90,
        "all" => return Ok((None, None)),
        other => bail!("periodo desconocido: {other} (usa today, 7d, 30d, 60d, 90d o all)"),
    };
    // Días de calendario locales: restar múltiplos fijos de 24 h se desalinea al cambiar la hora.
    let start_day = chrono::Local::now().date_naive() - chrono::Days::new((days - 1) as u64);
    let start = chrono::Local
        .from_local_datetime(&start_day.and_hms_opt(0, 0, 0).expect("medianoche válida"))
        .earliest()
        .ok_or_else(|| anyhow!("no se pudo calcular el inicio del periodo"))?;
    Ok((Some(start.timestamp_millis()), None))
}

/// Zona IANA del sistema: agrupa las series por el día local histórico de cada fecha.
fn tz_name() -> String {
    agentboard_lib::tz::system_name()
}

/// Esquema del filtro común a casi todas las herramientas.
fn filter_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "period": { "type": "string", "enum": ["today", "7d", "30d", "60d", "90d", "all"], "description": "Periodo: hoy (desde las 00:00 locales), últimos N días o todo; por defecto, todo." },
            "agents": { "type": "array", "items": { "type": "string" }, "description": "IDs de agente a incluir; por defecto, todos." },
            "projects": { "type": "array", "items": { "type": "integer" }, "description": "IDs de proyecto a incluir; por defecto, todos." },
            "no_project": { "type": "boolean", "description": "true = solo las sesiones sin proyecto detectado; por defecto, false (todas)." }
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
        f("get_activity", "Coste, turnos y 1-shot por tipo de actividad (coding, testing, debugging, exploración…), clasificando cada turno como el apartado By Activity."),
        f("get_tools", "Uso y número de errores por herramienta nativa (Bash, Read, Edit…)."),
        f("get_shell_commands", "Comandos de shell más ejecutados."),
        f("get_skills", "Skills y subagentes invocados."),
        f("get_mcp_servers", "Servidores MCP usados y su actividad."),
        f("get_agent_types", "Subagentes de todos los agentes (Claude Code, Codex, Gemini, OpenCode, Cursor…) por agente y tipo, con llamadas y coste."),
        f("get_daily", "Serie diaria: coste, llamadas, sesiones y tokens por día."),
        f("get_hourly", "Serie por horas (hora local): coste, llamadas, sesiones y tokens por hora; pensada para periodos cortos como today o 7d."),
        f("get_insights", "Avisos automáticos del periodo (compactaciones, modelo caro en tareas sencillas, herramientas que fallan, picos de gasto, caída del cache hit), del más grave al menos."),
        f("list_agents", "Agentes detectados en esta máquina, con su coste y carpeta de logs."),
        f("list_projects", "Proyectos detectados, con su coste."),
        f("get_prices", "Precios por modelo (entrada, salida, lectura de caché y escritura a 5 min y 1 h, USD por millón de tokens) con su origen (default, edited, reported o missing) y su uso en el periodo: primero los usados sin precio, luego los usados y después el resto."),
        json!({ "name": "get_data_info", "description": "Rango de fechas y totales del historial cargado.", "inputSchema": { "type": "object", "properties": {} } }),
        {
            let mut schema = filter_schema();
            schema["properties"]["limit"] = json!({ "type": "integer", "minimum": 1, "maximum": 500, "description": "Máximo de sesiones (las más recientes); por defecto, 50." });
            json!({ "name": "get_sessions", "description": "Sesiones del periodo, más recientes primero: agente, proyecto, rama, modelo principal, coste, turnos, compactaciones y errores de herramientas.", "inputSchema": schema })
        },
        f("get_projects", "Proyectos del periodo (agrupados por repo), del más caro al más barato: agentes, ramas, modelo principal, sesiones, tiempo activo, turnos, compactaciones y coste."),
        {
            let mut schema = filter_schema();
            schema["properties"]["key"] = json!({ "type": "string", "description": "Clave del proyecto (campo key de get_projects; \"\" = sin proyecto)." });
            schema["required"] = json!(["key"]);
            json!({ "name": "get_project_detail", "description": "Detalle de un proyecto: serie diaria, actividades, modelos, ramas, sus sesiones y latencia p50/p95 por herramienta.", "inputSchema": schema })
        },
        json!({ "name": "get_session_detail", "description": "Detalle de una sesión: coste acumulado, compactaciones, turnos con su actividad y coste, modelos y latencia (p50/p95) por herramienta.", "inputSchema": { "type": "object", "properties": { "id": { "type": "string", "description": "Id de la sesión (de get_sessions)." } }, "required": ["id"] } }),
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

    const DAY_MS: i64 = 86_400_000;

    #[test]
    fn periodo_se_traduce() {
        assert_eq!(period_range("all").unwrap(), (None, None));
        let (from, to) = period_range("7d").unwrap();
        assert!(from.is_some() && to.is_none());
        assert!(period_range("xx").is_err());
        // «Hoy» empieza a las 00:00 locales: como mucho, 24 h antes de ahora.
        let (today, _) = period_range("today").unwrap();
        let ahora = chrono::Local::now().timestamp_millis();
        assert!(today.unwrap() <= ahora && ahora - today.unwrap() < DAY_MS);
    }

    #[test]
    fn lista_todas_las_herramientas() {
        let tools = tools_list();
        assert!(tools.len() >= 22);
        assert!(tools.iter().any(|t| t["name"] == "get_project_detail"));
        assert!(tools.iter().any(|t| t["name"] == "get_session_detail"));
        assert!(tools.iter().any(|t| t["name"] == "get_summary"));
        assert!(tools.iter().any(|t| t["name"] == "get_hourly"));
        assert!(tools.iter().any(|t| t["name"] == "get_prices"));
        assert!(tools.iter().all(|t| t["inputSchema"].is_object()));
        // El filtro común anuncia el campo no_project.
        let resumen = tools.iter().find(|t| t["name"] == "get_summary").unwrap();
        assert!(resumen["inputSchema"]["properties"]["no_project"].is_object());
    }

    /// Base en memoria con dos sesiones (una sin proyecto) y una llamada por sesión,
    /// separadas tres horas dentro del mismo día local.
    fn base_sembrada() -> Connection {
        let conn = db::open_in_memory().unwrap();
        let ts1 = chrono::Utc
            .with_ymd_and_hms(2026, 1, 15, 10, 0, 0)
            .unwrap()
            .timestamp_millis();
        let ts2 = ts1 + 3 * 3_600_000;
        conn.execute_batch(&format!(
            "INSERT INTO agents VALUES ('claude-code', 'Claude Code', '/', 0);
             INSERT INTO projects (id, name, cwd, repo_root) VALUES (1, 'demo', '/demo', '/demo');
             INSERT INTO sessions (id, agent_id, project_id, started_at, ended_at) VALUES
               ('s1', 'claude-code', 1, {ts1}, {ts1}),
               ('s2', 'claude-code', NULL, {ts2}, {ts2});
             INSERT INTO calls (message_id, session_id, ts, model, input_tokens) VALUES
               ('m1', 's1', {ts1}, 'claude-sonnet-4-5', 1000000),
               ('m2', 's2', {ts2}, 'modelo-fantasma', 1000000);"
        ))
        .unwrap();
        conn
    }

    /// Llama a una herramienta y devuelve su JSON (falla el test si dio error).
    fn llama(conn: &Connection, nombre: &str, args: Value) -> Value {
        let r = tools_call(
            conn,
            Some(json!(1)),
            &json!({ "params": { "name": nombre, "arguments": args } }),
        );
        assert_ne!(r["result"]["isError"], true, "{nombre} falló: {r}");
        serde_json::from_str(r["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn filtro_no_project_limita_a_sesiones_sin_proyecto() {
        // Tipado estricto: si no es booleano, error; nunca filtro ampliado.
        assert!(filter_from(&json!({ "no_project": "sí" })).is_err());
        assert!(filter_from(&json!({ "no_project": 1 })).is_err());
        assert!(!filter_from(&json!({})).unwrap().no_project);
        assert!(
            filter_from(&json!({ "no_project": true }))
                .unwrap()
                .no_project
        );

        let conn = base_sembrada();
        assert_eq!(llama(&conn, "get_summary", json!({}))["calls"], 2);
        let sin = llama(&conn, "get_summary", json!({ "no_project": true }));
        assert_eq!(sin["calls"], 1);
        assert_eq!(sin["sessions"], 1);
        // Con false, igual que sin el campo.
        let con = llama(&conn, "get_summary", json!({ "no_project": false }));
        assert_eq!(con["calls"], 2);
        // Y el tipo incorrecto llega al cliente como isError, con la conexión viva.
        let r = tools_call(
            &conn,
            Some(json!(2)),
            &json!({ "params": { "name": "get_summary", "arguments": { "no_project": "x" } } }),
        );
        assert_eq!(r["result"]["isError"], true);
    }

    #[test]
    fn get_hourly_agrupa_por_horas_y_get_daily_por_dias() {
        let conn = base_sembrada();
        // Dos llamadas separadas tres horas: dos cubos horarios, con una llamada cada uno.
        let horas = llama(&conn, "get_hourly", json!({}));
        let horas = horas.as_array().unwrap();
        assert_eq!(horas.len(), 2);
        assert!(horas.iter().all(|p| p["calls"] == 1));
        assert!(horas[0]["ts"].as_i64().unwrap() < horas[1]["ts"].as_i64().unwrap());
        // A mediodía UTC caen en el mismo día local: get_daily sigue dando un solo cubo.
        let dias = llama(&conn, "get_daily", json!({}));
        assert_eq!(dias.as_array().unwrap().len(), 1);
        assert_eq!(dias[0]["calls"], 2);
        // El filtro común también aplica a la serie horaria.
        let sin = llama(&conn, "get_hourly", json!({ "no_project": true }));
        assert_eq!(sin.as_array().unwrap().len(), 1);
    }

    #[test]
    fn get_prices_da_precio_origen_y_uso() {
        // Sin precios del usuario: el test no depende del settings.json de la máquina.
        let _ = PRICE_OVERRIDES.set(Vec::new());
        let conn = base_sembrada();
        let precios = llama(&conn, "get_prices", json!({}));
        let filas = precios.as_array().unwrap();
        // Primero los modelos usados sin precio, con su uso del periodo.
        assert_eq!(filas[0]["model"], "modelo-fantasma");
        assert_eq!(filas[0]["source"], "missing");
        assert_eq!(filas[0]["calls"], 1);
        assert!(filas[0]["prices"].is_null());
        // Un modelo usado con precio por defecto: sus cinco precios y su coste.
        let sonnet = filas
            .iter()
            .find(|r| r["model"] == "claude-sonnet-4-5")
            .unwrap();
        assert_eq!(sonnet["source"], "default");
        assert_eq!(sonnet["calls"], 1);
        assert_eq!(sonnet["prices"].as_array().unwrap().len(), 5);
        assert!(sonnet["costUsd"].as_f64().unwrap() > 0.0);
        // Respeta el filtro común: sin proyecto, el modelo con precio queda sin uso.
        let sin = llama(&conn, "get_prices", json!({ "no_project": true }));
        let sonnet = sin
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["model"] == "claude-sonnet-4-5")
            .unwrap();
        assert_eq!(sonnet["calls"], 0);
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
    fn negocia_version_y_no_responde_notificaciones() {
        let conn = db::open_in_memory().unwrap();
        let init = |v: Value| {
            handle(
                &conn,
                &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                         "params": { "protocolVersion": v } }),
            )
            .unwrap()
        };
        // Versión desconocida o mal tipada: se ofrece la nuestra, no se repite la suya.
        assert_eq!(
            init(json!("2099-01-01"))["result"]["protocolVersion"],
            PROTOCOL
        );
        assert_eq!(init(json!(42))["result"]["protocolVersion"], PROTOCOL);
        // Versión soportada: se confirma esa misma.
        assert_eq!(
            init(json!("2025-03-26"))["result"]["protocolVersion"],
            "2025-03-26"
        );
        // `ping` como petición responde; como notificación (sin id o id null), no.
        assert!(handle(
            &conn,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" })
        )
        .is_some());
        assert!(handle(&conn, &json!({ "jsonrpc": "2.0", "method": "ping" })).is_none());
        assert!(handle(
            &conn,
            &json!({ "jsonrpc": "2.0", "id": Value::Null, "method": "ping" })
        )
        .is_none());
        // Método desconocido sin id: notificación desconocida, tampoco se responde.
        assert!(handle(&conn, &json!({ "jsonrpc": "2.0", "method": "no_existe" })).is_none());
    }

    #[test]
    fn filtro_mal_tipado_es_error_no_consulta_ampliada() {
        // Tipos incorrectos: error de herramienta, nunca «sin filtro».
        assert!(filter_from(&json!({ "agents": "codex" })).is_err());
        assert!(filter_from(&json!({ "agents": ["codex", 3] })).is_err());
        assert!(filter_from(&json!({ "period": 123 })).is_err());
        assert!(filter_from(&json!({ "projects": ["uno"] })).is_err());
        // Bien tipado o ausente: funciona igual que antes.
        let f = filter_from(&json!({ "agents": ["codex"], "projects": [1, 2] })).unwrap();
        assert_eq!(f.agents.as_deref(), Some(&["codex".to_string()][..]));
        assert_eq!(f.projects.as_deref(), Some(&[1, 2][..]));
        assert!(filter_from(&Value::Null).unwrap().agents.is_none());
        // Y el error llega al cliente como isError, con la conexión viva.
        let conn = db::open_in_memory().unwrap();
        let r = tools_call(
            &conn,
            Some(json!(9)),
            &json!({ "params": { "name": "get_summary", "arguments": { "agents": "codex" } } }),
        );
        assert_eq!(r["result"]["isError"], true);
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
    fn detalle_de_sesion_inexistente_da_error_controlado() {
        let conn = db::open_in_memory().unwrap();
        let r = tools_call(
            &conn,
            Some(json!(1)),
            &json!({ "params": { "name": "get_session_detail", "arguments": { "id": "nada" } } }),
        );
        assert_eq!(r["result"]["isError"], true);
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
