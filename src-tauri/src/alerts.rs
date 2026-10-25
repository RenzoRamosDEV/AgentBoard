//! Bandeja del sistema y avisos de presupuesto. Tras cada relectura, actualiza el texto de la
//! bandeja con el gasto del mes y avisa una vez al llegar al 80 % y al 100 % de cada presupuesto:
//! mensual, diario y mensuales por proyecto o agente.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use crate::{queries, settings};
use anyhow::Result;
use rusqlite::{params_from_iter, Connection};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use tauri::tray::TrayIcon;
use tauri_plugin_notification::NotificationExt;

/// Un presupuesto frente a su valor actual (proyección del mes o gasto del día).
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// Identifica el presupuesto y su periodo para no repetir avisos y volver a avisar al
    /// cambiar de día o de mes (`month:2026-09`, `day:2026-09-27`, `project:/repo:2026-09`…).
    pub id: String,
    /// «tu presupuesto mensual», «el presupuesto de Codex CLI»…
    pub name: String,
    /// «La proyección del mes», «El gasto de hoy».
    pub measure: &'static str,
    pub value: f64,
    pub limit: f64,
}

/// Qué umbrales avisar (preferencias de Ajustes).
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    pub at_80: bool,
    pub at_100: bool,
}

impl Thresholds {
    pub const ALL: Self = Self {
        at_80: true,
        at_100: true,
    };
}

/// Avisos (título, texto) pendientes; marca en `notified` los umbrales avisados.
pub fn pending(
    checks: &[Check],
    notified: &mut HashSet<String>,
    th: Thresholds,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for c in checks.iter().filter(|c| c.limit > 0.0) {
        let ratio = c.value / c.limit;
        let (k80, k100) = (format!("{}:80", c.id), format!("{}:100", c.id));
        if ratio >= 1.0 && th.at_100 && !notified.contains(&k100) {
            out.push((
                "Presupuesto superado".to_string(),
                format!(
                    "{} (${:.2}) supera {} de ${:.2}.",
                    c.measure, c.value, c.name, c.limit
                ),
            ));
            notified.insert(k100);
            notified.insert(k80);
        } else if ratio >= 0.8 && th.at_80 && !notified.contains(&k80) {
            out.push((
                "Cerca del presupuesto".to_string(),
                format!(
                    "{} va por el {:.0}% de {} de ${:.2}.",
                    c.measure,
                    ratio * 100.0,
                    c.name,
                    c.limit
                ),
            ));
            notified.insert(k80);
        }
    }
    // Depura los umbrales de periodos pasados: al cambiar el día o el mes cambia la clave,
    // así que el mismo presupuesto vuelve a avisar (la app puede vivir días en la bandeja).
    notified.retain(|k| {
        checks.iter().any(|c| {
            k.strip_prefix(c.id.as_str())
                .is_some_and(|r| r.starts_with(':'))
        })
    });
    out
}

/// Inicio del día local en curso (epoch ms).
pub fn today_start() -> i64 {
    use chrono::{Local, TimeZone};
    Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| Local.from_local_datetime(&midnight).earliest())
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

/// Gasto del día local en curso, de todos los agentes y proyectos.
pub fn today_spent(conn: &Connection) -> Result<f64> {
    let today = queries::Filter {
        from: Some(today_start()),
        ..Default::default()
    };
    let (w, args) = today.sql("c.ts");
    Ok(conn.query_row(
        &format!("SELECT COALESCE(SUM(c.cost_usd),0) FROM call_costs c JOIN sessions s ON s.id = c.session_id WHERE {w}"),
        params_from_iter(args.iter()),
        |r| r.get(0),
    )?)
}

