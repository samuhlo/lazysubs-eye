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
    #[serde(
        rename = "rateLimitResetCredits",
        default = "missing_root_reset_credits",
        deserialize_with = "deserialize_root_reset_credits"
    )]
    root_reset_credits: RootResetCredits,
}

enum RootResetCredits {
    Missing,
    Present(Option<RateLimitResetCredits>),
}

fn missing_root_reset_credits() -> RootResetCredits {
    RootResetCredits::Missing
}

fn deserialize_root_reset_credits<'de, D>(
    deserializer: D,
) -> std::result::Result<RootResetCredits, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(RootResetCredits::Present(Option::deserialize(
        deserializer,
    )?))
}

#[derive(Deserialize)]
struct RateLimitSnapshot {
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
    #[serde(rename = "planType")]
    plan_type: Option<String>,
    #[serde(rename = "rateLimitResetCredits")]
    reset_credits: Option<RateLimitResetCredits>,
}

#[derive(Deserialize)]
struct RateLimitResetCredits {
    #[serde(rename = "availableCount")]
    available_count: Option<u64>,
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

fn provider_status_from_rate_limits(response: RateLimitsResponse) -> ProviderStatus {
    let reset_credits_available = match response.root_reset_credits {
        RootResetCredits::Missing => response
            .rate_limits
            .reset_credits
            .as_ref()
            .and_then(|credits| credits.available_count),
        RootResetCredits::Present(credits) => credits.and_then(|credits| credits.available_count),
    };
    let rate_limits = response.rate_limits;
    let mut windows = Vec::new();
    for w in [rate_limits.primary, rate_limits.secondary]
        .into_iter()
        .flatten()
    {
        windows.push(Window {
            label: window_label(w.window_duration_mins),
            used_percent: w.used_percent,
            resets_at: w.resets_at,
            active: true,
        });
    }

    ProviderStatus {
        id: "codex".into(),
        name: "Codex".into(),
        icon: ICON.into(),
        plan: rate_limits.plan_type,
        account: None,
        windows,
        reset_credits_available,
        stale_since: None,
        error: None,
    }
}

fn rate_limits_result_from_response(msg: &serde_json::Value) -> Result<Option<serde_json::Value>> {
    if msg.get("id") != Some(&json!(2)) {
        return Ok(None);
    }
    if let Some(err) = msg.get("error") {
        bail!("app-server devolvió error: {err}");
    }
    Ok(Some(
        msg.get("result").cloned().context("respuesta sin result")?,
    ))
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
               "params":{"clientInfo":{"name":"lazysubs-eye","title":"lazysubs-eye","version":env!("CARGO_PKG_VERSION")}}}),
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
        if let Some(result) = rate_limits_result_from_response(&msg)? {
            break result;
        }
    };
    drop(child); // el guard mata el app-server; ya tenemos la respuesta

    let parsed: RateLimitsResponse =
        serde_json::from_value(result).context("parseando rate limits")?;

    Ok(provider_status_from_rate_limits(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn parse_rate_limits(reset_credits: Value) -> serde_json::Result<RateLimitsResponse> {
        serde_json::from_value(json!({
            "rateLimits": {
                "rateLimitResetCredits": reset_credits,
            }
        }))
    }

    #[test]
    fn deserializes_reset_credits_when_present() {
        let parsed = parse_rate_limits(json!({ "availableCount": 3 })).unwrap();
        assert_eq!(
            parsed.rate_limits.reset_credits.unwrap().available_count,
            Some(3)
        );
    }

    #[test]
    fn preserves_zero_reset_credits() {
        let parsed = parse_rate_limits(json!({ "availableCount": 0 })).unwrap();
        assert_eq!(
            parsed.rate_limits.reset_credits.unwrap().available_count,
            Some(0)
        );
    }

    fn parse_live_rate_limits(
        root_credits: Value,
        nested_credits: Option<Value>,
    ) -> serde_json::Result<RateLimitsResponse> {
        let mut rate_limits = json!({
            "primary": { "usedPercent": 40.0, "windowDurationMins": 300, "resetsAt": 100 },
            "secondary": { "usedPercent": 20.0, "windowDurationMins": 10080, "resetsAt": 200 },
            "planType": "pro",
        });
        if let Some(nested_credits) = nested_credits {
            rate_limits["rateLimitResetCredits"] = nested_credits;
        }
        serde_json::from_value(json!({
            "rateLimits": rate_limits,
            "rateLimitResetCredits": root_credits,
            "rateLimitsByLimitId": { "primary": { "usedPercent": 40.0 } },
        }))
    }

    #[test]
    fn reads_reset_credits_from_the_live_result_root() {
        let parsed = parse_live_rate_limits(json!({ "availableCount": 7 }), None).unwrap();
        let status = provider_status_from_rate_limits(parsed);

        assert_eq!(status.reset_credits_available, Some(7));
        assert_eq!(status.plan.as_deref(), Some("pro"));
        assert_eq!(status.windows.len(), 2);
    }

    #[test]
    fn root_reset_credits_take_precedence_over_nested_fallback() {
        let parsed = parse_live_rate_limits(
            json!({ "availableCount": 7 }),
            Some(json!({ "availableCount": 3 })),
        )
        .unwrap();
        let status = provider_status_from_rate_limits(parsed);

        assert_eq!(status.reset_credits_available, Some(7));
    }

    #[test]
    fn treats_missing_or_null_reset_credits_as_absent() {
        for value in [
            serde_json::from_value(json!({ "rateLimits": {} })).unwrap(),
            parse_rate_limits(Value::Null).unwrap(),
        ] {
            assert!(value.rate_limits.reset_credits.is_none());
        }

        for value in [json!({}), json!({ "availableCount": null })] {
            let parsed = parse_rate_limits(value).unwrap();
            assert_eq!(
                parsed.rate_limits.reset_credits.unwrap().available_count,
                None
            );
        }
    }

    #[test]
    fn rejects_invalid_reset_credit_values() {
        for value in [json!(-1), json!(1.5), json!("3")] {
            assert!(parse_rate_limits(json!({ "availableCount": value })).is_err());
        }
        assert!(serde_json::from_str::<RateLimitsResponse>(
            r#"{"rateLimits":{"rateLimitResetCredits":{"availableCount":18446744073709551616}}}"#
        )
        .is_err());
    }

    #[test]
    fn turns_json_rpc_error_responses_into_creditless_error_statuses() {
        let error = rate_limits_result_from_response(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "error": { "code": -32000, "message": "denied" },
        }))
        .unwrap_err();
        let status = ProviderStatus::err("codex", "Codex", ICON, error);

        assert_eq!(
            status.error.as_deref(),
            Some("app-server devolvió error: {\"code\":-32000,\"message\":\"denied\"}")
        );
        assert_eq!(status.reset_credits_available, None);
    }

    #[test]
    fn ignores_other_responses_and_accepts_the_rate_limits_result() {
        assert_eq!(
            rate_limits_result_from_response(&json!({ "jsonrpc": "2.0", "id": 1, "result": {} }))
                .unwrap(),
            None
        );
        assert_eq!(
            rate_limits_result_from_response(&json!({
                "jsonrpc": "2.0",
                "id": 2,
                "result": { "rateLimits": {} },
            }))
            .unwrap(),
            Some(json!({ "rateLimits": {} }))
        );
    }

    #[test]
    fn maps_snapshot_to_status_without_changing_plan_or_windows() {
        for (credits, expected) in [
            (json!({ "availableCount": 3 }), Some(3)),
            (json!({ "availableCount": 0 }), Some(0)),
            (Value::Null, None),
        ] {
            let parsed: RateLimitsResponse = serde_json::from_value(json!({
                "rateLimits": {
                    "primary": { "usedPercent": 40.0, "windowDurationMins": 300, "resetsAt": 100 },
                    "secondary": { "usedPercent": 20.0, "windowDurationMins": 10080, "resetsAt": 200 },
                    "planType": "pro",
                    "rateLimitResetCredits": credits,
                }
            }))
            .unwrap();

            let status = provider_status_from_rate_limits(parsed);
            assert_eq!(status.plan.as_deref(), Some("pro"));
            assert_eq!(status.windows.len(), 2);
            assert_eq!(status.windows[0].label, "5h");
            assert_eq!(status.windows[1].label, "semana");
            assert_eq!(status.reset_credits_available, expected);
        }
    }
}
