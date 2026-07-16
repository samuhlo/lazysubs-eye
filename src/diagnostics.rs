use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set_verbose(enabled: bool) {
    VERBOSE.store(enabled, Ordering::Relaxed);
}

pub fn verbose(message: impl AsRef<str>) {
    if VERBOSE.load(Ordering::Relaxed) {
        eprintln!("diagnóstico: {}", sanitize_error(message.as_ref()));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastError {
    pub code: String,
    pub message: String,
    pub at: i64,
}

fn last_error_path() -> std::path::PathBuf {
    crate::cache::dir().join("last-error.json")
}

pub fn record_last_error(code: &str, message: impl AsRef<str>) {
    let event = LastError {
        code: code.to_owned(),
        message: sanitize_error(message),
        at: chrono::Utc::now().timestamp(),
    };
    if let Ok(bytes) = serde_json::to_vec(&event) {
        let _ = crate::cache::atomic_save(&last_error_path(), &bytes);
    }
}

pub fn last_error() -> Option<LastError> {
    let raw = std::fs::read(last_error_path()).ok()?;
    serde_json::from_slice(&raw).ok()
}

/// Normaliza errores antes de mostrarlos o persistirlos. Conserva la causa
/// accionable, pero elimina credenciales, URLs privadas, saltos de línea y
/// detalles internos de SQLite.
pub fn sanitize_error(message: impl AsRef<str>) -> String {
    let mut sanitized = message.as_ref().replace(['\n', '\r'], " ");
    if let Some(home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        sanitized = sanitized.replace(&home.to_string_lossy().to_string(), "~");
    }
    for marker in [
        "api_key=",
        "api-key=",
        "token=",
        "access_token=",
        "Authorization: Bearer ",
        "Bearer ",
    ] {
        let mut cursor = 0;
        while let Some(relative) = sanitized[cursor..].find(marker) {
            let start = cursor + relative;
            let value_start = start + marker.len();
            let value_end = sanitized[value_start..]
                .find(|c: char| c.is_whitespace() || c == '&' || c == ',' || c == ';')
                .map(|offset| value_start + offset)
                .unwrap_or(sanitized.len());
            if value_start == value_end {
                break;
            }
            sanitized.replace_range(value_start..value_end, "[REDACTED]");
            cursor = value_start + "[REDACTED]".len();
        }
    }
    for scheme in ["https://", "http://"] {
        let mut cursor = 0;
        while let Some(relative) = sanitized[cursor..].find(scheme) {
            let start = cursor + relative;
            let token_end = sanitized[start..]
                .find(char::is_whitespace)
                .map(|offset| start + offset)
                .unwrap_or(sanitized.len());
            let authority_start = start + scheme.len();
            let authority_end = sanitized[authority_start..token_end]
                .find(['/', '?', '#'])
                .map(|offset| authority_start + offset)
                .unwrap_or(token_end);
            if authority_end < token_end {
                sanitized.replace_range(authority_end..token_end, "/[REDACTED]");
                cursor = authority_end + "/[REDACTED]".len();
            } else {
                cursor = token_end;
            }
        }
    }
    if sanitized.to_ascii_lowercase().contains("sqlite") {
        return "no se pudo acceder a la base de datos local; comprueba sus permisos".into();
    }
    sanitized
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LazysubsError {
    ConfigParseError,
    ConfigValidationError,
    ProviderUnavailable,
    ProviderStale,
    ProviderNotConfigured,
    PermissionDenied,
    BinaryNotFound,
    NotifySendNotFound,
}

impl LazysubsError {
    pub fn code(self) -> &'static str {
        match self {
            Self::ConfigParseError => "E001",
            Self::ConfigValidationError => "E002",
            Self::ProviderUnavailable => "E003",
            Self::ProviderStale => "E004",
            Self::ProviderNotConfigured => "E005",
            Self::PermissionDenied => "E006",
            Self::BinaryNotFound => "E007",
            Self::NotifySendNotFound => "E008",
        }
    }
}

impl std::fmt::Display for LazysubsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            Self::ConfigParseError => "la configuración no se puede interpretar",
            Self::ConfigValidationError => "la configuración contiene valores incoherentes",
            Self::ProviderUnavailable => "un provider no está disponible",
            Self::ProviderStale => "un provider solo tiene datos antiguos",
            Self::ProviderNotConfigured => "no hay providers configurados",
            Self::PermissionDenied => "faltan permisos para acceder al estado local",
            Self::BinaryNotFound => "no se encontró un binario requerido",
            Self::NotifySendNotFound => "notify-send no está instalado",
        };
        write!(f, "{} {description}", self.code())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DoctorCheck {
    pub name: &'static str,
    pub state: CheckState,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DoctorReport {
    pub version: &'static str,
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn exit_code(&self) -> i32 {
        i32::from(
            self.checks
                .iter()
                .any(|check| check.state == CheckState::Fail),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_son_estables_y_accionables() {
        assert_eq!(LazysubsError::ConfigParseError.code(), "E001");
        assert_eq!(LazysubsError::NotifySendNotFound.code(), "E008");
        assert!(LazysubsError::PermissionDenied
            .to_string()
            .contains("permisos"));
    }

    #[test]
    fn sanitize_elimina_home_secretos_sqlite_y_multilinea() {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = format!("{home}/private api_key=secret\notra línea");
        let clean = sanitize_error(path);
        assert!(!clean.contains(&home));
        assert!(!clean.contains("secret"));
        assert!(!clean.contains('\n'));
        assert_eq!(
            sanitize_error("SQLite error: database is locked"),
            "no se pudo acceder a la base de datos local; comprueba sus permisos"
        );
        assert_eq!(
            sanitize_error("falló https://example.test/private?token=x ahora"),
            "falló https://example.test/[REDACTED] ahora"
        );
    }

    #[test]
    fn doctor_report_solo_usa_exit_cero_o_uno() {
        let report = DoctorReport {
            version: "test",
            checks: vec![DoctorCheck {
                name: "config",
                state: CheckState::Warn,
                message: "opcional".into(),
            }],
        };
        assert_eq!(report.exit_code(), 0);
    }
}
