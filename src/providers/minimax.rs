//! [API] MiniMax coding/token-plan collector.
//!
//! Calls `GET {base_url}/v1/token_plan/remains` with `Authorization: Bearer <key>`.
//! The key is the plan **Subscription Key**, not a normal platform API key; it
//! comes from `[minimax] api_key` in `config.toml` or `MINIMAX_API_KEY`.
//!
//! Response contract (verified against the live API; do not infer it):
//! - `model_remains[]` has one entry per plan model (`general` = LLM, `video`,
//!   ...), each with interval (for example 5h) and weekly windows.
//! - `start_time`, `end_time`, and `weekly_end_time` are Unix **milliseconds**.
//! - `*_remaining_percent` is remaining quota, not used quota, so invert it.
//! - `*_status == 3` means the window is outside the subscribed plan: omit it.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use super::{ProviderStatus, Window};

pub const ICON: &str = "◆";
const DEFAULT_BASE_URL: &str = "https://api.minimax.io";
const STATUS_NOT_IN_PLAN: i64 = 3;

/// Primary-account key from `[minimax] api_key` or `MINIMAX_API_KEY`.
/// Multi-account configuration passes each key directly to `collect`.
pub fn primary_api_key() -> Option<String> {
    crate::config::get()
        .minimax
        .api_key
        .clone()
        .or_else(|| std::env::var("MINIMAX_API_KEY").ok())
        .filter(|k| !k.trim().is_empty())
}

#[derive(Deserialize)]
struct RemainsResponse {
    #[serde(default)]
    model_remains: Vec<ModelRemains>,
    base_resp: Option<BaseResp>,
}

#[derive(Deserialize)]
struct BaseResp {
    status_code: i64,
    status_msg: Option<String>,
}

#[derive(Deserialize)]
struct ModelRemains {
    model_name: String,
    start_time: Option<i64>,
    end_time: Option<i64>,
    current_interval_status: Option<i64>,
    current_interval_remaining_percent: Option<f64>,
    weekly_end_time: Option<i64>,
    current_weekly_status: Option<i64>,
    current_weekly_remaining_percent: Option<f64>,
}

fn interval_label(start_ms: Option<i64>, end_ms: Option<i64>) -> String {
    match (start_ms, end_ms) {
        (Some(start), Some(end)) if end > start => {
            let mins = (end - start) / 60_000;
            match mins {
                10080 => "semana".into(),
                m if m % 60 == 0 => format!("{}h", m / 60),
                m => format!("{m}m"),
            }
        }
        _ => "ventana".into(),
    }
}

/// The coding-plan `general` model has no prefix; other plan models (such as
/// video) include their name so equal-duration windows remain distinguishable.
fn window_label(model: &str, base: &str) -> String {
    if model == "general" {
        base.to_string()
    } else {
        format!("{base} · {model}")
    }
}

fn windows_from(remains: &[ModelRemains]) -> Vec<Window> {
    let mut windows = Vec::new();
    for model in remains {
        if model.current_interval_status != Some(STATUS_NOT_IN_PLAN) {
            if let Some(remaining) = model.current_interval_remaining_percent {
                windows.push(Window {
                    label: window_label(
                        &model.model_name,
                        &interval_label(model.start_time, model.end_time),
                    ),
                    used_percent: (100.0 - remaining).clamp(0.0, 100.0),
                    resets_at: model.end_time.map(|ms| ms / 1000),
                    active: true,
                });
            }
        }
        if model.current_weekly_status != Some(STATUS_NOT_IN_PLAN) {
            if let Some(remaining) = model.current_weekly_remaining_percent {
                windows.push(Window {
                    label: window_label(&model.model_name, "semana"),
                    used_percent: (100.0 - remaining).clamp(0.0, 100.0),
                    resets_at: model.weekly_end_time.map(|ms| ms / 1000),
                    active: true,
                });
            }
        }
    }
    windows
}

