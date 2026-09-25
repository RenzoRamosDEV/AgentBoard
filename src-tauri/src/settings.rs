//! Preferencias del usuario en `settings.json` dentro de la carpeta de configuración
//! (`~/.config/agentboard/` en Linux). Es lo único que la app escribe en disco.

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
        if let Some(b) = self.monthly_budget {
            if !b.is_finite() || b < 0.0 {
                bail!("el presupuesto debe ser un número mayor o igual que 0");
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
