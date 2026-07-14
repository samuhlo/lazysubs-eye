//! Configuración opcional en `~/.config/lazysubs-eye/config.toml`.
//!
//! Todos los campos tienen defaults que reproducen el comportamiento sin
//! config. Un fichero inválido nunca rompe el output: se avisa por stderr y
//! se usan los defaults.

use serde::Deserialize;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Validez de la cache en segundos (el flag --ttl tiene prioridad).
    pub ttl: i64,
    /// Umbral de warning en % (clase CSS, color de gauge, notificación).
    pub warning_at: f64,
    /// Umbral de critical en %.
    pub critical_at: f64,
    /// Notificaciones de escritorio (notify-send) al cruzar un umbral.
    pub notifications: bool,
    pub providers: Providers,
    pub icons: Icons,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Providers {
    pub claude: bool,
    pub codex: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Icons {
    pub claude: Option<String>,
    pub codex: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ttl: 60,
            warning_at: 80.0,
            critical_at: 95.0,
            notifications: true,
            providers: Providers::default(),
            icons: Icons::default(),
        }
    }
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            claude: true,
            codex: true,
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Icons {
    fn default() -> Self {
        Self {
            claude: None,
            codex: None,
        }
    }
}

fn config_file() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|base| base.join("lazysubs-eye/config.toml"))
}

fn load() -> Config {
    let Some(path) = config_file() else {
        return Config::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Config::default();
    };
    match toml::from_str(&text) {
        Ok(config) => config,
        Err(e) => {
            eprintln!(
                "lazysubs-eye: config inválida en {} (uso los defaults): {e}",
                path.display()
            );
            Config::default()
        }
    }
}

/// Config global, cargada una sola vez. En tests devuelve los defaults para
/// que la config del usuario no afecte a los resultados.
pub fn get() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        if cfg!(test) {
            Config::default()
        } else {
            load()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_vacio_da_los_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config, Config::default());
        assert_eq!(config.ttl, 60);
        assert_eq!(config.warning_at, 80.0);
        assert_eq!(config.critical_at, 95.0);
        assert!(config.notifications);
        assert!(config.providers.claude && config.providers.codex);
    }

    #[test]
    fn toml_completo_se_parsea() {
        let config: Config = toml::from_str(
            r#"
ttl = 120
warning_at = 70
critical_at = 90
notifications = false

[providers]
codex = false

[icons]
claude = "C"
"#,
        )
        .unwrap();
        assert_eq!(config.ttl, 120);
        assert_eq!(config.warning_at, 70.0);
        assert_eq!(config.critical_at, 90.0);
        assert!(!config.notifications);
        assert!(config.providers.claude);
        assert!(!config.providers.codex);
        assert_eq!(config.icons.claude.as_deref(), Some("C"));
        assert_eq!(config.icons.codex, None);
    }

    #[test]
    fn clave_desconocida_es_error() {
        assert!(toml::from_str::<Config>("ttll = 60").is_err());
    }
}
