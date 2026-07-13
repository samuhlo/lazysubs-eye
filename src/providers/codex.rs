use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use super::{ProviderStatus, Window};

pub const ICON: &str = "⬡";

const RPC_TIMEOUT: Duration = Duration::from_secs(15);

fn auth_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".codex/auth.json")
}

pub fn available() -> bool {
    auth_path().exists()
}

#[derive(Deserialize)]
struct RateLimitsResponse {
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitSnapshot,
}

#[derive(Deserialize)]
struct RateLimitSnapshot {
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
    #[serde(rename = "planType")]
    plan_type: Option<String>,
}

#[derive(Deserialize)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: f64,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<i64>,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
}

fn window_label(mins: Option<i64>) -> String {
    match mins {
        Some(10080) => "semana".into(),
        Some(m) if m % 60 == 0 => format!("{}h", m / 60),
        Some(m) => format!("{m}m"),
        None => "ventana".into(),
    }
}

/// Guard that kills the app-server child even on early return.
struct KillOnDrop(Child);
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn collect() -> Result<ProviderStatus> {
    let mut child = Command::new("codex")
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("lanzando codex app-server")?;

    let mut stdin = child.stdin.take().context("sin stdin")?;
    let stdout = child.stdout.take().context("sin stdout")?;
    let child = KillOnDrop(child);

    let (tx, rx) = mpsc::channel::<serde_json::Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });

    for msg in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize",
               "params":{"clientInfo":{"name":"lazysubs","title":"lazysubs","version":env!("CARGO_PKG_VERSION")}}}),
        json!({"jsonrpc":"2.0","method":"initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read","params":{}}),
    ] {
        writeln!(stdin, "{msg}").context("escribiendo al app-server")?;
    }
    stdin.flush()?;

    let deadline = std::time::Instant::now() + RPC_TIMEOUT;
    let result = loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            bail!("timeout esperando rate limits del app-server");
        }
        let msg = rx
            .recv_timeout(remaining)
            .map_err(|_| anyhow::anyhow!("app-server cerró sin responder"))?;
        if msg.get("id") == Some(&json!(2)) {
            if let Some(err) = msg.get("error") {
                bail!("app-server devolvió error: {err}");
            }
            break msg.get("result").cloned().context("respuesta sin result")?;
        }
    };
    drop(child); // el guard mata el app-server; ya tenemos la respuesta

    let parsed: RateLimitsResponse = serde_json::from_value(result).context("parseando rate limits")?;

    let mut windows = Vec::new();
    for w in [parsed.rate_limits.primary, parsed.rate_limits.secondary].into_iter().flatten() {
        windows.push(Window {
            label: window_label(w.window_duration_mins),
            used_percent: w.used_percent,
            resets_at: w.resets_at,
            active: true,
        });
    }

    Ok(ProviderStatus {
        id: "codex".into(),
        name: "Codex".into(),
        icon: ICON.into(),
        plan: parsed.rate_limits.plan_type,
        windows,
        error: None,
    })
}
