//! Configuración opcional en `~/.config/lazysubs-eye/config.toml`.
//!
//! Todos los campos tienen defaults que reproducen el comportamiento sin
//! config. Un fichero inválido nunca rompe el output: se avisa por stderr y
//! se usan los defaults.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::RwLock;
use toml_edit::{value, DocumentMut};

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
    /// Segundos mínimos entre notificaciones repetidas de una misma ventana
    /// (resets rodantes, bajar y volver a cruzar). La escalada a un nivel
    /// superior no espera. Alto por defecto para no ametrallar.
    pub notification_cooldown: i64,
    /// Colores de umbral (clase warning/critical en waybar, semáforo de los
    /// gauges de la TUI). En false todo va en color neutro; la clase `error`
    /// se mantiene porque señala rotura, no uso.
    pub colors: bool,
    /// Mostrar la cuenta (email/alias) junto al plan en la TUI y el tooltip.
    pub show_account: bool,
    pub providers: Providers,
    pub icons: Icons,
    pub minimax: MiniMax,
    pub waybar: Waybar,
    pub tui: Tui,
    pub stats: Stats,
    pub accounts: Accounts,
}

/// Multicuenta por IA. Vacío = una cuenta autodetectada por provider (el
/// comportamiento de siempre). Con entradas, cada una produce su propio
/// `ProviderStatus`; ver `providers::collect_all`.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Accounts {
    pub claude: Vec<ClaudeAccount>,
    pub codex: Vec<CodexAccount>,
    pub minimax: Vec<MiniMaxAccount>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClaudeAccount {
    /// Alias visible (se compone en el id `claude:<name>` y en el nombre).
    pub name: String,
    /// Ruta del `.credentials.json`; por defecto `~/.claude/.credentials.json`.
    pub credentials: Option<String>,
    pub icon: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CodexAccount {
    pub name: String,
    /// Directorio que se pasa como `CODEX_HOME` al app-server; por defecto
    /// `~/.codex`.
    pub codex_home: Option<String>,
    pub icon: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MiniMaxAccount {
    pub name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub icon: Option<String>,
}

/// Expande un `~` inicial usando `$HOME`. Rutas sin tilde se devuelven tal cual.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

/// Historial de gasto de tokens (SQLite en XDG_STATE_HOME) y sus estadísticas.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Stats {
    /// En false no se abre la base ni se pintan paneles de periodo (como antes).
    pub enabled: bool,
    /// Periodo inicial de los paneles de tokens: "hoy" | "semana" | "mes".
    pub default_period: String,
    /// Retención en días; 0 = sin límite.
    pub history_days: i64,
    /// Sparkline con el total diario bajo cada panel.
    pub sparkline: bool,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            enabled: true,
            default_period: "hoy".into(),
            history_days: 90,
            sparkline: true,
        }
    }
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
            notification_cooldown: 30 * 60,
            colors: true,
            show_account: true,
            providers: Providers::default(),
            icons: Icons::default(),
            minimax: MiniMax::default(),
            waybar: Waybar::default(),
            tui: Tui::default(),
            stats: Stats::default(),
            accounts: Accounts::default(),
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

static CONFIG: RwLock<Option<Config>> = RwLock::new(None);

/// Config global, cargada del fichero la primera vez y actualizable en
/// caliente desde el panel de opciones de la TUI. En tests devuelve los
/// defaults para que la config del usuario no afecte a los resultados.
pub fn get() -> Config {
    if let Some(config) = CONFIG.read().unwrap().as_ref() {
        return config.clone();
    }
    let loaded = if cfg!(test) {
        Config::default()
    } else {
        load()
    };
    *CONFIG.write().unwrap() = Some(loaded.clone());
    loaded
}

/// Sustituye la config en memoria (panel de opciones de la TUI).
pub fn set(config: Config) {
    *CONFIG.write().unwrap() = Some(config);
}

/// Añade o quita un id de una lista de superficie. `None` significa "todos",
/// así que al primer cambio se materializa la lista completa.
pub fn toggle_id(list: &mut Option<Vec<String>>, all: &[&str], id: &str) {
    let mut current: Vec<String> = match list {
        None => all.iter().map(|s| s.to_string()).collect(),
        Some(v) => v.clone(),
    };
    match current.iter().position(|x| x == id) {
        Some(at) => {
            current.remove(at);
        }
        None => current.push(id.to_string()),
    }
    *list = Some(current);
}

