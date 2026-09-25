//! Preferencias del usuario en `settings.json` dentro de la carpeta de configuración
//! (`~/.config/agentboard/` en Linux). Es lo único que la app escribe en disco.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Presupuesto mensual en USD; `None` = sin presupuesto.
    pub monthly_budget: Option<f64>,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
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
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_to(path: &std::path::Path, s: &Settings) -> Result<()> {
    s.validate()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    }
    std::fs::write(path, serde_json::to_string_pretty(s)?).with_context(|| format!("no se pudo escribir {}", path.display()))?;
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
        save_to(&path, &Settings { monthly_budget: Some(50.0) }).unwrap();
        assert_eq!(load_from(&path).monthly_budget, Some(50.0));
    }

    #[test]
    fn negativo_se_rechaza_y_se_mantiene_el_anterior() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        save_to(&path, &Settings { monthly_budget: Some(50.0) }).unwrap();
        assert!(save_to(&path, &Settings { monthly_budget: Some(-1.0) }).is_err());
        assert_eq!(load_from(&path).monthly_budget, Some(50.0));
        assert_eq!(load_from(&dir.path().join("no-existe.json")).monthly_budget, None);
    }
}
