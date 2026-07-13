use crate::providers::Status;
use serde_json::json;

const WARNING_AT: f64 = 80.0;
const CRITICAL_AT: f64 = 95.0;

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

fn class_for(percent: f64) -> &'static str {
    if percent >= CRITICAL_AT {
        "critical"
    } else if percent >= WARNING_AT {
        "warning"
    } else {
        "normal"
    }
}

pub fn waybar(status: &Status) -> String {
    let mut parts = Vec::new();
    let mut tooltip = Vec::new();
    let mut max_percent: f64 = 0.0;
    let mut has_error = false;

    for p in &status.providers {
        if let Some(err) = &p.error {
            parts.push(format!("{} !", p.icon));
            tooltip.push(format!("{}: {}", p.name, err));
            has_error = true;
            continue;
        }
        let Some(worst) = p.worst() else { continue };
        max_percent = max_percent.max(worst.used_percent);
        parts.push(format!("{} {:.0}%", p.icon, worst.used_percent));

        let plan = p.plan.as_deref().unwrap_or("?");
        tooltip.push(format!("{} ({plan})", p.name));
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
        class_for(max_percent)
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
                windows: vec![Window {
                    label: "5h".into(),
                    used_percent: 40.0,
                    resets_at: None,
                    active: true,
                }],
                reset_credits_available: credits,
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
