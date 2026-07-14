pub mod claude;
pub mod codex;
pub mod minimax;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Window {
    /// Short label shown in the UI: "5h", "semana", "semana · Fable"…
    pub label: String,
    pub used_percent: f64,
    /// Unix seconds; None when the provider doesn't report a reset time.
    pub resets_at: Option<i64>,
    pub active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderStatus {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub plan: Option<String>,
    pub windows: Vec<Window>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_credits_available: Option<u64>,
    /// Unix secs de cuándo se obtuvieron estos datos, si son de una consulta
    /// anterior conservada porque la fresca falló (p. ej. 429).
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
            windows: vec![],
            reset_credits_available: None,
            stale_since: None,
            error: Some(format!("{e:#}")),
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

/// Cuánto tiempo se siguen mostrando los datos de la última consulta buena
/// cuando la fresca falla (429 puntual, corte de red…). Pasado el plazo, el
/// error se muestra tal cual.
const STALE_GRACE_SECS: i64 = 30 * 60;

/// Sustituye los providers en error por sus datos de la consulta anterior si
/// aún están dentro del periodo de gracia, marcándolos con `stale_since`.
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
        // Si lo guardado ya era stale, la edad cuenta desde la consulta buena
        // original, no desde la última vez que se re-guardó.
        let data_from = old.stale_since.unwrap_or(previous.fetched_at);
        if now - data_from <= STALE_GRACE_SECS {
            let mut kept = old.clone();
            kept.stale_since = Some(data_from);
            *provider = kept;
        }
    }
}

pub fn collect_all() -> Status {
    let config = crate::config::get();
    let mut providers = Vec::new();

    if config.providers.claude && claude::available() {
        providers.push(
            claude::collect()
                .unwrap_or_else(|e| ProviderStatus::err("claude", "Claude Code", claude::ICON, e)),
        );
    }
    if config.providers.codex && codex::available() {
        providers.push(
            codex::collect()
                .unwrap_or_else(|e| ProviderStatus::err("codex", "Codex", codex::ICON, e)),
        );
    }
    if config.providers.minimax && minimax::available() {
        providers.push(
            minimax::collect()
                .unwrap_or_else(|e| ProviderStatus::err("minimax", "MiniMax", minimax::ICON, e)),
        );
    }

    if providers.iter().any(|p| p.error.is_some()) {
        if let Some(previous) = crate::cache::load_stale() {
            keep_stale_data(&mut providers, &previous, chrono::Utc::now().timestamp());
        }
    }

    for provider in &mut providers {
        let icon = match provider.id.as_str() {
            "claude" => config.icons.claude.as_ref(),
            "codex" => config.icons.codex.as_ref(),
            "minimax" => config.icons.minimax.as_ref(),
            _ => None,
        };
        if let Some(icon) = icon {
            provider.icon = icon.clone();
        }
    }

    Status {
        fetched_at: chrono::Utc::now().timestamp(),
        providers,
    }
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
        // el previo ya era stale: su edad viene de stale_since, no de fetched_at
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
    fn serializes_numeric_reset_credits_without_changing_historical_fields() {
        for credits in [Some(3), Some(0)] {
            let provider = ProviderStatus {
                id: "codex".into(),
                name: "Codex".into(),
                icon: "⬡".into(),
                plan: Some("pro".into()),
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
        }
    }
}
