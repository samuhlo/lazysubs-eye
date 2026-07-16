use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::{ProviderStatus, Window};

pub const ICON: &str = "✳";

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

pub fn default_creds_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".claude/.credentials.json")
}

pub fn available_at(creds: &Path) -> bool {
    creds.exists()
}

#[derive(Deserialize)]
struct ClaudeJson {
    #[serde(rename = "oauthAccount")]
    oauth_account: Option<OauthAccount>,
}

#[derive(Deserialize)]
struct OauthAccount {
    #[serde(rename = "emailAddress")]
    email_address: Option<String>,
}

/// Identidad de la cuenta desde `~/.claude.json` (`oauthAccount.emailAddress`).
/// Solo lee el email; nunca toca los tokens. None si el fichero no existe o no
/// lo tiene (degrada sin ruido: la cuenta es un extra informativo).
fn account_identity() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".claude.json");
    let raw = std::fs::read_to_string(path).ok()?;
    parse_account(&raw)
}

fn parse_account(raw: &str) -> Option<String> {
    let parsed: ClaudeJson = serde_json::from_str(raw).ok()?;
    parsed
        .oauth_account?
        .email_address
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_desde_oauth_account_email() {
        let raw = r#"{"oauthAccount":{"emailAddress":"sam@example.com","displayName":"Sam"}}"#;
        assert_eq!(parse_account(raw).as_deref(), Some("sam@example.com"));
    }

    #[test]
    fn account_ausente_o_vacio_da_none() {
        assert_eq!(parse_account("{}"), None);
        assert_eq!(parse_account(r#"{"oauthAccount":{}}"#), None);
        assert_eq!(
            parse_account(r#"{"oauthAccount":{"emailAddress":"  "}}"#),
            None
        );
        assert_eq!(parse_account("no es json"), None);
    }
}

#[derive(Deserialize)]
struct CredsFile {
    #[serde(rename = "claudeAiOauth")]
    oauth: Oauth,
}

#[derive(Deserialize)]
struct Oauth {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

#[derive(Deserialize)]
struct UsageResponse {
    limits: Vec<Limit>,
}

#[derive(Deserialize)]
struct Limit {
    kind: String,
    percent: f64,
    resets_at: Option<String>,
    is_active: bool,
    scope: Option<Scope>,
}

#[derive(Deserialize)]
struct Scope {
    model: Option<ScopeModel>,
}

#[derive(Deserialize)]
struct ScopeModel {
    display_name: Option<String>,
}

fn label_for(l: &Limit) -> String {
    match l.kind.as_str() {
        "session" => "5h".into(),
        "weekly_all" => "semana".into(),
        "weekly_scoped" => {
            let model = l
                .scope
                .as_ref()
                .and_then(|s| s.model.as_ref())
                .and_then(|m| m.display_name.as_deref())
                .unwrap_or("modelo");
            format!("semana · {model}")
        }
        other => other.replace('_', " "),
    }
}

/// Claude Code refreshes this token itself; never attempt a refresh here or
/// we would invalidate the CLI's refresh token. On 401 just surface "reauth".
pub fn collect(creds_path: &Path) -> Result<ProviderStatus> {
    let raw = std::fs::read_to_string(creds_path).context("leyendo credenciales")?;
    let creds: CredsFile = serde_json::from_str(&raw).context("parseando credenciales")?;
    // El email solo es fiable para la cuenta primaria (~/.claude.json es único);
    // para credenciales de otra cuenta no lo autodetectamos (va el alias).
    let account = (creds_path == default_creds_path())
        .then(account_identity)
        .flatten();

    if creds.oauth.expires_at / 1000 < chrono::Utc::now().timestamp() {
        bail!("token caducado — abre Claude Code para refrescarlo");
    }

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout_read(std::time::Duration::from_secs(5))
        .timeout_write(std::time::Duration::from_secs(3))
        .build();
    let resp = agent
        .get(USAGE_URL)
        .set(
            "Authorization",
            &format!("Bearer {}", creds.oauth.access_token),
        )
        .set("anthropic-beta", "oauth-2025-04-20")
        .call();

    let usage: UsageResponse = match resp {
        Ok(r) => r.into_json().context("parseando respuesta de usage")?,
        Err(ureq::Error::Status(401, _)) => {
            bail!("token rechazado — abre Claude Code para refrescarlo")
        }
        Err(ureq::Error::Status(code, _)) => bail!("API respondió {code}"),
        Err(e) => return Err(e).context("llamando al endpoint de usage"),
    };

    let windows = usage
        .limits
        .iter()
        .map(|l| Window {
            label: label_for(l),
            used_percent: l.percent,
            resets_at: l
                .resets_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp()),
            active: l.is_active,
        })
        .collect();

    Ok(ProviderStatus {
        id: "claude".into(),
        name: "Claude Code".into(),
        icon: ICON.into(),
        plan: creds.oauth.subscription_type,
        account,
        windows,
        reset_credits_available: None,
        stale_since: None,
        error: None,
    })
}
