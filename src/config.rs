//! [CORE] OPTIONAL USER CONFIGURATION
//!
//! Every field defaults to the behavior without a config file. Invalid input is
//! recorded as an actionable error: diagnostic modes return exit 3, while other
//! modes fail closed to defaults.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::RwLock;
use toml_edit::{value, DocumentMut};

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Cache validity in seconds; the `--ttl` flag takes precedence.
    pub ttl: i64,
    /// Warning threshold percentage for CSS class, gauge color, and alerts.
    pub warning_at: f64,
    /// Critical threshold percentage.
    pub critical_at: f64,
    /// Send desktop notifications through `notify-send` on threshold crossings.
    pub notifications: bool,
    /// Minimum seconds before repeating an alert for one window after rolling
    /// resets or recrossing. Escalation to a higher level bypasses the wait.
    /// WHY: the high default prevents notification floods.
    pub notification_cooldown: i64,
    /// Threshold colors for Waybar classes and TUI gauges. When false, usage
    /// stays neutral; the `error` class remains because it signals failure, not use.
    pub colors: bool,
    /// Show the account email or alias beside the plan in the TUI and tooltip.
    pub show_account: bool,
    pub providers: Providers,
    pub icons: Icons,
    pub minimax: MiniMax,
    pub waybar: Waybar,
    pub tui: Tui,
    pub stats: Stats,
    pub accounts: Accounts,
}

/// [DATA] MULTI-ACCOUNT PROVIDERS
///
/// Empty means one provider-detected account, preserving the original behavior.
/// Each configured entry instead produces its own `ProviderStatus`; see
/// `providers::collect_all`.
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
    /// Visible alias, included in the `claude:<name>` ID and display name.
    pub name: String,
    /// Path to `.credentials.json`; defaults to `~/.claude/.credentials.json`.
    pub credentials: Option<String>,
    pub icon: Option<String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CodexAccount {
    pub name: String,
    /// Directory passed as `CODEX_HOME` to app-server; defaults to `~/.codex`.
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

/// Expands a leading `~` through `$HOME`; paths without it pass through unchanged.
pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

/// Token-spend history in SQLite under XDG_STATE_HOME and its statistics.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Stats {
    /// When false, do not open the database or render period panels.
    pub enabled: bool,
    /// Initial token-panel period: "hoy" | "semana" | "mes".
    pub default_period: String,
    /// Retention in days; 0 means unlimited.
    pub history_days: i64,
    /// Render the daily-total sparkline below each panel.
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

/// Controls what the Waybar module renders.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Waybar {
    /// Visible providers in the bar **and their order** (IDs: "claude", "codex",
    /// "minimax"). Absent means all in collection order.
    pub providers: Option<Vec<String>>,
    /// When false, the bar displays icons without percentages.
    pub percent: Option<bool>,
    /// Which window the bar shows per provider by label (ID → label, for example
    /// `{ claude = "semana" }`). Matching tries exact then substring
    /// (`"Fable"` → `"semana · Fable"`). No selection or match uses the most
    /// urgent window.
    pub window: Option<std::collections::BTreeMap<String, String>>,
}

impl Waybar {
    pub fn percent(&self) -> bool {
        self.percent.unwrap_or(true)
    }
}

/// Controls what the TUI renders.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Tui {
    /// Visible providers in the TUI and their order. Absent means all.
    pub providers: Option<Vec<String>>,
    /// Daily token panels: "claude_tokens", "pi_tokens", and
    /// "opencode_tokens". Absent means all.
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

/// MiniMax credentials: the plan can only be queried with the token-plan
/// Subscription Key from config or `MINIMAX_API_KEY`.
#[derive(Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct MiniMax {
    pub api_key: Option<String>,
    /// Alternate host, for example https://api.minimaxi.com for China.
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

pub fn config_file() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|base| base.join("lazysubs-eye/config.toml"))
}

fn load() -> Config {
    let Some(path) = config_file() else {
        return Config::default();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Config::default(),
        Err(_) => {
            set_load_errors(vec![
                "E006: no se puede leer la configuración; comprueba sus permisos".into(),
            ]);
            return Config::default();
        }
    };
    let config: Config = match toml::from_str(&text) {
        Ok(config) => config,
        Err(_) => {
            set_load_errors(vec![
                "E001: la configuración TOML no se puede interpretar".into()
            ]);
            return Config::default();
        }
    };
    let errors = validate(&config);
    if errors.is_empty() {
        config
    } else {
        set_load_errors(
            errors
                .iter()
                .map(|error| format!("E002: {error}"))
                .collect(),
        );
        Config::default()
    }
}

static CONFIG: RwLock<Option<Config>> = RwLock::new(None);
static CONFIG_LOAD_ERRORS: RwLock<Vec<String>> = RwLock::new(Vec::new());

fn set_load_errors(errors: Vec<String>) {
    for error in &errors {
        eprintln!(
            "lazysubs-eye: {}",
            crate::diagnostics::sanitize_error(error)
        );
    }
    *CONFIG_LOAD_ERRORS.write().unwrap() = errors;
}

pub fn load_errors() -> Vec<String> {
    CONFIG_LOAD_ERRORS.read().unwrap().clone()
}

/// [CORE] PROCESS CONFIG CACHE
///
/// Loads the file once and allows live replacement from the TUI settings panel.
/// Tests always receive defaults so user configuration cannot leak into results.
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

