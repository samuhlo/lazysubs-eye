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
            tooltip.push(format!("  {:<16} {:>3.0}%{}", w.label, w.used_percent, reset));
        }
        tooltip.push(String::new());
    }

    if parts.is_empty() {
        parts.push("sin providers".into());
    }
    let class = if has_error { "error" } else { class_for(max_percent) };

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
