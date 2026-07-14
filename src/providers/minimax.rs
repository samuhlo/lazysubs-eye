//! Collector de MiniMax (coding/token plan).
//!
//! `GET {base_url}/v1/token_plan/remains` con `Authorization: Bearer <key>`,
//! donde la key es la **Subscription Key** del plan (no una API key normal de
//! la plataforma). Se configura en `[minimax] api_key` del config.toml o en
//! la variable de entorno `MINIMAX_API_KEY`.
//!
//! Semántica de la respuesta (verificada en vivo, no re-derivar):
//! - `model_remains[]`: una entrada por modelo del plan (`general` = LLM,
//!   `video`, …), cada una con ventana de intervalo (5h) y semanal.
//! - Los tiempos (`start_time`, `end_time`, `weekly_end_time`) van en
//!   **milisegundos** unix.
//! - `*_remaining_percent` es cuota **restante**, no consumida: se invierte.
//! - `*_status == 3` significa que esa ventana no forma parte del plan
//!   contratado: se omite.

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use super::{ProviderStatus, Window};

pub const ICON: &str = "◆";
const DEFAULT_BASE_URL: &str = "https://api.minimax.io";
const STATUS_NOT_IN_PLAN: i64 = 3;

fn api_key() -> Option<String> {
    crate::config::get()
        .minimax
        .api_key
        .clone()
        .or_else(|| std::env::var("MINIMAX_API_KEY").ok())
        .filter(|k| !k.trim().is_empty())
}

pub fn available() -> bool {
    api_key().is_some()
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

/// El modelo `general` (el del plan de coding) va sin prefijo; el resto de
/// modelos del plan (video…) llevan el nombre para distinguirlos.
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

pub fn collect() -> Result<ProviderStatus> {
    let key = api_key().context("falta la api_key de MiniMax")?;
    let base_url = crate::config::get()
        .minimax
        .base_url
        .clone()
        .unwrap_or_else(|| DEFAULT_BASE_URL.into());

    let resp = ureq::get(&format!("{base_url}/v1/token_plan/remains"))
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
        windows,
        reset_credits_available: None,
        stale_since: None,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Respuesta real del endpoint (2026-07-14), valores redondeados.
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
        // general: solo el intervalo 5h (la semanal es status 3 = fuera del
        // plan); video: fuera del plan por completo.
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].label, "5h");
        assert_eq!(windows[0].used_percent, 75.0); // 100 - 25 restante
        assert_eq!(windows[0].resets_at, Some(1_784_041_200)); // ms → s
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
