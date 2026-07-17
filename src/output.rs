use crate::config;
use crate::providers::{ProviderStatus, Status, Window};
use serde_json::json;

/// [UI] WAYBAR WINDOW SELECTION
///
/// Prefers the configured `[waybar.window]` label, first exactly and then as a
/// substring. Without a match, falls back to the provider's most urgent window.
fn display_window<'a>(p: &'a ProviderStatus, config: &config::Config) -> Option<&'a Window> {
    if let Some(want) = config.waybar.window.as_ref().and_then(|m| m.get(&p.id)) {
        if let Some(w) = p
            .windows
            .iter()
            .find(|w| w.label.eq_ignore_ascii_case(want))
        {
            return Some(w);
        }
        let want_lc = want.to_lowercase();
        if let Some(w) = p
            .windows
            .iter()
            .find(|w| w.label.to_lowercase().contains(&want_lc))
        {
            return Some(w);
        }
    }
    p.worst()
}

/// [UI] COMPACT RESET COUNTDOWN
///
/// Uses whole-day, hour, and minute units for narrow terminal and Waybar output.
/// A non-positive duration becomes `ahora`, avoiding negative countdowns.
pub fn countdown(resets_at: i64) -> String {
    let secs = resets_at - chrono::Utc::now().timestamp();
    if secs <= 0 {
        return "ahora".into();
    }
    let (d, h, m) = (secs / 86400, (secs % 86400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d{h}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

/// Formats elapsed time using the same compact units as `countdown()`.
/// WHY: stale-data age and reset time must remain visually comparable.
pub fn age(since: i64) -> String {
    let secs = (chrono::Utc::now().timestamp() - since).max(0);
    let (d, h, m) = (secs / 86400, (secs % 86400) / 3600, (secs % 3600) / 60);
    if d > 0 {
        format!("{d}d{h}h")
    } else if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

/// Maps usage to the CSS severity class.
/// INVARIANT: disabled colors never suppress the separate error class.
fn class_for(percent: f64, config: &config::Config) -> &'static str {
    if !config.colors {
        return "normal";
    }
    if percent >= config.critical_at {
        "critical"
    } else if percent >= config.warning_at {
        "warning"
    } else {
        "normal"
    }
}

pub fn waybar(status: &Status) -> String {
    waybar_with(status, &config::get())
}

/// [UI] WAYBAR JSON CONTRACT
///
/// Builds one custom-module payload from selected providers. Provider errors win
/// over usage severity so a broken data source cannot look healthy.
fn waybar_with(status: &Status, config: &config::Config) -> String {
    let mut parts = Vec::new();
    let mut tooltip = Vec::new();
    let mut max_percent: f64 = 0.0;
    let mut has_error = false;

    for p in crate::providers::select(&status.providers, &config.waybar.providers) {
        if let Some(err) = &p.error {
            parts.push(format!("{} !", p.icon));
            tooltip.push(format!("{}: {}", p.name, err));
            has_error = true;
            continue;
        }
        let Some(worst) = display_window(p, config) else {
            continue;
        };
        max_percent = max_percent.max(worst.used_percent);
        if config.waybar.percent() {
            parts.push(format!("{} {:.0}%", p.icon, worst.used_percent));
        } else {
            parts.push(p.icon.clone());
        }

        let plan = p.plan.as_deref().unwrap_or("?");
        tooltip.push(format!("{} ({plan})", p.name));
        if config.show_account {
            if let Some(account) = &p.account {
                tooltip.push(format!("  {account}"));
            }
        }
        if let Some(since) = p.stale_since {
            tooltip.push(format!("  datos de hace {}", age(since)));
        }
        for w in &p.windows {
            let reset = w
                .resets_at
                .map(|t| format!("  → {}", countdown(t)))
                .unwrap_or_default();
            tooltip.push(format!(
                "  {:<16} {:>3.0}%{}",
                w.label, w.used_percent, reset
            ));
        }
        tooltip.push(String::new());
    }

    if parts.is_empty() {
        parts.push("sin providers".into());
    }
    let class = if has_error {
        "error"
    } else {
        class_for(max_percent, config)
    };

    json!({
        "text": parts.join("  "),
        "tooltip": tooltip.join("\n").trim_end(),
        "class": class,
        "percentage": max_percent as i64,
    })
    .to_string()
}

pub fn pretty(status: &Status) -> String {
    serde_json::to_string_pretty(status).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::{ProviderStatus, Window};

    fn status(credits: Option<u64>, error: Option<&str>) -> Status {
        Status {
            fetched_at: 1,
            providers: vec![ProviderStatus {
                id: "codex".into(),
                name: "Codex".into(),
                icon: "⬡".into(),
                plan: Some("pro".into()),
                account: None,
                windows: vec![Window {
                    label: "5h".into(),
                    used_percent: 40.0,
                    resets_at: None,
                    active: true,
                }],
                reset_credits_available: credits,
                stale_since: None,
                error: error.map(str::to_owned),
            }],
        }
    }

    #[test]
    fn pretty_serializes_reset_credits_only_when_present() {
        for credits in [Some(3), Some(0)] {
            let value: serde_json::Value =
                serde_json::from_str(&pretty(&status(credits, None))).unwrap();
            assert_eq!(
                value["providers"][0]["reset_credits_available"],
                serde_json::json!(credits)
            );
            assert_eq!(value["providers"][0]["id"], "codex");
            assert_eq!(value["providers"][0]["windows"][0]["used_percent"], 40.0);
        }
        let value: serde_json::Value = serde_json::from_str(&pretty(&status(None, None))).unwrap();
        assert!(value["providers"][0]
            .get("reset_credits_available")
            .is_none());
    }

    #[test]
    fn waybar_ignores_reset_credits_in_normal_and_error_statuses() {
        assert_eq!(waybar(&status(Some(3), None)), waybar(&status(None, None)));
        assert_eq!(
            waybar(&status(Some(0), Some("falló"))),
            waybar(&status(None, Some("falló")))
        );
    }

    #[test]
    fn waybar_respeta_visibilidad_orden_percent_y_colors() {
        let mut status = status(None, None);
        status.providers.push(ProviderStatus {
            id: "claude".into(),
            name: "Claude Code".into(),
            icon: "✳".into(),
            plan: Some("pro".into()),
            account: None,
            windows: vec![Window {
                label: "5h".into(),
                used_percent: 90.0,
                resets_at: None,
                active: true,
            }],
            reset_credits_available: None,
            stale_since: None,
            error: None,
        });
        let mut config = crate::config::Config::default();

        // Reversed order with one provider hidden.
        config.waybar.providers = Some(vec!["claude".into()]);
        let out: serde_json::Value = serde_json::from_str(&waybar_with(&status, &config)).unwrap();
        assert_eq!(out["text"], "✳ 90%");
        assert_eq!(out["class"], "warning"); // Hidden Codex at 40% does not affect the class.

        // Icons only.
        config.waybar.percent = Some(false);
        let out: serde_json::Value = serde_json::from_str(&waybar_with(&status, &config)).unwrap();
        assert_eq!(out["text"], "✳");

        // Without threshold colors, class stays normal while percentage remains.
        config.colors = false;
        let out: serde_json::Value = serde_json::from_str(&waybar_with(&status, &config)).unwrap();
        assert_eq!(out["class"], "normal");
        assert_eq!(out["percentage"], 90);
    }

    #[test]
    fn waybar_window_elige_por_etiqueta_o_worst() {
        let mut status = status(None, None);
        status.providers[0].windows = vec![
            Window {
                label: "5h".into(),
                used_percent: 90.0,
                resets_at: None,
                active: true,
            },
            Window {
                label: "semana".into(),
                used_percent: 30.0,
                resets_at: None,
                active: true,
            },
            Window {
                label: "semana · Fable".into(),
                used_percent: 55.0,
                resets_at: None,
                active: true,
            },
        ];
        let mut config = crate::config::Config::default();

        // No selection -> worst window (5h, 90%).
        let out: serde_json::Value = serde_json::from_str(&waybar_with(&status, &config)).unwrap();
        assert_eq!(out["text"], "⬡ 90%");

        // Exact "semana" label, not the Fable variant.
        let mut map = std::collections::BTreeMap::new();
        map.insert("codex".to_string(), "semana".to_string());
        config.waybar.window = Some(map.clone());
        let out: serde_json::Value = serde_json::from_str(&waybar_with(&status, &config)).unwrap();
        assert_eq!(out["text"], "⬡ 30%");
        assert_eq!(
            out["percentage"], 30,
            "la clase también sigue la ventana elegida"
        );

        // Substring "Fable" -> semana · Fable (55%).
        map.insert("codex".to_string(), "Fable".to_string());
        config.waybar.window = Some(map);
        let out: serde_json::Value = serde_json::from_str(&waybar_with(&status, &config)).unwrap();
        assert_eq!(out["text"], "⬡ 55%");
    }

    #[test]
    fn json_and_waybar_contracts_remain_byte_stable_without_pi_data() {
        let status = status(None, None);
        assert_eq!(
            pretty(&status),
            "{\n  \"fetched_at\": 1,\n  \"providers\": [\n    {\n      \"id\": \"codex\",\n      \"name\": \"Codex\",\n      \"icon\": \"⬡\",\n      \"plan\": \"pro\",\n      \"windows\": [\n        {\n          \"label\": \"5h\",\n          \"used_percent\": 40.0,\n          \"resets_at\": null,\n          \"active\": true\n        }\n      ],\n      \"error\": null\n    }\n  ]\n}"
        );
        assert_eq!(
            waybar(&status),
            "{\"class\":\"normal\",\"percentage\":40,\"text\":\"⬡ 40%\",\"tooltip\":\"Codex (pro)\\n  5h                40%\"}"
        );
    }
}
