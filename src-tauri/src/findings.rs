//! «Lo que deberías saber»: avisos automáticos sobre el filtro activo. Cada aviso trae un tipo y
//! sus parámetros (para que la interfaz lo componga en su idioma) y un texto en español (MCP).

use crate::insights::{classify_turn, shell_commands, turn_stats};
use crate::queries::{self, Filter};
use crate::sessions;
use anyhow::Result;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Modelo de referencia para estimar el ahorro de los turnos sencillos.
pub const CHEAPER_MODEL: &str = "claude-sonnet-5";
const SHELL_TOOLS: &[&str] = &["Bash", "shell", "exec_command", "run_shell_command"];

#[derive(Debug, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// `compactions`, `expensive_model`, `tool_errors`, `spend_spike` o `cache_drop`.
    pub kind: String,
    /// `critical`, `warn`, `info` o `good` (lo que va bien).
    pub severity: String,
    pub params: Map<String, Value>,
    /// Texto en español, para quien no compone el suyo (servidor MCP).
    pub message: String,
}

fn finding(kind: &str, severity: &str, params: Value, message: String) -> Finding {
    Finding {
        kind: kind.into(),
        severity: severity.into(),
        params: params.as_object().cloned().unwrap_or_default(),
        message,
    }
}

fn rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "warn" => 1,
        "info" => 2,
        _ => 3,
    }
}

/// Precio de entrada vigente (USD/M) de un modelo, si tiene.
fn input_price(conn: &Connection, model: &str) -> Result<Option<f64>> {
    Ok(conn
        .query_row(
            "SELECT input FROM prices WHERE model = ?1 ORDER BY valid_from DESC LIMIT 1",
            params![model],
            |r| r.get(0),
        )
        .optional()?)
}

/// Todos los avisos del filtro, del más grave al menos. `now` en epoch ms y `tz` como en `queries`.
pub fn compute(conn: &Connection, f: &Filter, now: i64, tz: &str) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    out.extend(compactions(conn, f)?);
    out.extend(expensive_model(conn, f)?);
    out.extend(tool_errors(conn, f)?);
    out.extend(spend_spike(conn, f, tz)?);
    out.extend(cache_drop(conn, f, now)?);
    // Avisos informativos y positivos: cuentan cómo es el uso aunque no haya problemas.
    let s = queries::summary(conn, f, now)?;
    out.extend(unpriced(conn, f, &s)?);
    out.extend(concentration(conn, f, &s)?);
    out.extend(top_session(conn, f, &s)?);
    out.extend(shell_command_errors(conn, f)?);
    out.extend(mcp_errors(conn, f)?);
    out.extend(low_cache_models(conn, f)?);
    out.extend(one_shot(conn, f)?);
    out.extend(subagent_share(conn, f, &s)?);
    out.extend(pace(conn, f, &s, tz)?);
    out.extend(after_hours(conn, f, &s, tz)?);
    out.extend(cost_per_session(conn, f, &s, now)?);
    out.extend(unused_agents(conn, f)?);
    out.extend(cache_savings(&s));
    out.sort_by_key(|x| rank(&x.severity));
    Ok(out)
}

