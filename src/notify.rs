//! Notificaciones de escritorio al cruzar los umbrales de uso.
//!
//! Se llama tras cada consulta fresca (waybar corre el binario cada 60s sin
//! estado entre ejecuciones), así que el último nivel notificado por ventana
//! se persiste en `~/.cache/lazysubs-eye/notify-state.json` para no spamear:
//! solo se notifica al *subir* de nivel (none→warning, warning→critical) y el
//! estado se limpia cuando la ventana se resetea o baja del umbral.

use crate::providers::Status;
use crate::{cache, config, output};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::process::Command;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
struct WindowState {
    /// Último nivel notificado: 1 = warning, 2 = critical.
    level: u8,
    resets_at: Option<i64>,
    /// Unix secs de la última notificación de esta ventana; ancla el
    /// cooldown. `default` para leer estados de versiones anteriores.
    #[serde(default)]
    notified_at: i64,
}

type State = BTreeMap<String, WindowState>;

#[derive(Debug, PartialEq)]
struct Alert {
    provider: String,
    icon: String,
    label: String,
    percent: f64,
    resets_at: Option<i64>,
    critical: bool,
}

fn level_for(percent: f64, warning_at: f64, critical_at: f64) -> u8 {
    if percent >= critical_at {
        2
    } else if percent >= warning_at {
        1
    } else {
        0
    }
}

/// Decide qué notificar y el nuevo estado. Pura para poder testearla.
///
/// Reglas: se notifica al subir de nivel dentro de la misma ventana
/// (`resets_at` igual), pero re-notificar el mismo nivel — por reset de la
/// ventana o por bajar y volver a cruzar — respeta un `cooldown` desde la
/// última notificación. Subir a un nivel nunca notificado antes (p. ej.
/// warning→critical) salta el cooldown: es información nueva.
fn plan(
    status: &Status,
    state: &State,
    warning_at: f64,
    critical_at: f64,
    now: i64,
    cooldown: i64,
) -> (Vec<Alert>, State) {
    let mut alerts = Vec::new();
    let mut next = State::new();

    for provider in &status.providers {
        if provider.error.is_some() {
            // Sin datos frescos no se puede decidir; conserva el estado previo
            // para no re-notificar cuando el provider se recupere sin cambios.
            for (key, value) in state.range(format!("{}|", provider.id)..) {
                if !key.starts_with(&format!("{}|", provider.id)) {
                    break;
                }
                next.insert(key.clone(), *value);
            }
            continue;
        }
        for window in &provider.windows {
            let key = format!("{}|{}", provider.id, window.label);
            let level = level_for(window.used_percent, warning_at, critical_at);
            let previous = state.get(&key).copied();
            // Nivel ya notificado en ESTA ventana; un reset lo deja a 0…
            let notified_level = previous
                .filter(|s| s.resets_at == window.resets_at)
                .map(|s| s.level)
                .unwrap_or(0);
            // …pero el cooldown sobrevive a los resets (ventanas rodantes
            // como la de MiniMax cambian resets_at en cada consulta).
            let last_level = previous.map(|s| s.level).unwrap_or(0);
            let last_at = previous.map(|s| s.notified_at).unwrap_or(0);

            let escalates = level > last_level;
            if level > notified_level && (escalates || now - last_at >= cooldown) {
                alerts.push(Alert {
                    provider: provider.name.clone(),
                    icon: provider.icon.clone(),
                    label: window.label.clone(),
                    percent: window.used_percent,
                    resets_at: window.resets_at,
                    critical: level == 2,
                });
                next.insert(
                    key,
                    WindowState {
                        level,
                        resets_at: window.resets_at,
                        notified_at: now,
                    },
                );
            } else if let Some(previous) = previous {
                // Conserva el registro mientras siga anclando el cooldown o
                // el nivel siga cruzado; si no, se descarta (rearme total).
                if level > 0 || now - previous.notified_at < cooldown {
                    next.insert(key, previous);
                }
            }
        }
    }
    (alerts, next)
}

fn state_file() -> std::path::PathBuf {
    cache::dir().join("notify-state.json")
}

fn load_state() -> State {
    std::fs::read_to_string(state_file())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn send(alert: &Alert) {
    let urgency = if alert.critical { "critical" } else { "normal" };
    let reset = alert
        .resets_at
        .map(|t| format!(" — resetea en {}", output::countdown(t)))
        .unwrap_or_default();
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "lazysubs-eye",
            "-u",
            urgency,
            &format!("{} {}", alert.icon, alert.provider),
            &format!("{} al {:.0}%{reset}", alert.label, alert.percent),
        ])
        .output();
}

