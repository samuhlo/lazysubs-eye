pub mod claude;
pub mod codex;
pub mod minimax;

use serde::{Deserialize, Serialize};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const REFRESH_GLOBAL_BUDGET: Duration = Duration::from_secs(8);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Window {
    /// Compact UI label, such as `5h`, `semana`, or `semana · Fable`.
    pub label: String,
    pub used_percent: f64,
    /// Reset timestamp in Unix seconds; `None` when the provider omits it.
    pub resets_at: Option<i64>,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub plan: Option<String>,
    /// Account identity (email or alias). Omit `None` during serialization to
    /// preserve the stable JSON contract, like `stale_since`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub windows: Vec<Window>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_credits_available: Option<u64>,
    /// Unix seconds when these data were fetched, if a prior result was retained
    /// because the fresh request failed (for example, HTTP 429).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_since: Option<i64>,
    pub error: Option<String>,
}

impl ProviderStatus {
    pub fn err(id: &str, name: &str, icon: &str, e: anyhow::Error) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: icon.into(),
            plan: None,
            account: None,
            windows: vec![],
            reset_credits_available: None,
            stale_since: None,
            error: Some(crate::diagnostics::sanitize_error(format!("{e:#}"))),
        }
    }

    /// Most urgent window: highest used_percent, preferring active ones.
    pub fn worst(&self) -> Option<&Window> {
        self.windows.iter().max_by(|a, b| {
            (a.active, a.used_percent)
                .partial_cmp(&(b.active, b.used_percent))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Status {
    pub fetched_at: i64,
    pub providers: Vec<ProviderStatus>,
}

/// [CACHE] How long the last successful data remain visible after a fresh
/// failure (transient 429 or network loss). After this grace period, show the error.
const STALE_GRACE_SECS: i64 = 30 * 60;

/// Replace failed providers with prior successful data inside the grace period,
/// marking the preserved result with `stale_since`.
fn keep_stale_data(providers: &mut [ProviderStatus], previous: &Status, now: i64) {
    for provider in providers {
        if provider.error.is_none() {
            continue;
        }
        let Some(old) = previous
            .providers
            .iter()
            .find(|o| o.id == provider.id && o.error.is_none() && !o.windows.is_empty())
        else {
            continue;
        };
        // If cached data are already stale, age begins at the original good fetch,
        // not the last time stale data were saved.
        let data_from = old.stale_since.unwrap_or(previous.fetched_at);
        if now - data_from <= STALE_GRACE_SECS {
            let mut kept = old.clone();
            kept.stale_since = Some(data_from);
            *provider = kept;
        }
    }
}

/// Providers visible on one surface (waybar or TUI), in requested order.
/// `None` selects collection order; unknown IDs are ignored.
pub fn select<'a>(
    providers: &'a [ProviderStatus],
    selection: &Option<Vec<String>>,
) -> Vec<&'a ProviderStatus> {
    match selection {
        None => providers.iter().collect(),
        Some(ids) => ids
            .iter()
            .filter_map(|id| providers.iter().find(|p| &p.id == id))
            .collect(),
    }
}

/// Resolved account ready for collection: composed ID, name, and optional icon.
#[derive(Debug, Clone, PartialEq)]
struct AccountSpec {
    id: String,
    name: String,
    icon_override: Option<String>,
}

/// Compose account ID and name. The first or only account keeps the plain ID
/// (`claude`) to preserve ID-based surfaces and notifications; later accounts
/// use `base:alias`.
fn compose(
    base_id: &str,
    base_name: &str,
    len: usize,
    index: usize,
    alias: &str,
) -> (String, String) {
    if len <= 1 {
        (base_id.to_string(), base_name.to_string())
    } else if index == 0 {
        (base_id.to_string(), format!("{base_name} · {alias}"))
    } else {
        (
            format!("{base_id}:{alias}"),
            format!("{base_name} · {alias}"),
        )
    }
}

fn apply_spec(
    ps: &mut ProviderStatus,
    spec: &AccountSpec,
    base: &str,
    config: &crate::config::Config,
) {
    ps.id = spec.id.clone();
    ps.name = spec.name.clone();
    let icon = spec.icon_override.as_ref().or(match base {
        "claude" => config.icons.claude.as_ref(),
        "codex" => config.icons.codex.as_ref(),
        "minimax" => config.icons.minimax.as_ref(),
        _ => None,
    });
    if let Some(icon) = icon {
        ps.icon = icon.clone();
    }
}

fn claude_targets(config: &crate::config::Config) -> Vec<(AccountSpec, std::path::PathBuf)> {
    let accounts = &config.accounts.claude;
    if accounts.is_empty() {
        let (id, name) = compose("claude", "Claude Code", 1, 0, "");
        return vec![(
            AccountSpec {
                id,
                name,
                icon_override: None,
            },
            claude::default_creds_path(),
        )];
    }
    let len = accounts.len();
    accounts
        .iter()
        .enumerate()
        .map(|(i, acc)| {
            let (id, name) = compose("claude", "Claude Code", len, i, &acc.name);
            let creds = acc
                .credentials
                .as_deref()
                .map(crate::config::expand_tilde)
                .unwrap_or_else(claude::default_creds_path);
            (
                AccountSpec {
                    id,
                    name,
                    icon_override: acc.icon.clone(),
                },
                creds,
            )
        })
        .collect()
}

fn codex_targets(config: &crate::config::Config) -> Vec<(AccountSpec, Option<std::path::PathBuf>)> {
    let accounts = &config.accounts.codex;
    if accounts.is_empty() {
        let (id, name) = compose("codex", "Codex", 1, 0, "");
        return vec![(
            AccountSpec {
                id,
                name,
                icon_override: None,
            },
            None,
        )];
    }
    let len = accounts.len();
    accounts
        .iter()
        .enumerate()
        .map(|(i, acc)| {
            let (id, name) = compose("codex", "Codex", len, i, &acc.name);
            let home = acc.codex_home.as_deref().map(crate::config::expand_tilde);
            (
                AccountSpec {
                    id,
                    name,
                    icon_override: acc.icon.clone(),
                },
                home,
            )
        })
        .collect()
}

fn minimax_targets(config: &crate::config::Config) -> Vec<(AccountSpec, String, Option<String>)> {
    let accounts = &config.accounts.minimax;
    if accounts.is_empty() {
        // Single-account shorthand: `[minimax] api_key` / `MINIMAX_API_KEY`.
        return match minimax::primary_api_key() {
            Some(key) => {
                let (id, name) = compose("minimax", "MiniMax", 1, 0, "");
                vec![(
                    AccountSpec {
                        id,
                        name,
                        icon_override: None,
                    },
                    key,
                    config.minimax.base_url.clone(),
                )]
            }
            None => vec![],
        };
    }
    let len = accounts.len();
    accounts
        .iter()
        .enumerate()
        .map(|(i, acc)| {
            let (id, name) = compose("minimax", "MiniMax", len, i, &acc.name);
            (
                AccountSpec {
                    id,
                    name,
                    icon_override: acc.icon.clone(),
                },
                acc.api_key.clone().unwrap_or_default(),
                acc.base_url.clone(),
            )
        })
        .collect()
}

/// Configured account IDs and short names for TUI settings rows, independent
/// of filesystem availability. Honors `[providers]` toggles.
pub fn configured_providers() -> Vec<(String, String)> {
    let config = crate::config::get();
    let mut out = Vec::new();
    if config.providers.claude {
        out.extend(
            claude_targets(&config)
                .into_iter()
                .map(|(s, _)| (s.id, s.name)),
        );
    }
    if config.providers.codex {
        out.extend(
            codex_targets(&config)
                .into_iter()
                .map(|(s, _)| (s.id, s.name)),
        );
    }
    if config.providers.minimax {
        out.extend(
            minimax_targets(&config)
                .into_iter()
                .map(|(s, _, _)| (s.id, s.name)),
        );
    }
    out
}

// [FLOW] Collect provider families concurrently under one refresh budget.
// Results are restored to family order after arrival; timed-out families reuse
// stale cache when possible, otherwise surface an explicit provider error.
pub fn collect_all() -> Status {
    let config = crate::config::get();
    let claude_config = config.clone();
    let codex_config = config.clone();
    let minimax_config = config.clone();
    let (results_tx, results_rx) = mpsc::channel();
    let claude_tx = results_tx.clone();
    std::thread::spawn(move || {
        crate::diagnostics::verbose("collector iniciado: claude");
        let mut out = Vec::new();
        if claude_config.providers.claude {
            for (spec, creds) in claude_targets(&claude_config) {
                if !claude::available_at(&creds) {
                    continue;
                }
                let mut ps = claude::collect(&creds)
                    .unwrap_or_else(|e| ProviderStatus::err(&spec.id, &spec.name, claude::ICON, e));
                apply_spec(&mut ps, &spec, "claude", &claude_config);
                out.push(ps);
            }
        }
        let _ = claude_tx.send((0, out));
    });
    let codex_tx = results_tx.clone();
    std::thread::spawn(move || {
        crate::diagnostics::verbose("collector iniciado: codex");
        let mut out = Vec::new();
        if codex_config.providers.codex {
            for (spec, home) in codex_targets(&codex_config) {
                if !codex::available_at(home.as_deref()) {
                    continue;
                }
                let mut ps = codex::collect(home.as_deref())
                    .unwrap_or_else(|e| ProviderStatus::err(&spec.id, &spec.name, codex::ICON, e));
                apply_spec(&mut ps, &spec, "codex", &codex_config);
                out.push(ps);
            }
        }
        let _ = codex_tx.send((1, out));
    });
    std::thread::spawn(move || {
        crate::diagnostics::verbose("collector iniciado: minimax");
        let mut out = Vec::new();
        if minimax_config.providers.minimax {
            for (spec, key, base_url) in minimax_targets(&minimax_config) {
                let mut ps = minimax::collect(&key, base_url.as_deref()).unwrap_or_else(|e| {
                    ProviderStatus::err(&spec.id, &spec.name, minimax::ICON, e)
                });
                apply_spec(&mut ps, &spec, "minimax", &minimax_config);
                out.push(ps);
            }
        }
        let _ = results_tx.send((2, out));
    });
    let (mut batches, timed_out) = collect_until_budget(results_rx, 3, REFRESH_GLOBAL_BUDGET);
    batches.sort_by_key(|(family, _)| *family);
    let completed: std::collections::BTreeSet<usize> =
        batches.iter().map(|(family, _)| *family).collect();
    let mut providers: Vec<ProviderStatus> = batches
        .into_iter()
        .flat_map(|(_, providers)| providers)
        .collect();
    if timed_out {
        crate::diagnostics::verbose("presupuesto global de collectors agotado");
        if let Some(previous) = crate::cache::load_stale() {
            for mut provider in previous.providers {
                let family = match provider.id.split(':').next().unwrap_or_default() {
                    "claude" => 0,
                    "codex" => 1,
                    "minimax" => 2,
                    _ => continue,
                };
                if !completed.contains(&family) {
                    provider.stale_since =
                        Some(provider.stale_since.unwrap_or(previous.fetched_at));
                    providers.push(provider);
                }
            }
        }
        for (family, enabled, id, name, icon) in [
            (
                0,
                config.providers.claude,
                "claude",
                "Claude Code",
                claude::ICON,
            ),
            (1, config.providers.codex, "codex", "Codex", codex::ICON),
            (
                2,
                config.providers.minimax,
                "minimax",
                "MiniMax",
                minimax::ICON,
            ),
        ] {
            if enabled
                && !completed.contains(&family)
                && !providers
                    .iter()
                    .any(|provider| provider.id.split(':').next() == Some(id))
            {
                providers.push(ProviderStatus::err(
                    id,
                    name,
                    icon,
                    anyhow::anyhow!("timeout del collector; reintenta el refresh"),
                ));
            }
        }
    }

    if providers.iter().any(|p| p.error.is_some()) {
        if let Some(previous) = crate::cache::load_stale() {
            keep_stale_data(&mut providers, &previous, chrono::Utc::now().timestamp());
        }
    }

    Status {
        fetched_at: chrono::Utc::now().timestamp(),
        providers,
    }
}

// [FLOW] Receive only until the shared deadline. Workers may finish later, but
// their late results cannot delay this refresh or reorder accepted batches.
fn collect_until_budget<T>(
    receiver: mpsc::Receiver<(usize, T)>,
    expected: usize,
    budget: Duration,
) -> (Vec<(usize, T)>, bool) {
    let deadline = Instant::now() + budget;
    let mut results = Vec::with_capacity(expected);
    while results.len() < expected {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(result) => results.push(result),
            Err(_) => break,
        }
    }
    let timed_out = results.len() < expected;
    (results, timed_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(id: &str, percent: f64, error: Option<&str>) -> ProviderStatus {
        ProviderStatus {
            id: id.into(),
            name: id.into(),
            icon: "x".into(),
            plan: Some("pro".into()),
            account: None,
            windows: if error.is_some() {
                vec![]
            } else {
                vec![Window {
                    label: "5h".into(),
                    used_percent: percent,
                    resets_at: Some(1000),
                    active: true,
                }]
            },
            reset_credits_available: None,
            stale_since: None,
            error: error.map(str::to_owned),
        }
    }

    #[test]
    fn compose_conserva_id_simple_para_la_primera_o_unica() {
        assert_eq!(
            compose("claude", "Claude Code", 1, 0, "x"),
            ("claude".to_string(), "Claude Code".to_string())
        );
        assert_eq!(
            compose("claude", "Claude Code", 2, 0, "personal"),
            ("claude".to_string(), "Claude Code · personal".to_string())
        );
        assert_eq!(
            compose("claude", "Claude Code", 2, 1, "trabajo"),
            (
                "claude:trabajo".to_string(),
                "Claude Code · trabajo".to_string()
            )
        );
    }

    #[test]
    fn claude_targets_default_y_multicuenta() {
        use crate::config::{ClaudeAccount, Config};

        // No accounts -> one account, plain ID, default credentials.
        let default = Config::default();
        let targets = claude_targets(&default);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0.id, "claude");
        assert_eq!(targets[0].1, claude::default_creds_path());

        // Two accounts -> `claude` and `claude:trabajo`, with expanded credentials.
        let mut config = Config::default();
        config.accounts.claude = vec![
            ClaudeAccount {
                name: "personal".into(),
                credentials: None,
                icon: None,
            },
            ClaudeAccount {
                name: "trabajo".into(),
                credentials: Some("/tmp/w/.credentials.json".into()),
                icon: Some("❄".into()),
            },
        ];
        let targets = claude_targets(&config);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].0.id, "claude");
        assert_eq!(targets[0].1, claude::default_creds_path());
        assert_eq!(targets[1].0.id, "claude:trabajo");
        assert_eq!(targets[1].0.name, "Claude Code · trabajo");
        assert_eq!(targets[1].0.icon_override.as_deref(), Some("❄"));
        assert_eq!(
            targets[1].1,
            std::path::PathBuf::from("/tmp/w/.credentials.json")
        );
    }

    #[test]
    fn minimax_targets_vacio_sin_key() {
        use crate::config::Config;
        let mut config = Config::default();
        // No API key or accounts -> no targets; the environment may provide `MINIMAX_API_KEY`.
        std::env::remove_var("MINIMAX_API_KEY");
        config.minimax.api_key = None;
        assert!(minimax_targets(&config).is_empty());
    }

    #[test]
    fn select_filtra_ordena_e_ignora_desconocidos() {
        let all = vec![
            provider("claude", 10.0, None),
            provider("codex", 20.0, None),
            provider("minimax", 30.0, None),
        ];
        let ids = |sel: &Option<Vec<String>>| {
            select(&all, sel)
                .iter()
                .map(|p| p.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&None), ["claude", "codex", "minimax"]);
        assert_eq!(
            ids(&Some(vec!["minimax".into(), "claude".into()])),
            ["minimax", "claude"]
        );
        assert_eq!(ids(&Some(vec!["gemini".into()])), Vec::<String>::new());
    }

    #[test]
    fn un_error_conserva_los_datos_previos_dentro_de_la_gracia() {
        let previous = Status {
            fetched_at: 10_000,
            providers: vec![provider("claude", 42.0, None)],
        };
        let mut fresh = vec![provider("claude", 0.0, Some("429"))];
        keep_stale_data(&mut fresh, &previous, 10_000 + 60);
        assert!(fresh[0].error.is_none());
        assert_eq!(fresh[0].windows[0].used_percent, 42.0);
        assert_eq!(fresh[0].stale_since, Some(10_000));
    }

    #[test]
    fn pasada_la_gracia_se_muestra_el_error() {
        let previous = Status {
            fetched_at: 10_000,
            providers: vec![provider("claude", 42.0, None)],
        };
        let mut fresh = vec![provider("claude", 0.0, Some("429"))];
        keep_stale_data(&mut fresh, &previous, 10_000 + STALE_GRACE_SECS + 1);
        assert!(fresh[0].error.is_some());
    }

    #[test]
    fn la_gracia_cuenta_desde_la_consulta_buena_original() {
        // Already-stale data age from `stale_since`, not `fetched_at`.
        let mut old = provider("claude", 42.0, None);
        old.stale_since = Some(5_000);
        let previous = Status {
            fetched_at: 10_000,
            providers: vec![old],
        };
        let mut fresh = vec![provider("claude", 0.0, Some("429"))];
        keep_stale_data(&mut fresh, &previous, 5_000 + STALE_GRACE_SECS + 1);
        assert!(fresh[0].error.is_some(), "no debe encadenar la gracia");

        let mut fresh = vec![provider("claude", 0.0, Some("429"))];
        keep_stale_data(&mut fresh, &previous, 5_000 + 60);
        assert_eq!(fresh[0].stale_since, Some(5_000));
    }

    #[test]
    fn sin_error_no_se_toca_nada_y_un_previo_en_error_no_sirve() {
        let previous = Status {
            fetched_at: 10_000,
            providers: vec![provider("claude", 42.0, Some("caído"))],
        };
        let mut fresh = vec![provider("claude", 7.0, None)];
        keep_stale_data(&mut fresh, &previous, 10_000);
        assert_eq!(fresh[0].windows[0].used_percent, 7.0);
        assert!(fresh[0].stale_since.is_none());

        let mut errored = vec![provider("claude", 0.0, Some("429"))];
        keep_stale_data(&mut errored, &previous, 10_000);
        assert!(errored[0].error.is_some(), "un previo en error no rescata");
    }

    #[test]
    fn reads_old_cache_and_omits_absent_reset_credits() {
        let old_cache = json!({
            "fetched_at": 1,
            "providers": [{
                "id": "codex",
                "name": "Codex",
                "icon": "⬡",
                "plan": "pro",
                "windows": [],
                "error": null,
            }],
        });
        let status: Status = serde_json::from_value(old_cache).unwrap();
        let provider = &status.providers[0];
        assert_eq!(provider.id, "codex");
        assert_eq!(provider.plan.as_deref(), Some("pro"));
        assert!(provider.reset_credits_available.is_none());

        let serialized = serde_json::to_value(provider).unwrap();
        assert!(serialized.get("reset_credits_available").is_none());
    }

    #[test]
    fn error_status_has_no_reset_credits() {
        let status = ProviderStatus::err("codex", "Codex", "⬡", anyhow::anyhow!("falló"));
        assert!(status.error.is_some());
        assert_eq!(status.reset_credits_available, None);
    }

    #[test]
    fn provider_errors_redact_credentials() {
        let message =
            crate::diagnostics::sanitize_error("request failed api_key=supersecret token=other");
        assert!(!message.contains("supersecret"));
        assert!(!message.contains("other"));
        assert!(message.contains("[REDACTED]"));
    }

    #[test]
    fn parallel_budget_retorna_rapidos_y_cancela_pendientes() {
        let (tx, rx) = mpsc::channel();
        let fast = tx.clone();
        std::thread::spawn(move || fast.send((0, "rápido")).unwrap());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let _ = tx.send((1, "lento"));
        });
        let (results, timed_out) = collect_until_budget(rx, 2, Duration::from_millis(10));
        assert!(timed_out);
        assert_eq!(results, vec![(0, "rápido")]);
    }

    #[test]
    fn parallel_budget_con_todos_rapidos_conserva_orden_reordenable() {
        let (tx, rx) = mpsc::channel();
        tx.send((2, "c")).unwrap();
        tx.send((0, "a")).unwrap();
        let (mut results, timed_out) = collect_until_budget(rx, 2, Duration::from_secs(1));
        results.sort_by_key(|(index, _)| *index);
        assert!(!timed_out);
        assert_eq!(results, vec![(0, "a"), (2, "c")]);
    }

    #[test]
    fn serializes_numeric_reset_credits_without_changing_historical_fields() {
        for credits in [Some(3), Some(0)] {
            let provider = ProviderStatus {
                id: "codex".into(),
                name: "Codex".into(),
                icon: "⬡".into(),
                plan: Some("pro".into()),
                account: None,
                windows: vec![],
                reset_credits_available: credits,
                stale_since: None,
                error: None,
            };
            let value = serde_json::to_value(provider).unwrap();
            assert_eq!(value["id"], "codex");
            assert_eq!(value["plan"], "pro");
            assert_eq!(value["windows"], json!([]));
            assert_eq!(value["reset_credits_available"], json!(credits));
            assert!(
                value.get("account").is_none(),
                "account None no se serializa"
            );
        }
    }
}
