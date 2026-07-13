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
            error: Some(format!("{e:#}")),
        }
    }

    /// Most urgent window: highest used_percent, preferring active ones.
    pub fn worst(&self) -> Option<&Window> {
        self.windows
            .iter()
            .max_by(|a, b| {
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
            claude::collect().unwrap_or_else(|e| ProviderStatus::err("claude", "Claude Code", claude::ICON, e)),
        );
    }
    if codex::available() {
        providers.push(
            codex::collect().unwrap_or_else(|e| ProviderStatus::err("codex", "Codex", codex::ICON, e)),
        );
    }

    Status {
        fetched_at: chrono::Utc::now().timestamp(),
        providers,
    }
}