/// Punto de entrada: comparar el estado fresco con el persistido y notificar.
pub fn check(status: &Status) {
    let config = config::get();
    if !config.notifications {
        return;
    }
    let state = load_state();
    let (alerts, next) = plan(
        status,
        &state,
        config.warning_at,
        config.critical_at,
        chrono::Utc::now().timestamp(),
        config.notification_cooldown,
    );
    for alert in &alerts {
        send(alert);
    }
    if next != state {
        if let Ok(bytes) = serde_json::to_vec(&next) {
            let _ = cache::atomic_save(&state_file(), &bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderStatus, Window};

    fn status(percent: f64, resets_at: Option<i64>) -> Status {
        Status {
            fetched_at: 0,
            providers: vec![ProviderStatus {
                id: "claude".into(),
                name: "Claude Code".into(),
                icon: "✳".into(),
                plan: None,
                account: None,
                windows: vec![Window {
                    label: "5h".into(),
                    used_percent: percent,
                    resets_at,
                    active: true,
                }],
                reset_credits_available: None,
                stale_since: None,
                error: None,
            }],
        }
    }

    const COOLDOWN: i64 = 1800;

    fn plan_at(status: &Status, state: &State, now: i64) -> (Vec<Alert>, State) {
        plan(status, state, 80.0, 95.0, now, COOLDOWN)
    }

    #[test]
    fn notifica_al_cruzar_warning_y_no_repite() {
        let (alerts, state) = plan_at(&status(85.0, Some(100)), &State::new(), 1000);
        assert_eq!(alerts.len(), 1);
        assert!(!alerts[0].critical);

        // segunda pasada con el mismo estado: silencio
        let (alerts, state2) = plan_at(&status(88.0, Some(100)), &state, 1060);
        assert!(alerts.is_empty());
        assert_eq!(state, state2);
    }

    #[test]
    fn escala_a_critical_sin_esperar_el_cooldown() {
        let (_, state) = plan_at(&status(85.0, Some(100)), &State::new(), 1000);
        let (alerts, _) = plan_at(&status(96.0, Some(100)), &state, 1060);
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].critical);
    }

    #[test]
    fn el_reset_de_la_ventana_respeta_el_cooldown() {
        let (_, state) = plan_at(&status(85.0, Some(100)), &State::new(), 1000);
        // ventana rodante: resets_at cambia en cada consulta con consumo
        // rápido — dentro del cooldown no se repite el mismo nivel…
        let (alerts, state) = plan_at(&status(85.0, Some(200)), &state, 1060);
        assert!(alerts.is_empty());
        let (alerts, state) = plan_at(&status(85.0, Some(300)), &state, 1000 + COOLDOWN - 1);
        assert!(alerts.is_empty());
        // …y pasado el cooldown, sí
        let (alerts, _) = plan_at(&status(85.0, Some(400)), &state, 1000 + COOLDOWN);
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn bajar_y_volver_a_cruzar_tambien_respeta_el_cooldown() {
        let (_, state) = plan_at(&status(85.0, Some(100)), &State::new(), 1000);
        let (alerts, state) = plan_at(&status(20.0, Some(100)), &state, 1060);
        assert!(alerts.is_empty());
        // vuelve a cruzar enseguida: silencio (antes esto ametrallaba)
        let (alerts, state) = plan_at(&status(85.0, Some(100)), &state, 1120);
        assert!(alerts.is_empty());
        // por debajo del umbral y con el cooldown vencido, el registro se
        // descarta y el siguiente cruce notifica de inmediato
        let (alerts, state) = plan_at(&status(20.0, Some(100)), &state, 1000 + COOLDOWN);
        assert!(alerts.is_empty());
        assert!(state.is_empty());
        let (alerts, _) = plan_at(&status(85.0, Some(100)), &state, 1001 + COOLDOWN);
        assert_eq!(alerts.len(), 1);
    }

    #[test]
    fn provider_con_error_conserva_el_estado_previo() {
        let (_, state) = plan_at(&status(85.0, Some(100)), &State::new(), 1000);
        let mut errored = status(0.0, None);
        errored.providers[0].windows.clear();
        errored.providers[0].error = Some("reauth".into());
        let (alerts, state2) = plan_at(&errored, &state, 1060);
        assert!(alerts.is_empty());
        assert_eq!(state, state2);
    }

    #[test]
    fn cooldown_cero_recupera_el_comportamiento_inmediato() {
        let (_, state) = plan(&status(85.0, Some(100)), &State::new(), 80.0, 95.0, 1000, 0);
        let (alerts, _) = plan(&status(85.0, Some(200)), &state, 80.0, 95.0, 1001, 0);
        assert_eq!(alerts.len(), 1, "reset de ventana notifica al instante");
    }
}