/// Comprobaciones de todos los presupuestos de los ajustes.
pub fn checks(conn: &Connection, s: &settings::Settings) -> Result<Vec<Check>> {
    let mut out = Vec::new();
    // El periodo forma parte de la identidad del aviso (ver `Check::id`).
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let month_key = &today[..7];
    let (_, projection) = queries::month_progress(conn, &queries::Filter::default())?;
    if let Some(b) = s.monthly_budget {
        out.push(Check {
            id: format!("month:{month_key}"),
            name: "tu presupuesto mensual".into(),
            measure: "La proyección del mes",
            value: projection,
            limit: b,
        });
    }
    if let Some(b) = s.daily_budget {
        let spent = today_spent(conn)?;
        out.push(Check {
            id: format!("day:{today}"),
            name: "tu presupuesto diario".into(),
            measure: "El gasto de hoy",
            value: spent,
            limit: b,
        });
    }
    if !s.budgets.is_empty() {
        let (start, factor) = queries::month_factor();
        let month = queries::Filter {
            from: Some(start),
            ..Default::default()
        };
        let projects = queries::breakdown(conn, &month, "project")?;
        let agents = queries::breakdown(conn, &month, "agent")?;
        for b in &s.budgets {
            let rows = if b.kind == "project" {
                &projects
            } else {
                &agents
            };
            let spent = rows
                .iter()
                .find(|r| r.key == b.key)
                .map_or(0.0, |r| r.cost_usd);
            out.push(Check {
                id: format!("{}:{}:{month_key}", b.kind, b.key),
                name: format!("el presupuesto de {}", b.label),
                measure: "La proyección del mes",
                value: spent * factor,
                limit: b.monthly,
            });
        }
    }
    Ok(out)
}

/// Umbrales de presupuesto ya avisados en esta sesión de la app.
#[derive(Default)]
pub struct Alerts {
    notified: Mutex<HashSet<String>>,
}

impl Alerts {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Recalcula tras un escaneo: refresca la bandeja y lanza los avisos de presupuesto pendientes.
    pub fn refresh(&self, app: &tauri::AppHandle, readers: &crate::db::ReadPool, tray: &TrayIcon) {
        let s = settings::load();
        let read = readers.with(|conn| {
            Ok((
                queries::month_progress(conn, &queries::Filter::default()).unwrap_or((0.0, 0.0)),
                s.tray_shows_today.then(|| today_spent(conn).ok()).flatten(),
                checks(conn, &s).unwrap_or_default(),
            ))
        });
        let Ok((month, today, checks)) = read else {
            return;
        };
        let (spent, projection) = month;
        let today = today
            .map(|t| format!(" · hoy: ${t:.2}"))
            .unwrap_or_default();
        let _ = tray.set_tooltip(Some(&format!(
            "AgentBoard — este mes: ${spent:.2} (proyección ${projection:.2}){today}"
        )));

        let Ok(mut notified) = self.notified.lock() else {
            return;
        };
        let th = Thresholds {
            at_80: s.alert_at_80,
            at_100: s.alert_at_100,
        };
        for (title, body) in pending(&checks, &mut notified, th) {
            let _ = app
                .notification()
                .builder()
                .title(&title)
                .body(&body)
                .show();
        }
    }

    /// Al cambiar los presupuestos en Ajustes, se vuelven a permitir los avisos.
    pub fn reset(&self) {
        if let Ok(mut n) = self.notified.lock() {
            n.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_dia_en_curso_empieza_en_las_ultimas_26_horas() {
        let now = crate::ingest::now_ms();
        let start = today_start();
        assert!(
            start <= now && now - start < 26 * 3_600_000,
            "{start} {now}"
        );
    }
    use crate::db;
    use crate::queries::testdata;
    use crate::settings::{ScopedBudget, Settings};

    fn check(id: &str, value: f64, limit: f64) -> Check {
        Check {
            id: id.into(),
            name: format!("el presupuesto de {id}"),
            measure: "La proyección del mes",
            value,
            limit,
        }
    }

    #[test]
    fn avisa_una_vez_por_umbral() {
        let mut notified = HashSet::new();
        let a = pending(
            &[check("codex", 58.0, 60.0)],
            &mut notified,
            Thresholds::ALL,
        );
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].0, "Cerca del presupuesto");
        assert!(a[0].1.contains("97%"));
        // Misma situación: no se repite.
        assert!(pending(
            &[check("codex", 58.0, 60.0)],
            &mut notified,
            Thresholds::ALL
        )
        .is_empty());
        // Al superarlo, aviso del 100 % (una vez).
        let b = pending(
            &[check("codex", 61.0, 60.0)],
            &mut notified,
            Thresholds::ALL,
        );
        assert_eq!(b[0].0, "Presupuesto superado");
        assert!(pending(
            &[check("codex", 70.0, 60.0)],
            &mut notified,
            Thresholds::ALL
        )
        .is_empty());
    }