/// Persiste en config.toml los campos editables desde la TUI, conservando
/// comentarios y claves que no gestionamos (p. ej. [minimax] api_key). Solo
/// se escriben claves que difieren del default o que ya existían.
pub fn persist(config: &Config) -> Result<()> {
    let path = config_file().context("sin HOME ni XDG_CONFIG_HOME")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok();
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: DocumentMut = text
        .parse()
        .with_context(|| format!("config.toml existente inválido, no lo toco: {path:?}"))?;
    apply_to_doc(&mut doc, config);
    crate::cache::atomic_save(&path, doc.to_string().as_bytes())
        .with_context(|| format!("no pude escribir {path:?}"))?;
    Ok(())
}

/// Vuelca en el documento TOML los campos editables desde la TUI. Separado de
/// persist() para poder testearlo sin tocar el filesystem.
fn apply_to_doc(doc: &mut DocumentMut, config: &Config) {
    let defaults = Config::default();

    fn set_if(doc: &mut DocumentMut, key: &str, differs: bool, v: toml_edit::Value) {
        if differs || doc.contains_key(key) {
            doc[key] = value(v);
        }
    }
    set_if(doc, "ttl", config.ttl != defaults.ttl, config.ttl.into());
    set_if(
        doc,
        "warning_at",
        config.warning_at != defaults.warning_at,
        config.warning_at.into(),
    );
    set_if(
        doc,
        "critical_at",
        config.critical_at != defaults.critical_at,
        config.critical_at.into(),
    );
    set_if(
        doc,
        "notifications",
        config.notifications != defaults.notifications,
        config.notifications.into(),
    );
    set_if(
        doc,
        "notification_cooldown",
        config.notification_cooldown != defaults.notification_cooldown,
        config.notification_cooldown.into(),
    );
    set_if(
        doc,
        "colors",
        config.colors != defaults.colors,
        config.colors.into(),
    );
    set_if(
        doc,
        "show_account",
        config.show_account != defaults.show_account,
        config.show_account.into(),
    );

    let any_provider_off =
        !(config.providers.claude && config.providers.codex && config.providers.minimax);
    if any_provider_off || doc.contains_key("providers") {
        let table = ensure_table(doc, "providers");
        table["claude"] = value(config.providers.claude);
        table["codex"] = value(config.providers.codex);
        table["minimax"] = value(config.providers.minimax);
    }

    set_list(doc, "waybar", "providers", &config.waybar.providers);
    if let Some(percent) = config.waybar.percent {
        ensure_table(doc, "waybar")["percent"] = value(percent);
    }
    set_list(doc, "tui", "providers", &config.tui.providers);
    set_list(doc, "tui", "panels", &config.tui.panels);

    let stats = &config.stats;
    let stats_defaults = Stats::default();
    let stats_differs = stats != &stats_defaults;
    if stats_differs || doc.contains_key("stats") {
        let table = ensure_table(doc, "stats");
        table["enabled"] = value(stats.enabled);
        table["default_period"] = value(stats.default_period.as_str());
        table["history_days"] = value(stats.history_days);
        table["sparkline"] = value(stats.sparkline);
    }
}

/// Tabla explícita (`[nombre]` al final del fichero), nunca inline: toml_edit
/// crearía `nombre = { … }` en la primera línea, delante de los comentarios.
fn ensure_table<'a>(doc: &'a mut DocumentMut, name: &str) -> &'a mut toml_edit::Table {
    if !doc.contains_key(name) || doc[name].as_table().is_none() {
        doc[name] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc[name].as_table_mut().expect("recién insertada")
}

