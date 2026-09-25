//! Bandeja del sistema y avisos de presupuesto. Tras cada relectura, actualiza el texto de la
//! bandeja con el gasto de hoy y avisa una vez al superar el 80 % y el 100 % del presupuesto mensual.

use crate::{queries, settings};
use rusqlite::Connection;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use tauri::tray::TrayIcon;
use tauri_plugin_notification::NotificationExt;

/// Umbrales de presupuesto ya avisados en esta sesión (bit 0 = 80 %, bit 1 = 100 %).
#[derive(Default)]
pub struct Alerts {
    notified: AtomicU8,
}

impl Alerts {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Recalcula tras un escaneo: refresca la bandeja y lanza los avisos de presupuesto pendientes.
    pub fn refresh(&self, app: &tauri::AppHandle, db: &Arc<Mutex<Connection>>, tray: &TrayIcon) {
        let filter = queries::Filter::default();
        let (spent, projection) = match db.lock() {
            Ok(conn) => queries::month_progress(&conn, &filter).unwrap_or((0.0, 0.0)),
            Err(_) => return,
        };

        let _ = tray.set_tooltip(Some(&format!("AgentBoard — este mes: ${spent:.2} (proyección ${projection:.2})")));

        let budget = settings::load().monthly_budget;
        if let Some(budget) = budget.filter(|b| *b > 0.0) {
            let ratio = projection / budget;
            let mut done = self.notified.load(Ordering::SeqCst);
            if ratio >= 1.0 && done & 0b10 == 0 {
                notify(app, "Presupuesto superado", &format!("La proyección del mes (${projection:.2}) supera tu presupuesto de ${budget:.2}."));
                done |= 0b10 | 0b01;
            } else if ratio >= 0.8 && done & 0b01 == 0 {
                notify(app, "Cerca del presupuesto", &format!("Vas por el {:.0}% de tu presupuesto mensual de ${budget:.2}.", ratio * 100.0));
                done |= 0b01;
            }
            self.notified.store(done, Ordering::SeqCst);
        }
    }

    /// Al cambiar el presupuesto en Ajustes, se vuelven a permitir los avisos.
    pub fn reset(&self) {
        self.notified.store(0, Ordering::SeqCst);
    }
}

fn notify(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}
