pub mod claude;
pub mod codex;

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

pub fn collect_all() -> Status {
    let mut providers = Vec::new();

    if claude::available() {
        providers.push(
            claude::collect()
                .unwrap_or_else(|e| ProviderStatus::err("claude", "Claude Code", claude::ICON, e)),
        );
    }
    if codex::available() {
        providers.push(
            codex::collect()
                .unwrap_or_else(|e| ProviderStatus::err("codex", "Codex", codex::ICON, e)),
        );
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