fn set_list(doc: &mut DocumentMut, table: &str, key: &str, list: &Option<Vec<String>>) {
    match list {
        Some(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(item.as_str());
            }
            ensure_table(doc, table)[key] = value(array);
        }
        None => {
            if let Some(t) = doc.get_mut(table).and_then(|i| i.as_table_mut()) {
                t.remove(key);
            }
        }
    }
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

    #[test]
    fn toggle_id_materializa_quita_y_pone() {
        let all = ["a", "b", "c"];
        let mut list = None;
        toggle_id(&mut list, &all, "b");
        assert_eq!(list, Some(vec!["a".to_string(), "c".to_string()]));
        toggle_id(&mut list, &all, "b");
        assert_eq!(
            list,
            Some(vec!["a".to_string(), "c".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn apply_to_doc_conserva_comentarios_y_claves_ajenas() {
        let original = "\
# mi comentario importante
# ttl = 60

[minimax]
api_key = \"sk-secreta\"  # no tocar
";
        let mut doc: DocumentMut = original.parse().unwrap();
        let config = Config {
            notifications: false,
            tui: Tui {
                panels: Some(vec!["claude_tokens".into()]),
                ..Tui::default()
            },
            ..Config::default()
        };
        apply_to_doc(&mut doc, &config);
        let out = doc.to_string();

        assert!(out.contains("# mi comentario importante"));
        assert!(out.contains("# ttl = 60"), "comentarios intactos");
        assert!(
            out.contains("api_key = \"sk-secreta\""),
            "clave ajena intacta"
        );
        assert!(out.contains("notifications = false"));
        assert!(out.contains("panels = [\"claude_tokens\"]"));
        assert!(out.contains("[tui]"), "tabla explícita, no inline: {out}");
        // lo que sigue en default y no estaba, no se añade
        assert!(!out.contains("warning_at"));
        assert!(!out.contains("[providers]"));

        // el resultado se puede volver a cargar
        let reloaded: Config = toml::from_str(&out).unwrap();
        assert!(!reloaded.notifications);
        assert!(reloaded.tui.panel("claude_tokens"));
        assert!(!reloaded.tui.panel("pi_tokens"));
    }

    #[test]
    fn apply_to_doc_vuelve_al_default_actualizando_la_clave_existente() {
        let mut doc: DocumentMut = "notifications = false\n".parse().unwrap();
        apply_to_doc(&mut doc, &Config::default());
        assert!(doc.to_string().contains("notifications = true"));
    }

    #[test]
    fn accounts_multicuenta_se_parsean() {
        let config: Config = toml::from_str(
            r#"
[[accounts.claude]]
name = "personal"

[[accounts.claude]]
name = "trabajo"
credentials = "~/trabajo/.claude/.credentials.json"
icon = "❄"

[[accounts.codex]]
name = "personal"
codex_home = "~/.codex"

[[accounts.minimax]]
name = "personal"
api_key = "sk-x"
"#,
        )
        .unwrap();
        assert_eq!(config.accounts.claude.len(), 2);
        assert_eq!(config.accounts.claude[0].name, "personal");
        assert_eq!(config.accounts.claude[0].credentials, None);
        assert_eq!(
            config.accounts.claude[1].credentials.as_deref(),
            Some("~/trabajo/.claude/.credentials.json")
        );
        assert_eq!(config.accounts.claude[1].icon.as_deref(), Some("❄"));
        assert_eq!(config.accounts.codex.len(), 1);
        assert_eq!(
            config.accounts.codex[0].codex_home.as_deref(),
            Some("~/.codex")
        );
        assert_eq!(config.accounts.minimax[0].api_key.as_deref(), Some("sk-x"));

        // sin tabla → vacío (comportamiento de siempre)
        let default: Config = toml::from_str("").unwrap();
        assert!(default.accounts.claude.is_empty());
        assert!(default.accounts.codex.is_empty());
        assert!(default.accounts.minimax.is_empty());
    }

    #[test]
    fn expand_tilde_usa_home() {
        std::env::set_var("HOME", "/home/tester");
        assert_eq!(expand_tilde("~/x/y"), PathBuf::from("/home/tester/x/y"));
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
        assert_eq!(expand_tilde("rel"), PathBuf::from("rel"));
    }

    #[test]
    fn account_con_clave_desconocida_es_error() {
        assert!(
            toml::from_str::<Config>("[[accounts.claude]]\nname = \"x\"\nbogus = 1\n").is_err()
        );
    }

    #[test]
    fn stats_por_defecto_y_parseo() {
        let config: Config = toml::from_str(
            r#"
[stats]
enabled = false
default_period = "mes"
history_days = 30
sparkline = false
"#,
        )
        .unwrap();
        assert!(!config.stats.enabled);
        assert_eq!(config.stats.default_period, "mes");
        assert_eq!(config.stats.history_days, 30);
        assert!(!config.stats.sparkline);

        // sin tabla [stats] → defaults
        let default: Config = toml::from_str("").unwrap();
        assert!(default.stats.enabled);
        assert_eq!(default.stats.default_period, "hoy");
        assert_eq!(default.stats.history_days, 90);
        assert!(default.stats.sparkline);
    }

    #[test]
    fn apply_to_doc_persiste_stats_no_default_y_recarga() {
        let mut doc: DocumentMut = "# cfg\n".parse().unwrap();
        let config = Config {
            stats: Stats {
                enabled: false,
                default_period: "semana".into(),
                history_days: 30,
                sparkline: false,
            },
            ..Config::default()
        };
        apply_to_doc(&mut doc, &config);
        let out = doc.to_string();
        assert!(out.contains("[stats]"), "tabla explícita: {out}");
        assert!(out.contains("enabled = false"));
        assert!(out.contains("default_period = \"semana\""));
        assert!(out.contains("history_days = 30"));

        let reloaded: Config = toml::from_str(&out).unwrap();
        assert_eq!(reloaded.stats, config.stats);

        // stats por defecto y sin tabla previa → no se escribe nada
        let mut doc: DocumentMut = "# cfg\n".parse().unwrap();
        apply_to_doc(&mut doc, &Config::default());
        assert!(!doc.to_string().contains("[stats]"));
    }
}
