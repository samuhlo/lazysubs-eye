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
    /// Colores de umbral (clase warning/critical en waybar, semáforo de los
    /// gauges de la TUI). En false todo va en color neutro; la clase `error`
    /// se mantiene porque señala rotura, no uso.
    pub colors: bool,
    pub providers: Providers,
    pub icons: Icons,
    pub minimax: MiniMax,
    pub waybar: Waybar,
    pub tui: Tui,
}

/// Qué pinta el módulo de waybar.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Waybar {
    /// Providers visibles en la barra **y su orden** (ids: "claude", "codex",
    /// "minimax"). Ausente = todos, en el orden de colección.
    pub providers: Option<Vec<String>>,
    /// En false la barra muestra solo los iconos, sin porcentaje.
    pub percent: Option<bool>,
}

impl Waybar {
    pub fn percent(&self) -> bool {
        self.percent.unwrap_or(true)
    }
}

/// Qué pinta la TUI.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Tui {
    /// Providers visibles en la TUI y su orden. Ausente = todos.
    pub providers: Option<Vec<String>>,
    /// Paneles de tokens diarios: "claude_tokens", "pi_tokens",
    /// "opencode_tokens". Ausente = todos.
    pub panels: Option<Vec<String>>,
}

impl Tui {
    pub fn panel(&self, name: &str) -> bool {
        match &self.panels {
            Some(panels) => panels.iter().any(|p| p == name),
            None => true,
        }
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Providers {
    pub claude: bool,
    pub codex: bool,
    pub minimax: bool,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Icons {
    pub claude: Option<String>,
    pub codex: Option<String>,
    pub minimax: Option<String>,
}

/// Credenciales de MiniMax: el plan solo se puede consultar con la
/// Subscription Key del token plan (config o env `MINIMAX_API_KEY`).
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MiniMax {
    pub api_key: Option<String>,
    /// Host alternativo (p. ej. https://api.minimaxi.com para China).
    pub base_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ttl: 60,
            warning_at: 80.0,
            critical_at: 95.0,
            notifications: true,
            colors: true,
            providers: Providers::default(),
            icons: Icons::default(),
            minimax: MiniMax::default(),
            waybar: Waybar::default(),
            tui: Tui::default(),
        }
    }
}

impl Default for Providers {
    fn default() -> Self {
        Self {
            claude: true,
            codex: true,
            minimax: true,
        }
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Icons {
    fn default() -> Self {
        Self {
            claude: None,
            codex: None,
            minimax: None,
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

[minimax]
api_key = "sk-test"

[waybar]
providers = ["minimax", "claude"]
percent = false

[tui]
panels = ["claude_tokens"]
"#,
        )
        .unwrap();
        assert_eq!(config.ttl, 120);
        assert_eq!(config.warning_at, 70.0);
        assert_eq!(config.critical_at, 90.0);
        assert!(!config.notifications);
        assert!(config.providers.claude);
        assert!(!config.providers.codex);
        assert!(config.providers.minimax);
        assert_eq!(config.icons.claude.as_deref(), Some("C"));
        assert_eq!(config.icons.codex, None);
        assert_eq!(config.minimax.api_key.as_deref(), Some("sk-test"));
        assert_eq!(config.minimax.base_url, None);
        assert_eq!(
            config.waybar.providers,
            Some(vec!["minimax".into(), "claude".into()])
        );
        assert!(!config.waybar.percent());
        assert!(config.tui.providers.is_none());
        assert!(config.tui.panel("claude_tokens"));
        assert!(!config.tui.panel("pi_tokens"));
        assert!(config.colors); // default si no se toca
    }

    #[test]
    fn clave_desconocida_es_error() {
        assert!(toml::from_str::<Config>("ttll = 60").is_err());
    }
}