pub fn collect(key: &str, base_url: Option<&str>) -> Result<ProviderStatus> {
    if key.trim().is_empty() {
        bail!("falta la api_key de MiniMax");
    }
    let base_url = base_url.unwrap_or(DEFAULT_BASE_URL);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout_read(std::time::Duration::from_secs(5))
        .timeout_write(std::time::Duration::from_secs(3))
        .build();
    let resp = agent
        .get(&format!("{base_url}/v1/token_plan/remains"))
        .set("Authorization", &format!("Bearer {key}"))
        .set("Content-Type", "application/json")
        .call();

    let remains: RemainsResponse = match resp {
        Ok(r) => r.into_json().context("parseando respuesta de remains")?,
        Err(ureq::Error::Status(401 | 403, _)) => {
            bail!("key rechazada — revisa [minimax] api_key (debe ser la Subscription Key)")
        }
        Err(ureq::Error::Status(code, _)) => bail!("API respondió {code}"),
        Err(e) => return Err(e).context("llamando al endpoint de remains"),
    };

    if let Some(base) = &remains.base_resp {
        if base.status_code != 0 {
            bail!(
                "API devolvió {}: {}",
                base.status_code,
                base.status_msg.as_deref().unwrap_or("?")
            );
        }
    }

    let windows = windows_from(&remains.model_remains);
    if windows.is_empty() {
        bail!("sin ventanas de plan activas en la respuesta");
    }

    Ok(ProviderStatus {
        id: "minimax".into(),
        name: "MiniMax".into(),
        icon: ICON.into(),
        plan: Some("token plan".into()),
        account: None,
        windows,
        reset_credits_available: None,
        stale_since: None,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Live endpoint response captured on 2026-07-14; values are rounded.
    const FIXTURE: &str = r#"{
      "model_remains": [
        {
          "start_time": 1784023200000,
          "end_time": 1784041200000,
          "remains_time": 3139240,
          "model_name": "general",
          "weekly_start_time": 1783900800000,
          "weekly_end_time": 1784505600000,
          "current_interval_status": 2,
          "current_interval_remaining_percent": 25,
          "current_weekly_status": 3,
          "current_weekly_remaining_percent": 100
        },
        {
          "start_time": 1783987200000,
          "end_time": 1784073600000,
          "model_name": "video",
          "weekly_end_time": 1784505600000,
          "current_interval_status": 3,
          "current_interval_remaining_percent": 100,
          "current_weekly_status": 3,
          "current_weekly_remaining_percent": 100
        }
      ],
      "base_resp": { "status_code": 0, "status_msg": "success" }
    }"#;

    #[test]
    fn mapea_la_respuesta_real() {
        let resp: RemainsResponse = serde_json::from_str(FIXTURE).unwrap();
        let windows = windows_from(&resp.model_remains);
        // `general`: only its 5h interval; weekly status 3 is outside the plan.
        // `video`: every window is outside the plan.
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "5h");
        assert_eq!(windows[0].used_percent, 75.0); // Used percentage = 100 - 25 remaining.
        assert_eq!(windows[0].resets_at, Some(1_784_041_200)); // API milliseconds -> UI Unix seconds.
    }

    #[test]
    fn ventana_semanal_dentro_del_plan_y_modelo_no_general() {
        let resp: RemainsResponse = serde_json::from_str(
            r#"{
              "model_remains": [{
                "start_time": 0,
                "end_time": 604800000,
                "model_name": "video",
                "weekly_end_time": 604800000,
                "current_interval_status": 1,
                "current_interval_remaining_percent": 90,
                "current_weekly_status": 1,
                "current_weekly_remaining_percent": 40
              }]
            }"#,
        )
        .unwrap();
        let windows = windows_from(&resp.model_remains);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].label, "semana · video");
        assert_eq!(windows[0].used_percent, 10.0);
        assert_eq!(windows[1].label, "semana · video");
        assert_eq!(windows[1].used_percent, 60.0);
    }

    #[test]
    fn sin_ventanas_activas_da_lista_vacia() {
        let resp: RemainsResponse = serde_json::from_str(
            r#"{"model_remains": [{
                "model_name": "general",
                "current_interval_status": 3,
                "current_interval_remaining_percent": 100,
                "current_weekly_status": 3,
                "current_weekly_remaining_percent": 100
            }]}"#,
        )
        .unwrap();
        assert!(windows_from(&resp.model_remains).is_empty());
    }

    #[test]
    fn etiquetas_de_intervalo() {
        assert_eq!(interval_label(Some(0), Some(18_000_000)), "5h");
        assert_eq!(interval_label(Some(0), Some(86_400_000)), "24h");
        assert_eq!(interval_label(Some(0), Some(604_800_000)), "semana");
        assert_eq!(interval_label(None, None), "ventana");
    }
}