fn compactions(conn: &Connection, f: &Filter) -> Result<Option<Finding>> {
    // Agregado directo sobre todo el filtro: el listado de sesiones se queda en las 500 más
    // recientes y ocultaría sesiones antiguas con muchas compactaciones.
    let (w, args) = f.sql("e.ts");
    let cost_bounds = sessions::time_bounds(f, "cc.ts");
    let sql = format!(
        "SELECT e.session_id, COUNT(*) AS n, COALESCE(p.name, '(sin proyecto)'),
                COALESCE(s.git_branch, ''),
                (SELECT COALESCE(SUM(cc.cost_usd), 0) FROM call_costs cc
                  WHERE cc.session_id = e.session_id{cost_bounds}) AS cost
         FROM events e JOIN sessions s ON s.id = e.session_id
         LEFT JOIN projects p ON p.id = s.project_id
         WHERE e.kind = 'compaction' AND {w}
         GROUP BY 1 HAVING n >= 3
         ORDER BY n DESC, cost DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let heavy: Vec<(String, i64, String, String)> = stmt
        .query_map(params_from_iter(args.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let Some((session_id, max, project, branch)) = heavy.first().cloned() else {
        return Ok(None);
    };
    let place = if branch.is_empty() {
        project.clone()
    } else {
        format!("{project} · {branch}")
    };
    Ok(Some(finding(
        "compactions",
        "critical",
        json!({ "n": heavy.len(), "project": project, "branch": branch, "max": max, "sessionId": session_id }),
        format!(
            "{} sesiones con 3+ compactaciones de contexto; la peor ({max} compactaciones) en {place}. Divide la tarea o delega en subagentes.",
            heavy.len(),
        ),
    )))
}

fn expensive_model(conn: &Connection, f: &Filter) -> Result<Option<Finding>> {
    let Some(cheap) = input_price(conn, CHEAPER_MODEL)? else {
        return Ok(None);
    };
    let mut by_model: HashMap<String, f64> = HashMap::new();
    let mut simple_total = 0.0;
    for t in turn_stats(conn, f)?.values() {
        if !matches!(classify_turn(t), "exploration" | "conversation") {
            continue;
        }
        simple_total += t.cost_usd;
        if let Some(m) = t.dominant_model() {
            if m.contains("opus") || m.contains("fable") {
                *by_model.entry(m.to_string()).or_default() += t.cost_usd;
            }
        }
    }
    let mut cost = 0.0;
    let mut saving = 0.0;
    let mut top: Option<(String, f64)> = None;
    for (model, c) in &by_model {
        let Some(p) = input_price(conn, model)? else {
            continue;
        };
        if p <= cheap {
            continue;
        }
        cost += c;
        saving += c * (1.0 - cheap / p);
        if top.as_ref().is_none_or(|(_, tc)| c > tc) {
            top = Some((model.clone(), *c));
        }
    }
    let Some((model, _)) = top.filter(|_| saving > 1.0) else {
        return Ok(None);
    };
    let share = if simple_total > 0.0 {
        cost / simple_total
    } else {
        0.0
    };
    Ok(Some(finding(
        "expensive_model",
        "warn",
        json!({ "model": model, "cost": cost, "saving": saving, "share": share, "cheaper": CHEAPER_MODEL }),
        format!(
            "El {:.0}% del gasto en exploración y conversación (${cost:.2}) va a {model}. Con {CHEAPER_MODEL} habría costado ≈ ${saving:.2} menos.",
            share * 100.0
        ),
    )))
}

fn tool_errors(conn: &Connection, f: &Filter) -> Result<Vec<Finding>> {
    let mut failing: Vec<_> = queries::breakdown(conn, f, "tool")?
        .into_iter()
        .filter(|r| r.calls >= 20 && r.errors as f64 / r.calls as f64 > 0.10)
        .collect();
    failing.sort_by(|a, b| (b.errors * a.calls).cmp(&(a.errors * b.calls)));
    let mut commands = None;
    let mut out = Vec::new();
    for r in failing.into_iter().take(3) {
        let rate = r.errors as f64 / r.calls as f64;
        let mut p = json!({ "tool": r.key, "rate": rate, "errors": r.errors, "calls": r.calls });
        let mut message = format!(
            "{} falla un {:.0}% ({} de {} usos).",
            r.key,
            rate * 100.0,
            r.errors,
            r.calls
        );
        if SHELL_TOOLS.contains(&r.key.as_str()) {
            if commands.is_none() {
                commands = Some(shell_commands(conn, f)?);
            }
            if let Some(c) = commands
                .as_ref()
                .and_then(|c| c.iter().filter(|c| c.errors > 0).max_by_key(|c| c.errors))
            {
                p["command"] = json!(c.key);
                p["commandErrors"] = json!(c.errors);
                message.push_str(&format!(" {} concentra {} errores.", c.key, c.errors));
            }
        }
        out.push(finding("tool_errors", "warn", p, message));
    }
    Ok(out)
}

fn spend_spike(conn: &Connection, f: &Filter, tz: &str) -> Result<Option<Finding>> {
    let days: Vec<_> = queries::timeseries(conn, f, "day", tz)?
        .into_iter()
        .filter(|p| p.cost_usd > 0.0)
        .collect();
    if days.len() < 5 {
        return Ok(None);
    }
    let n = days.len() as f64;
    let mean = days.iter().map(|p| p.cost_usd).sum::<f64>() / n;
    let sd = (days
        .iter()
        .map(|p| (p.cost_usd - mean).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    let max = days
        .iter()
        .max_by(|a, b| a.cost_usd.total_cmp(&b.cost_usd))
        .expect("hay días");
    if max.cost_usd <= 2.0 * mean || max.cost_usd <= mean + 2.0 * sd {
        return Ok(None);
    }
    let times = max.cost_usd / mean;
    Ok(Some(finding(
        "spend_spike",
        "info",
        json!({ "ts": max.ts, "cost": max.cost_usd, "times": times, "mean": mean }),
        format!(
            "Pico de gasto: ${:.2} en un día, {times:.1}× la media diaria (${mean:.2}).",
            max.cost_usd
        ),
    )))
}

fn cache_drop(conn: &Connection, f: &Filter, now: i64) -> Result<Option<Finding>> {
    let Some(from) = f.from else { return Ok(None) };
    let span = f.to.unwrap_or(now) - from;
    if span <= 0 {
        return Ok(None);
    }
    let prev = Filter {
        from: Some(from - span),
        to: Some(from),
        ..f.clone()
    };
    let cur = queries::summary(conn, f, now)?;
    let before = queries::summary(conn, &prev, now)?;
    if cur.calls == 0 || before.calls == 0 {
        return Ok(None);
    }
    let drop = before.cache_hit - cur.cache_hit;
    if drop <= 0.03 {
        return Ok(None);
    }
    Ok(Some(finding(
        "cache_drop",
        "warn",
        json!({ "before": before.cache_hit, "now": cur.cache_hit, "pp": drop * 100.0 }),
        format!(
            "El cache hit ha bajado {:.1} puntos frente al periodo anterior ({:.1}% → {:.1}%).",
            drop * 100.0,
            before.cache_hit * 100.0,
            cur.cache_hit * 100.0
        ),
    )))
}

// ---------------------------------------------------------------------------
// Avisos informativos y positivos

fn unpriced(conn: &Connection, f: &Filter, s: &queries::Summary) -> Result<Option<Finding>> {
    if s.unpriced_models.is_empty() {
        return Ok(None);
    }
    let calls: i64 = queries::breakdown(conn, f, "model")?
        .iter()
        .filter(|r| s.unpriced_models.contains(&r.key))
        .map(|r| r.calls)
        .sum();
    let n = s.unpriced_models.len();
    Ok(Some(finding(
        "unpriced_models",
        "warn",
        json!({ "n": n, "models": s.unpriced_models.join(", "), "calls": calls }),
        if n == 1 {
            format!(
                "1 modelo sin precio ({}): {calls} llamadas cuentan como $0.",
                s.unpriced_models[0]
            )
        } else {
            format!(
                "{n} modelos sin precio ({}): {calls} llamadas cuentan como $0.",
                s.unpriced_models.join(", ")
            )
        },
    )))
}

fn concentration(conn: &Connection, f: &Filter, s: &queries::Summary) -> Result<Vec<Finding>> {
    let mut out = Vec::new();
    if s.cost_usd <= 0.0 {
        return Ok(out);
    }
    let projects = sessions::list_projects(conn, f)?;
    if projects.len() > 1 {
        if let Some(p) = projects.first().filter(|p| p.cost_usd / s.cost_usd >= 0.5) {
            let share = p.cost_usd / s.cost_usd;
            out.push(finding(
                "project_share",
                "info",
                json!({ "project": p.name, "key": p.key, "share": share, "cost": p.cost_usd, "n": projects.len() }),
                format!("{} concentra el {:.0}% del gasto (${:.2}) de {} proyectos.", p.name, share * 100.0, p.cost_usd, projects.len()),
            ));
        }
    }
    let models = queries::breakdown(conn, f, "model")?;
    if models.len() > 1 {
        if let Some(m) = models.first().filter(|m| m.cost_usd / s.cost_usd >= 0.7) {
            let share = m.cost_usd / s.cost_usd;
            out.push(finding(
                "model_share",
                "info",
                json!({ "model": m.key, "share": share, "cost": m.cost_usd }),
                format!(
                    "El {:.0}% del gasto (${:.2}) va a {}.",
                    share * 100.0,
                    m.cost_usd,
                    m.key
                ),
            ));
        }
    }
    Ok(out)
}

fn top_session(conn: &Connection, f: &Filter, s: &queries::Summary) -> Result<Option<Finding>> {
    // Agregado directo (sin el recorte a 500 del listado): la sesión más cara de todo el filtro.
    let (w, args) = f.sql("c.ts");
    let top: Option<(String, f64, String, String, i64, i64)> = conn
        .query_row(
            &format!(
                "SELECT c.session_id, SUM(c.cost_usd) AS cost, COALESCE(p.name, '(sin proyecto)'),
                        COALESCE(s.git_branch, ''), MIN(c.ts), MAX(c.ts)
                 FROM call_costs c JOIN sessions s ON s.id = c.session_id
                 LEFT JOIN projects p ON p.id = s.project_id
                 WHERE {w} GROUP BY 1 ORDER BY cost DESC LIMIT 1"
            ),
            params_from_iter(args.iter()),
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((session_id, cost, project, branch, first, last)) = top else {
        return Ok(None);
    };
    let total: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(DISTINCT c.session_id) FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"
        ),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let share = if s.cost_usd > 0.0 {
        cost / s.cost_usd
    } else {
        0.0
    };
    if total < 2 || (share < 0.2 && cost <= 5.0) || cost <= 0.0 {
        return Ok(None);
    }
    let tb = sessions::time_bounds(f, "ts");
    let model: Option<String> = conn
        .query_row(
            &format!(
                "SELECT model FROM call_costs WHERE session_id = ?1{tb}
                 GROUP BY 1 ORDER BY SUM(cost_usd) DESC, COUNT(*) DESC LIMIT 1"
            ),
            [&session_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(Some(finding(
        "top_session",
        "info",
        json!({
            "sessionId": session_id, "cost": cost, "share": share, "project": project,
            "branch": branch, "model": model.unwrap_or_default(),
            "durationMs": last - first,
        }),
        format!(
            "La sesión más cara: ${cost:.2} ({:.0}% del gasto) en {project}.",
            share * 100.0
        ),
    )))
}

fn shell_command_errors(conn: &Connection, f: &Filter) -> Result<Option<Finding>> {
    // En una línea compuesta no se sabe qué comando falló: los auxiliares (echo, cd, grep…)
    // heredan el error de los demás, así que no se señalan.
    const HELPERS: &[&str] = &[
        "echo", "printf", "cd", "ls", "cat", "head", "tail", "grep", "rg", "wc", "sort", "uniq",
        "true", "false", "sleep", "tr", "cut", "awk", "sed", "xargs", "tee", "test", "[", "pwd",
        "which", "export", "set",
    ];
    let rows = shell_commands(conn, f)?;
    let Some(c) = rows
        .iter()
        .filter(|c| !HELPERS.contains(&c.key.as_str()))
        .filter(|c| c.errors >= 5 && c.errors as f64 / c.calls as f64 >= 0.05)
        .max_by_key(|c| c.errors)
    else {
        return Ok(None);
    };
    let rate = c.errors as f64 / c.calls as f64;
    Ok(Some(finding(
        "command_errors",
        "info",
        json!({ "command": c.key, "errors": c.errors, "calls": c.calls, "rate": rate }),
        format!(
            "{} es el comando que más falla: {} de {} ({:.0}%).",
            c.key,
            c.errors,
            c.calls,
            rate * 100.0
        ),
    )))
}

fn mcp_errors(conn: &Connection, f: &Filter) -> Result<Vec<Finding>> {
    let mut rows: Vec<_> = crate::insights::mcp_servers(conn, f)?
        .into_iter()
        .filter(|r| r.calls >= 10 && r.errors as f64 / r.calls as f64 > 0.10)
        .collect();
    rows.sort_by(|a, b| (b.errors * a.calls).cmp(&(a.errors * b.calls)));
    Ok(rows
        .into_iter()
        .take(2)
        .map(|r| {
            let rate = r.errors as f64 / r.calls as f64;
            finding(
                "mcp_errors",
                "warn",
                json!({ "server": r.key, "errors": r.errors, "calls": r.calls, "rate": rate }),
                format!(
                    "El servidor MCP {} falla un {:.0}% ({} de {} usos).",
                    r.key,
                    rate * 100.0,
                    r.errors,
                    r.calls
                ),
            )
        })
        .collect())
}

fn low_cache_models(conn: &Connection, f: &Filter) -> Result<Option<Finding>> {
    let worst = queries::breakdown(conn, f, "model")?
        .into_iter()
        .filter(|m| m.calls >= 100 && m.cache_hit < 0.7)
        .min_by(|a, b| a.cache_hit.total_cmp(&b.cache_hit));
    Ok(worst.map(|m| {
        finding(
            "low_cache",
            "warn",
            json!({ "model": m.key, "cacheHit": m.cache_hit, "calls": m.calls }),
            format!(
                "{} solo aprovecha la caché en un {:.0}% de la entrada ({} llamadas).",
                m.key,
                m.cache_hit * 100.0,
                m.calls
            ),
        )
    }))
}

fn one_shot(conn: &Connection, f: &Filter) -> Result<Option<Finding>> {
    let report = crate::insights::activity(conn, f)?;
    let edits: i64 = report.activities.iter().map(|a| a.edit_turns).sum();
    if edits < 20 {
        return Ok(None);
    }
    let ok: f64 = report
        .activities
        .iter()
        .map(|a| a.one_shot.unwrap_or(0.0) * a.edit_turns as f64)
        .sum();
    let rate = ok / edits as f64;
    let p = json!({ "rate": rate, "turns": edits });
    Ok(if rate < 0.8 {
        Some(finding(
            "one_shot_low",
            "warn",
            p,
            format!(
                "Solo el {:.0}% de los {edits} turnos con ediciones sale a la primera.",
                rate * 100.0
            ),
        ))
    } else if rate >= 0.95 {
        Some(finding(
            "one_shot_good",
            "good",
            p,
            format!(
                "El {:.0}% de los {edits} turnos con ediciones sale a la primera.",
                rate * 100.0
            ),
        ))
    } else {
        None
    })
}

fn subagent_share(conn: &Connection, f: &Filter, s: &queries::Summary) -> Result<Option<Finding>> {
    if s.cost_usd <= 0.0 {
        return Ok(None);
    }
    let rows = crate::insights::agent_types(conn, f)?;
    let cost: f64 = rows.iter().map(|r| r.cost_usd).sum();
    let share = cost / s.cost_usd;
    if share < 0.2 {
        return Ok(None);
    }
    let top = rows.first().map(|r| r.label.clone()).unwrap_or_default();
    Ok(Some(finding(
        "subagent_share",
        "info",
        json!({ "share": share, "cost": cost, "top": top }),
        format!(
            "Los subagentes suman el {:.0}% del gasto (${cost:.2}); el que más, {top}.",
            share * 100.0
        ),
    )))
}

fn pace(conn: &Connection, f: &Filter, s: &queries::Summary, tz: &str) -> Result<Option<Finding>> {
    let hours: Vec<_> = queries::timeseries(conn, f, "hour", tz)?
        .into_iter()
        .filter(|p| p.cost_usd > 0.0)
        .collect();
    if hours.len() < 3 {
        return Ok(None);
    }
    let avg = hours.iter().map(|p| p.cost_usd).sum::<f64>() / hours.len() as f64;
    let now = s.burn_rate_usd_h;
    if now <= 1.0 || now <= 2.0 * avg {
        return Ok(None);
    }
    Ok(Some(finding(
        "pace",
        "info",
        json!({ "now": now, "avg": avg, "times": now / avg }),
        format!(
            "Ahora mismo gastas ${now:.2}/h, {:.1}× tu media por hora activa (${avg:.2}).",
            now / avg
        ),
    )))
}

fn after_hours(
    conn: &Connection,
    f: &Filter,
    s: &queries::Summary,
    tz: &str,
) -> Result<Option<Finding>> {
    use chrono::{TimeZone, Timelike};
    if s.cost_usd <= 0.0 {
        return Ok(None);
    }
    let zone = crate::tz::parse(tz);
    let hour_of = |ts: i64| zone.timestamp_millis_opt(ts).unwrap().hour();
    let late: f64 = queries::timeseries(conn, f, "hour", tz)?
        .iter()
        .filter(|p| !(9..19).contains(&hour_of(p.ts)))
        .map(|p| p.cost_usd)
        .sum();
    let share = late / s.cost_usd;
    if share < 0.3 {
        return Ok(None);
    }
    Ok(Some(finding(
        "after_hours",
        "info",
        json!({ "share": share, "cost": late }),
        format!(
            "El {:.0}% del gasto (${late:.2}) es antes de las 9 o desde las 19 h.",
            share * 100.0
        ),
    )))
}

fn cost_per_session(
    conn: &Connection,
    f: &Filter,
    cur: &queries::Summary,
    now: i64,
) -> Result<Option<Finding>> {
    let Some(from) = f.from else { return Ok(None) };
    let span = f.to.unwrap_or(now) - from;
    let prev = Filter {
        from: Some(from - span),
        to: Some(from),
        ..f.clone()
    };
    let before = queries::summary(conn, &prev, now)?;
    if cur.sessions < 3 || before.sessions < 3 || before.cost_usd <= 0.0 {
        return Ok(None);
    }
    let (a, b) = (
        before.cost_usd / before.sessions as f64,
        cur.cost_usd / cur.sessions as f64,
    );
    let change = b / a - 1.0;
    if change <= 0.3 {
        return Ok(None);
    }
    Ok(Some(finding(
        "session_cost_up",
        "warn",
        json!({ "before": a, "now": b, "change": change }),
        format!("Cada sesión cuesta de media ${b:.2}, un {:.0}% más que en el periodo anterior (${a:.2}).", change * 100.0),
    )))
}

fn unused_agents(conn: &Connection, f: &Filter) -> Result<Option<Finding>> {
    let idle: Vec<String> = queries::list_agents(conn, f)?
        .into_iter()
        .filter(|a| a.calls == 0)
        .filter(|a| f.agents.as_ref().is_none_or(|ids| ids.contains(&a.id)))
        .map(|a| a.name)
        .collect();
    if idle.is_empty() {
        return Ok(None);
    }
    Ok(Some(finding(
        "unused_agents",
        "info",
        json!({ "n": idle.len(), "agents": idle.join(", ") }),
        if idle.len() == 1 {
            format!("{} está instalado pero sin uso en el periodo.", idle[0])
        } else {
            format!(
                "{} agentes instalados sin uso en el periodo: {}.",
                idle.len(),
                idle.join(", ")
            )
        },
    )))
}

fn cache_savings(s: &queries::Summary) -> Option<Finding> {
    (s.cost_usd > 0.0 && s.cache_savings_usd > s.cost_usd).then(|| {
        let times = s.cache_savings_usd / s.cost_usd;
        finding(
            "cache_savings",
            "good",
            json!({ "saving": s.cache_savings_usd, "cost": s.cost_usd, "times": times }),
            format!(
                "La caché te ahorró ${:.2}, {times:.1}× lo que gastaste (${:.2}).",
                s.cache_savings_usd, s.cost_usd
            ),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::queries::testdata;

    const DAY: i64 = 86_400_000;

    fn conn() -> Connection {
        let c = db::open_in_memory().unwrap();
        c.execute_batch(
            "INSERT INTO agents VALUES ('claude-code','Claude Code','/c',0);
             INSERT INTO projects (id,name,cwd,repo_root) VALUES (1,'web','/w','/w');",
        )
        .unwrap();
        c
    }

    fn session(c: &Connection, id: &str, branch: &str) {
        c.execute(
            "INSERT INTO sessions (id,agent_id,project_id,git_branch,started_at,ended_at) VALUES (?1,'claude-code',1,?2,0,0)",
            params![id, branch],
        )
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)] // helper de test: una columna por argumento
    fn call(
        c: &Connection,
        id: &str,
        s: &str,
        ts: i64,
        model: &str,
        input: i64,
        cache_read: i64,
        turn: Option<&str>,
    ) {
        c.execute(
            "INSERT INTO calls (message_id,session_id,ts,model,input_tokens,cache_read,turn_id) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id, s, ts, model, input, cache_read, turn],
        )
        .unwrap();
        crate::derived::refresh(c).unwrap();
    }

    #[test]
    fn sin_avisos() {
        // Sin actividad no hay nada que contar.
        let c = db::open_in_memory().unwrap();
        assert!(compute(&c, &Filter::default(), 10_000, "UTC")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn compactaciones() {
        let c = conn();
        session(&c, "a", "feat/x");
        session(&c, "b", "main");
        call(&c, "m1", "a", 1000, "claude-sonnet-5", 10, 0, None);
        call(&c, "m2", "b", 2000, "claude-sonnet-5", 10, 0, None);
        for (s, n) in [("a", 4), ("b", 3)] {
            for i in 0..n {
                c.execute(
                    "INSERT INTO events (session_id, ts, kind) VALUES (?1, ?2, 'compaction')",
                    params![s, i],
                )
                .unwrap();
            }
        }
        let f = compactions(&c, &Filter::default()).unwrap().unwrap();
        assert_eq!(
            (
                f.severity.as_str(),
                &f.params["n"],
                &f.params["branch"],
                &f.params["max"]
            ),
            ("critical", &json!(2), &json!("feat/x"), &json!(4))
        );
    }

    #[test]
    fn modelo_caro_en_exploracion() {
        let c = conn();
        session(&c, "a", "main");
        c.execute_batch(
            "INSERT INTO turns (id, session_id, ts) VALUES ('t1','a',1000);
             INSERT INTO tool_calls (call_id,session_id,ts,tool,turn_id) VALUES ('u1','a',1000,'Read','t1');",
        )
        .unwrap();
        // 2M de entrada en claude-opus-4-5 (5 USD/M) = 10 USD de exploración.
        call(
            &c,
            "m1",
            "a",
            1000,
            "claude-opus-4-5",
            2_000_000,
            0,
            Some("t1"),
        );
        let f = expensive_model(&c, &Filter::default()).unwrap().unwrap();
        let saving = f.params["saving"].as_f64().unwrap();
        // claude-sonnet-5 = 2 USD/M → ahorro 10 × (1 − 2/5) = 6
        assert!((saving - 6.0).abs() < 1e-9, "{saving}");
        assert_eq!(f.params["model"], "claude-opus-4-5");
    }

    #[test]
    fn herramienta_que_falla_con_su_comando() {
        let c = conn();
        session(&c, "a", "main");
        for i in 0..100 {
            let (target, err) = if i < 9 {
                ("npm test", 1)
            } else if i < 14 {
                ("cargo build", 1)
            } else {
                ("ls", 0)
            };
            c.execute(
                "INSERT INTO tool_calls (call_id,session_id,ts,tool,target,is_error) VALUES (?1,'a',?2,'Bash',?3,?4)",
                params![format!("u{i}"), i, target, err],
            )
            .unwrap();
        }
        crate::derived::refresh(&c).unwrap();
        let v = tool_errors(&c, &Filter::default()).unwrap();
        assert_eq!(v.len(), 1);
        assert!((v[0].params["rate"].as_f64().unwrap() - 0.14).abs() < 1e-9);
        assert_eq!(
            (&v[0].params["command"], &v[0].params["commandErrors"]),
            (&json!("npm"), &json!(9))
        );
    }

    #[test]
    fn pico_de_gasto() {
        let c = conn();
        session(&c, "a", "main");
        // 10 días de 10 USD (sonnet-5 a 2 USD/M → 5M) y uno de 45 USD.
        for d in 0..10 {
            call(
                &c,
                &format!("m{d}"),
                "a",
                d * DAY + 1000,
                "claude-sonnet-5",
                5_000_000,
                0,
                None,
            );
        }
        call(
            &c,
            "pico",
            "a",
            10 * DAY + 1000,
            "claude-sonnet-5",
            22_500_000,
            0,
            None,
        );
        let f = spend_spike(&c, &Filter::default(), "UTC").unwrap().unwrap();
        assert!((f.params["cost"].as_f64().unwrap() - 45.0).abs() < 1e-9);
        assert_eq!(f.params["ts"], json!(10 * DAY));
        // Sin el pico, no hay aviso.
        c.execute("DELETE FROM calls WHERE message_id = 'pico'", [])
            .unwrap();
        crate::derived::refresh(&c).unwrap();
        assert!(spend_spike(&c, &Filter::default(), "UTC")
            .unwrap()
            .is_none());
    }

    #[test]
    fn caida_del_cache_hit() {
        let c = conn();
        session(&c, "a", "main");
        // Periodo anterior [0, 10): 96 %; actual [10, 20): 91 %.
        call(&c, "p", "a", 5, "claude-sonnet-5", 4, 96, None);
        call(&c, "n", "a", 15, "claude-sonnet-5", 9, 91, None);
        let f = Filter {
            from: Some(10),
            to: Some(20),
            ..Default::default()
        };
        let x = cache_drop(&c, &f, 20).unwrap().unwrap();
        assert!((x.params["pp"].as_f64().unwrap() - 5.0).abs() < 1e-6);
        assert!(
            cache_drop(&c, &Filter::default(), 20).unwrap().is_none(),
            "sin inicio no hay periodo anterior"
        );
    }

    #[test]
    fn ordenados_por_gravedad() {
        let mut v = [
            finding("a", "info", json!({}), String::new()),
            finding("b", "critical", json!({}), String::new()),
            finding("c", "warn", json!({}), String::new()),
        ];
        v.sort_by_key(|x| rank(&x.severity));
        assert_eq!(v.iter().map(|x| x.kind.as_str()).collect::<String>(), "bca");
    }

    fn kinds(c: &Connection, f: &Filter) -> Vec<String> {
        compute(c, f, 100 * DAY, "UTC")
            .unwrap()
            .into_iter()
            .map(|x| x.kind)
            .collect()
    }

    #[test]
    fn avisos_informativos_con_datos_normales() {
        let c = db::open_in_memory().unwrap();
        testdata::seed(&c);
        // s1: web (4.5 USD) · s2: api (1 USD) · s3: web, modelo sin precio.
        let k = kinds(&c, &Filter::default());
        for expected in [
            "unpriced_models",
            "project_share",
            "model_share",
            "top_session",
        ] {
            assert!(
                k.contains(&expected.to_string()),
                "falta {expected} en {k:?}"
            );
        }
        let all = compute(&c, &Filter::default(), 100 * DAY, "UTC").unwrap();
        let share = all.iter().find(|x| x.kind == "project_share").unwrap();
        assert_eq!(share.params["project"], "web");
        let top = all.iter().find(|x| x.kind == "top_session").unwrap();
        assert_eq!(top.params["sessionId"], "s1");
    }

    #[test]
    fn gravedades_ordenadas_con_good_al_final() {
        let c = db::open_in_memory().unwrap();
        testdata::seed(&c);
        // Mucha lectura de caché: el ahorro supera al coste.
        c.execute(
            "UPDATE calls SET cache_read = 50000000 WHERE message_id = 'm1'",
            [],
        )
        .unwrap();
        crate::derived::refresh(&c).unwrap();
        let all = compute(&c, &Filter::default(), 100 * DAY, "UTC").unwrap();
        assert_eq!(all.last().unwrap().severity, "good");
        assert!(all.iter().any(|x| x.kind == "cache_savings"));
        let ranks: Vec<u8> = all.iter().map(|x| rank(&x.severity)).collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn comando_y_mcp_que_fallan() {
        let c = conn();
        session(&c, "a", "main");
        for i in 0..75 {
            let err = i < 9;
            c.execute(
                "INSERT INTO tool_calls (call_id,session_id,ts,tool,target,is_error) VALUES (?1,'a',?2,'Bash','npm test',?3)",
                params![format!("b{i}"), i, err],
            )
            .unwrap();
        }
        for i in 0..20 {
            c.execute(
                "INSERT INTO tool_calls (call_id,session_id,ts,tool,is_error) VALUES (?1,'a',?2,'mcp__figma__get',?3)",
                params![format!("m{i}"), i, i < 5],
            )
            .unwrap();
        }
        crate::derived::refresh(&c).unwrap();
        let cmd = shell_command_errors(&c, &Filter::default())
            .unwrap()
            .unwrap();
        assert_eq!(
            (&cmd.params["command"], &cmd.params["errors"]),
            (&json!("npm"), &json!(9))
        );
        let mcp = mcp_errors(&c, &Filter::default()).unwrap();
        assert_eq!(mcp[0].params["server"], "figma");
    }

    #[test]
    fn fuera_de_horario_y_agentes_sin_uso() {
        let c = conn();
        session(&c, "a", "main");
        // 22:00 UTC (tz 0) → fuera de horario; 11:00 → dentro.
        call(
            &c,
            "n",
            "a",
            22 * 3_600_000,
            "claude-sonnet-5",
            3_000_000,
            0,
            None,
        );
        call(
            &c,
            "d",
            "a",
            11 * 3_600_000,
            "claude-sonnet-5",
            1_000_000,
            0,
            None,
        );
        c.execute("INSERT INTO agents VALUES ('codex','Codex','/x',0)", [])
            .unwrap();
        let s = queries::summary(&c, &Filter::default(), 0).unwrap();
        let late = after_hours(&c, &Filter::default(), &s, "UTC")
            .unwrap()
            .unwrap();
        assert!((late.params["share"].as_f64().unwrap() - 0.75).abs() < 1e-9);
        let idle = unused_agents(&c, &Filter::default()).unwrap().unwrap();
        assert_eq!(idle.params["agents"], "Codex");
    }

    #[test]
    fn coste_por_sesion_al_alza() {
        let c = conn();
        for (i, (ts, input)) in [
            (5, 1_000_000),
            (6, 1_000_000),
            (7, 1_000_000),
            (15, 2_000_000),
            (16, 2_000_000),
            (17, 2_000_000),
        ]
        .iter()
        .enumerate()
        {
            let id = format!("s{i}");
            session(&c, &id, "main");
            call(
                &c,
                &format!("m{i}"),
                &id,
                *ts,
                "claude-sonnet-5",
                *input,
                0,
                None,
            );
        }
        let f = Filter {
            from: Some(10),
            to: Some(20),
            ..Default::default()
        };
        let s = queries::summary(&c, &f, 20).unwrap();
        let x = cost_per_session(&c, &f, &s, 20).unwrap().unwrap();
        assert!(
            (x.params["change"].as_f64().unwrap() - 1.0).abs() < 1e-9,
            "de 2 a 4 USD por sesión"
        );
    }
}
