//! Preferencias del usuario en `settings.json` dentro de la carpeta de configuración
//! (`~/.config/agentboard/` en Linux). Lo demás que la app escribe es la caché reconstruible de
//! `db` (salvo con «Solo memoria»).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// `system`, `light` o `dark`.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// `system`, `es`, `en`, `pt` o `fr`.
    #[serde(default = "default_theme")]
    pub language: String,
    /// Presupuesto mensual en USD; `None` = sin presupuesto.
    pub monthly_budget: Option<f64>,
    /// Presupuesto del día local en USD; `None` = sin presupuesto.
    pub daily_budget: Option<f64>,
    /// Presupuestos mensuales de un proyecto o un agente.
    pub budgets: Vec<ScopedBudget>,
    /// Precios fijados por el usuario; sustituyen a los de por defecto del modelo.
    pub price_overrides: Vec<PriceOverride>,
    /// Avisar al llegar al 80 % de un presupuesto.
    #[serde(default = "yes")]
    pub alert_at_80: bool,
    /// Avisar al llegar al 100 % de un presupuesto.
    #[serde(default = "yes")]
    pub alert_at_100: bool,
    /// La bandeja muestra también el gasto de hoy.
    pub tray_shows_today: bool,
    /// No guardar la caché en disco: los datos viven solo en memoria y al abrir la app se
    /// vuelven a leer todos los logs.
    pub memory_only: bool,
}

fn yes() -> bool {
    true
}

/// Presupuesto mensual acotado a un proyecto (`key` = raíz del repo) o a un agente (`key` = id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedBudget {
    /// `project` o `agent`.
    pub kind: String,
    pub key: String,
    /// Nombre para mostrar (el proyecto o agente puede no estar cargado).
    pub label: String,
    pub monthly: f64,
}

/// Precio de un modelo en USD por millón de tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceOverride {
    pub model: String,
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
    #[serde(default)]
    pub cache_write_1h: f64,
}

fn default_theme() -> String {
    "system".into()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            language: default_theme(),
            monthly_budget: None,
            daily_budget: None,
            budgets: Vec::new(),
            price_overrides: Vec::new(),
            alert_at_80: true,
            alert_at_100: true,
            tray_shows_today: false,
            memory_only: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            bail!("tema desconocido: {}", self.theme);
        }
        if !matches!(self.language.as_str(), "system" | "es" | "en" | "pt" | "fr") {
            bail!("idioma desconocido: {}", self.language);
        }
        for b in [self.monthly_budget, self.daily_budget]
            .into_iter()
            .flatten()
        {
            if !b.is_finite() || b < 0.0 {
                bail!("el presupuesto debe ser un número mayor o igual que 0");
            }
        }
        for b in &self.budgets {
            if !matches!(b.kind.as_str(), "project" | "agent") {
                bail!("tipo de presupuesto desconocido: {}", b.kind);
            }
            if b.key.trim().is_empty() {
                bail!("el presupuesto necesita un proyecto o agente");
            }
            if !b.monthly.is_finite() || b.monthly <= 0.0 {
                bail!("el presupuesto de {} debe ser mayor que 0", b.label);
            }
        }
        let mut seen = std::collections::HashSet::new();
        for p in &self.price_overrides {
            if p.model.trim().is_empty() {
                bail!("el precio necesita el nombre del modelo");
            }
            if !seen.insert(p.model.as_str()) {
                bail!("el modelo {} tiene dos precios", p.model);
            }
            let values = [
                p.input,
                p.output,
                p.cache_read,
                p.cache_write,
                p.cache_write_1h,
            ];
            if values.iter().any(|v| !v.is_finite() || *v < 0.0) {
                bail!(
                    "los precios de {} deben ser números mayores o iguales que 0",
                    p.model
                );
            }
        }
        Ok(())
    }
}

pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("agentboard").join("settings.json"))
}

pub fn load_from(path: &std::path::Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_to(path: &std::path::Path, s: &Settings) -> Result<()> {
    s.validate()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("no se pudo crear {}", dir.display()))?;
    }
    std::fs::write(path, serde_json::to_string_pretty(s)?)
        .with_context(|| format!("no se pudo escribir {}", path.display()))?;
    Ok(())
}

pub fn load() -> Settings {
    default_path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn save(s: &Settings) -> Result<()> {
    let path = default_path().context("no se encontró la carpeta de configuración")?;
    save_to(&path, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presupuesto_persiste() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg").join("settings.json");
        save_to(
            &path,
            &Settings {
                theme: "light".into(),
                language: "fr".into(),
                monthly_budget: Some(50.0),
                ..Default::default()
            },
        )
        .unwrap();
        let loaded = load_from(&path);
        assert_eq!(
            (
                loaded.theme.as_str(),
                loaded.language.as_str(),
                loaded.monthly_budget
            ),
            ("light", "fr", Some(50.0))
        );
        assert!(save_to(
            &path,
            &Settings {
                theme: "neon".into(),
                ..Default::default()
            }
        )
        .is_err());
        assert!(save_to(
            &path,
            &Settings {
                language: "de".into(),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn presupuestos_y_precios_del_usuario() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let price = PriceOverride {
            model: "kimi-k2".into(),
            input: 1.0,
            output: 4.0,
            cache_read: 0.1,
            cache_write: 0.0,
            cache_write_1h: 0.0,
        };
        let s = Settings {
            daily_budget: Some(30.0),
            budgets: vec![ScopedBudget {
                kind: "agent".into(),
                key: "codex".into(),
                label: "Codex CLI".into(),
                monthly: 60.0,
            }],
            price_overrides: vec![price.clone()],
            ..Default::default()
        };
        save_to(&path, &s).unwrap();
        assert_eq!(load_from(&path), s);

        let bad = |f: &dyn Fn(&mut Settings)| {
            let mut x = s.clone();
            f(&mut x);
            save_to(&path, &x).is_err()
        };
        assert!(bad(&|x| x.price_overrides[0].input = -1.0));
        assert!(bad(&|x| x.price_overrides.push(price.clone())));
        assert!(bad(&|x| x.budgets[0].monthly = 0.0));
        assert!(bad(&|x| x.budgets[0].kind = "rama".into()));
        assert!(bad(&|x| x.daily_budget = Some(f64::NAN)));
        assert_eq!(
            load_from(&path),
            s,
            "un ajuste inválido no pisa el anterior"
        );
    }

    #[test]
    fn ajustes_antiguos_se_leen_sin_los_campos_nuevos() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"theme":"dark","language":"es","monthlyBudget":40}"#,
        )
        .unwrap();
        let s = load_from(&path);
        assert_eq!(
            (s.monthly_budget, s.daily_budget, s.budgets.len()),
            (Some(40.0), None, 0)
        );
    }

    #[test]
    fn solo_memoria_esta_desactivado_por_defecto_y_se_guarda() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"theme":"dark"}"#).unwrap();
        assert!(!load_from(&path).memory_only);
        save_to(
            &path,
            &Settings {
                memory_only: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(load_from(&path).memory_only);
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("\"memoryOnly\": true"));
    }

    #[test]
    fn negativo_se_rechaza_y_se_mantiene_el_anterior() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        save_to(
            &path,
            &Settings {
                monthly_budget: Some(50.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(save_to(
            &path,
            &Settings {
                monthly_budget: Some(-1.0),
                ..Default::default()
            }
        )
        .is_err());
        assert_eq!(load_from(&path).monthly_budget, Some(50.0));
        assert_eq!(
            load_from(&dir.path().join("no-existe.json")).monthly_budget,
            None
        );
    }
}