    #[test]
    fn directo_al_100_marca_tambien_el_80() {
        let mut notified = HashSet::new();
        assert_eq!(
            pending(&[check("day", 31.0, 30.0)], &mut notified, Thresholds::ALL).len(),
            1
        );
        assert!(pending(&[check("day", 25.0, 30.0)], &mut notified, Thresholds::ALL).is_empty());
    }

    #[test]
    fn respeta_los_umbrales_desactivados() {
        let mut notified = HashSet::new();
        let only100 = Thresholds {
            at_80: false,
            at_100: true,
        };
        assert!(pending(&[check("a", 85.0, 100.0)], &mut notified, only100).is_empty());
        assert_eq!(
            pending(&[check("a", 101.0, 100.0)], &mut notified, only100).len(),
            1
        );
        let none = Thresholds {
            at_80: false,
            at_100: false,
        };
        assert!(pending(&[check("b", 200.0, 100.0)], &mut HashSet::new(), none).is_empty());
    }

    #[test]
    fn sin_limite_o_por_debajo_no_avisa() {
        let mut notified = HashSet::new();
        assert!(pending(
            &[check("a", 10.0, 0.0), check("b", 1.0, 60.0)],
            &mut notified,
            Thresholds::ALL
        )
        .is_empty());
    }

    #[test]
    fn comprobaciones_de_los_ajustes() {
        let conn = db::open_in_memory().unwrap();
        testdata::seed(&conn);
        let s = Settings {
            monthly_budget: Some(50.0),
            daily_budget: Some(5.0),
            budgets: vec![ScopedBudget {
                kind: "agent".into(),
                key: "codex".into(),
                label: "Codex".into(),
                monthly: 10.0,
            }],
            ..Default::default()
        };
        let c = checks(&conn, &s).unwrap();
        let ids: Vec<&str> = c.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
        assert!(ids[0].starts_with("month:2"), "mes con periodo: {}", ids[0]);
        assert!(
            ids[1].starts_with("day:2") && ids[1].len() == "day:2026-09-27".len(),
            "día con fecha: {}",
            ids[1]
        );
        assert!(
            ids[2].starts_with("agent:codex:2"),
            "presupuesto por ámbito con mes: {}",
            ids[2]
        );
        assert!(c.iter().all(|x| x.value >= 0.0));
    }

    #[test]
    fn el_aviso_caduca_al_cambiar_de_dia() {
        let mut notified = HashSet::new();
        assert_eq!(
            pending(
                &[check("day:2026-09-27", 31.0, 30.0)],
                &mut notified,
                Thresholds::ALL
            )
            .len(),
            1
        );
        // Mismo día: no repite.
        assert!(pending(
            &[check("day:2026-09-27", 31.0, 30.0)],
            &mut notified,
            Thresholds::ALL
        )
        .is_empty());
        // Al día siguiente la clave cambia: avisa de nuevo y depura la del día anterior.
        assert_eq!(
            pending(
                &[check("day:2026-09-28", 31.0, 30.0)],
                &mut notified,
                Thresholds::ALL
            )
            .len(),
            1
        );
        assert!(
            !notified.iter().any(|k| k.contains("2026-09-27")),
            "las claves del día anterior se depuran"
        );
    }
}