/// [CORE] SEMANTIC VALIDATION
///
/// Kept separate from parsing so `doctor` gets short actionable messages with
/// no paths or sensitive values.
pub fn validate(config: &Config) -> Vec<String> {
    let mut errors = Vec::new();
    if config.ttl <= 0 {
        errors.push("ttl debe ser mayor que cero".into());
    }
    if !(0.0..=100.0).contains(&config.warning_at) {
        errors.push("warning_at debe estar entre 0 y 100".into());
    }
    if !(0.0..=100.0).contains(&config.critical_at) {
        errors.push("critical_at debe estar entre 0 y 100".into());
    }
    if config.warning_at >= config.critical_at {
        errors.push("warning_at debe ser menor que critical_at".into());
    }
    if config.stats.history_days < 0 {
        errors.push("history_days no puede ser negativo (0 significa sin límite)".into());
    }
    for base_url in std::iter::once(config.minimax.base_url.as_deref())
        .chain(
            config
                .accounts
                .minimax
                .iter()
                .map(|account| account.base_url.as_deref()),
        )
        .flatten()
    {
        if !valid_http_url(base_url) {
            errors.push("base_url debe ser una URL http(s) válida".into());
        }
    }
    errors
}

fn valid_http_url(value: &str) -> bool {
    let Some(rest) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !host.is_empty() && !host.chars().any(char::is_whitespace)
}

/// Replaces the in-memory configuration from the TUI settings panel.
pub fn set(config: Config) {
    *CONFIG.write().unwrap() = Some(config);
}

/// [DATA] SURFACE VISIBILITY TOGGLE
///
/// `None` means "all", so the first toggle materializes the full list before
/// adding or removing one ID.
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

/// [DATA] NON-DESTRUCTIVE TOML PERSISTENCE
///
/// Updates TUI-editable fields while preserving unmanaged keys and user comments,
/// such as `[minimax] api_key`. Only non-default or already-present keys are written.
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

/// Applies TUI-editable fields to TOML separately from `persist()` so this
/// transformation can be tested without filesystem I/O.
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
    set_window_map(doc, &config.waybar.window);
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

/// Persists `[waybar.window]` (ID → label) as an explicit subtable, or removes
/// it when no selection exists.
fn set_window_map(doc: &mut DocumentMut, map: &Option<std::collections::BTreeMap<String, String>>) {
    match map {
        Some(map) if !map.is_empty() => {
            let waybar = ensure_table(doc, "waybar");
            if !waybar.contains_key("window") || waybar["window"].as_table().is_none() {
                waybar["window"] = toml_edit::Item::Table(toml_edit::Table::new());
            }
            let window = waybar["window"].as_table_mut().expect("recién insertada");
            let stale: Vec<String> = window
                .iter()
                .map(|(k, _)| k.to_string())
                .filter(|k| !map.contains_key(k))
                .collect();
            for key in stale {
                window.remove(&key);
            }
            for (id, label) in map {
                window[id] = value(label.as_str());
            }
        }
        _ => {
            if let Some(waybar) = doc.get_mut("waybar").and_then(|i| i.as_table_mut()) {
                waybar.remove("window");
            }
        }
    }
}

/// [DATA] EXPLICIT TOML TABLE
///
/// Uses a trailing `[name]` table, never an inline table: `toml_edit` would put
/// `name = { … }` before the user's leading comments.
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
    fn validation_rechaza_umbral_y_ttl_incoherentes() {
        let config = Config {
            ttl: 0,
            warning_at: 95.0,
            critical_at: 80.0,
            stats: Stats {
                history_days: -1,
                ..Stats::default()
            },
            ..Config::default()
        };
        let errors = validate(&config);
        assert!(errors.iter().any(|e| e.contains("ttl")));
        assert!(errors.iter().any(|e| e.contains("warning_at")));
        assert!(errors.iter().any(|e| e.contains("history_days")));
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
        assert!(config.colors); // Default when the field is absent.
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
        // Fields still at defaults and absent from the input remain absent.
        assert!(!out.contains("warning_at"));
        assert!(!out.contains("[providers]"));

        // The resulting document remains loadable.
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

        // No table -> empty, preserving the original behavior.
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
    fn apply_to_doc_persiste_waybar_window() {
        let mut doc: DocumentMut = "# cfg\n".parse().unwrap();
        let mut map = std::collections::BTreeMap::new();
        map.insert("claude".to_string(), "semana".to_string());
        let config = Config {
            waybar: Waybar {
                window: Some(map),
                ..Waybar::default()
            },
            ..Config::default()
        };
        apply_to_doc(&mut doc, &config);
        let out = doc.to_string();
        assert!(out.contains("[waybar.window]"), "subtabla explícita: {out}");
        assert!(out.contains("claude = \"semana\""));
        let reloaded: Config = toml::from_str(&out).unwrap();
        assert_eq!(
            reloaded
                .waybar
                .window
                .as_ref()
                .and_then(|m| m.get("claude"))
                .map(String::as_str),
            Some("semana")
        );

        // Returning to "auto" (an empty map) removes the subtable.
        let cleared = Config {
            waybar: Waybar::default(),
            ..Config::default()
        };
        apply_to_doc(&mut doc, &cleared);
        assert!(!doc.to_string().contains("[waybar.window]"));
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

        // No [stats] table -> defaults.
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

        // Default stats without a prior table produce no output.
        let mut doc: DocumentMut = "# cfg\n".parse().unwrap();
        apply_to_doc(&mut doc, &Config::default());
        assert!(!doc.to_string().contains("[stats]"));
    }
}
