mod cache;
mod config;
mod diagnostics;
mod file_lock;
mod history;
mod install;
mod notify;
mod opencode_tokens;
mod output;
mod performance;
mod pi_tokens;
mod providers;
mod tokens;
mod tui;

use std::io::IsTerminal;

const DEFAULT_SIGNAL: u8 = 11;

const HELP: &str = "\
lazysubs-eye — monitor de cuotas de suscripciones de IA (Claude Code, Codex)

Uso: lazysubs-eye [tui|install|uninstall|--json|--waybar|--check] [opciones]

  tui         interfaz de terminal (por defecto si stdout es una tty)
  doctor      comprueba configuración y dependencias locales (`doctor --json`)
  install     integra lazysubs-eye en waybar y Hyprland (idempotente, con backups)
  uninstall   revierte la integración
  --json      volcado completo del estado (por defecto sin tty)
  --waybar    JSON de una línea para un módulo custom de waybar
  --check     resume el estado y sale con 0 ok / 1 warning / 2 critical / 3 error
  --no-cache  fuerza una consulta fresca a los providers
  --verbose   escribe decisiones de collectors y caché en stderr (sin secretos)
  --dry-run   con install, muestra el plan sin modificar el sistema
  --sandbox D con install, opera sólo dentro del XDG config root D y no recarga servicios
  --ttl N     validez de la cache en segundos (por defecto 60)
  --signal N  señal RTMIN+N del módulo waybar en install (por defecto 11)
  --version   muestra la versión

Config opcional en ~/.config/lazysubs-eye/config.toml (umbrales, ttl,
providers, iconos, notificaciones); ver README.

EXIT CODES
  0  OK: datos frescos y sin umbrales activos
  1  warning: datos stale/partial o umbral de aviso
  2  critical: umbral crítico
  3  error operativo, sin providers o configuración inválida
";

fn main() {
    let mut mode = if std::io::stdout().is_terminal() {
        "tui"
    } else {
        "json"
    };
    let mut use_cache = true;
    let mut dry_run = false;
    let mut doctor_json = false;
    let mut verbose = false;
    let mut sandbox = None;
    let mut ttl = config::get().ttl;
    let mut signal = DEFAULT_SIGNAL;

    // [FLOW] Parse left to right so `doctor --json` is a doctor-specific output
    // modifier while standalone `--json` selects the normal machine interface.
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "tui" | "--tui" => mode = "tui",
            "install" => mode = "install",
            "uninstall" => mode = "uninstall",
            "doctor" => mode = "doctor",
            "--json" if mode == "doctor" => doctor_json = true,
            "--json" => mode = "json",
            "--waybar" => mode = "waybar",
            "--check" => mode = "check",
            "--no-cache" => use_cache = false,
            "--verbose" => verbose = true,
            "--dry-run" => dry_run = true,
            "--sandbox" => sandbox = args.next().map(std::path::PathBuf::from),
            "--ttl" => {
                ttl = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(config::get().ttl)
            }
            "--signal" => {
                signal = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_SIGNAL)
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return;
            }
            "-V" | "--version" => {
                println!("lazysubs-eye {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("argumento desconocido: {other}\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }
    diagnostics::set_verbose(verbose);

    if mode == "tui" {
        if let Err(e) = tui::run() {
            eprintln!(
                "error en la TUI: {}",
                diagnostics::sanitize_error(format!("{e:#}"))
            );
            std::process::exit(1);
        }
        return;
    }

    if mode == "install" || mode == "uninstall" {
        let result = if mode == "install" {
            if let Some(root) = sandbox.as_deref() {
                install::install_sandbox(root, signal, dry_run).map(|plan| {
                    if dry_run {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&plan).unwrap_or_else(|_| "{}".into())
                        );
                    }
                })
            } else if dry_run {
                install::install_dry_run(signal)
            } else {
                install::install(signal)
            }
        } else {
            install::uninstall()
        };
        if let Err(e) = result {
            eprintln!(
                "error en {mode}: {}",
                diagnostics::sanitize_error(format!("{e:#}"))
            );
            std::process::exit(1);
        }
        return;
    }

    if mode == "doctor" {
        let code = doctor(doctor_json);
        if code != 0 {
            std::process::exit(code);
        }
        return;
    }

    diagnostics::verbose(if use_cache {
        "decisión de refresh: intentar caché"
    } else {
        "decisión de refresh: forzado por --no-cache"
    });
    // [CACHE] A valid snapshot avoids provider I/O; a miss performs one bounded
    // collection cycle, then persists it before notifications and history ingest.
    let cached = if use_cache { cache::load(ttl) } else { None };
    diagnostics::verbose(if cached.is_some() {
        "checkpoint de caché: hit"
    } else {
        "checkpoint de caché: miss; iniciando collectors"
    });
    let status = cached.unwrap_or_else(|| {
        let budget = performance::PerformanceBudget::default();
        let mut fresh = None;
        if let Err(error) = performance::measure_budget(
            std::time::Duration::from_millis(budget.refresh_global_ms),
            0,
            || fresh = Some(providers::collect_all()),
        ) {
            diagnostics::verbose(error);
        }
        let fresh = fresh.expect("el collector se ejecuta exactamente una vez");
        cache::save(&fresh);
        diagnostics::verbose("checkpoint de caché: estado persistido");
        notify::check(&fresh);
        history::ingest_today();
        fresh
    });

    match mode {
        "waybar" => println!("{}", output::waybar(&status)),
        "check" => std::process::exit(check(&status)),
        _ => println!("{}", output::pretty(&status)),
    }
}

/// [API] LOCAL HEALTH REPORT
///
/// Performs read-only checks over configuration, local state, and optional
/// executables. The report uses stable machine-readable states for CLI and JSON.
fn doctor(json: bool) -> i32 {
    let config_path = config::config_file();
    let config = config_path.as_ref().map(|p| p.exists()).unwrap_or(false);
    let notify = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("notify-send").is_file()))
        .unwrap_or(false);
    let cache = cache::dir().is_dir();
    let loaded = config::get();
    let validation = config::load_errors();
    use diagnostics::{CheckState, DoctorCheck, DoctorReport, LazysubsError};
    let providers: Vec<&str> = [
        loaded.providers.claude.then_some("claude"),
        loaded.providers.codex.then_some("codex"),
        loaded.providers.minimax.then_some("minimax"),
    ]
    .into_iter()
    .flatten()
    .collect();
    let cache_dir = cache::dir();
    let cache_permissions_ok = private_permissions(&cache_dir);
    let history_db = history::database_path();
    let database_state = match history_db.as_ref() {
        Some(path) if path.exists() => match rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(conn)
                if conn
                    .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
                    .is_ok() =>
            {
                (CheckState::Pass, "history.db accesible".to_string())
            }
            _ => (
                CheckState::Fail,
                "E006: no se puede leer history.db; comprueba permisos".into(),
            ),
        },
        _ => (
            CheckState::Warn,
            "history.db se creará al ingerir datos".into(),
        ),
    };
    let collector_indexes = index_health(&[
        cache::pi_daily_index_file(),
        cache::opencode_daily_index_file(),
    ]);
    let last_error = diagnostics::last_error();
    let checks = vec![
        DoctorCheck {
            name: "config",
            state: if validation.is_empty() {
                CheckState::Pass
            } else {
                CheckState::Fail
            },
            message: if validation.is_empty() {
                if config {
                    "ok".into()
                } else {
                    "no creada (opcional)".into()
                }
            } else {
                format!(
                    "{}: {}",
                    LazysubsError::ConfigValidationError,
                    validation.join("; ")
                )
            },
        },
        DoctorCheck {
            name: "providers",
            state: if providers.is_empty() {
                CheckState::Fail
            } else {
                CheckState::Pass
            },
            message: if providers.is_empty() {
                LazysubsError::ProviderNotConfigured.to_string()
            } else {
                format!("configurados: {}", providers.join(", "))
            },
        },
        DoctorCheck {
            name: "data-paths",
            state: if cache {
                CheckState::Pass
            } else {
                CheckState::Warn
            },
            message: if cache {
                "ok".into()
            } else {
                "se creará al primer refresco".into()
            },
        },
        DoctorCheck {
            name: "binary",
            state: CheckState::Pass,
            message: format!(
                "lazysubs-eye {} ({})",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::ARCH
            ),
        },
        DoctorCheck {
            name: "database",
            state: database_state.0,
            message: database_state.1,
        },
        DoctorCheck {
            name: "collector-indexes",
            state: collector_indexes.0,
            message: collector_indexes.1,
        },
        DoctorCheck {
            name: "permissions",
            state: if cache_permissions_ok {
                CheckState::Pass
            } else {
                CheckState::Warn
            },
            message: if cache_permissions_ok {
                "estado local privado".into()
            } else {
                "el directorio aún no existe o requiere permisos 0700".into()
            },
        },
        DoctorCheck {
            name: "last-error",
            state: if last_error.is_none() && validation.is_empty() {
                CheckState::Pass
            } else {
                CheckState::Warn
            },
            message: last_error
                .map(|event| format!("{}: {}", event.code, event.message))
                .or_else(|| validation.first().cloned())
                .unwrap_or_else(|| "ninguno conocido".into()),
        },
        DoctorCheck {
            name: "notify-send",
            state: if notify {
                CheckState::Pass
            } else {
                CheckState::Warn
            },
            message: if notify {
                "ok".into()
            } else {
                LazysubsError::NotifySendNotFound.to_string()
            },
        },
    ];
    let report = DoctorReport {
        version: env!("CARGO_PKG_VERSION"),
        checks,
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into())
        );
    } else {
        for check in &report.checks {
            println!("{:?} {}: {}", check.state, check.name, check.message);
        }
    }
    report.exit_code()
}

/// [CACHE] COLLECTOR INDEX HEALTH
///
/// Requires every existing index to parse with its schema marker and retain
/// private permissions. One invalid index fails the check because partial trust
/// would hide a local data-safety problem.
fn index_health(paths: &[std::path::PathBuf]) -> (diagnostics::CheckState, String) {
    let existing: Vec<_> = paths.iter().filter(|path| path.exists()).collect();
    if existing.is_empty() {
        return (
            diagnostics::CheckState::Warn,
            "los índices se crearán al escanear Pi/OpenCode".into(),
        );
    }
    for path in existing {
        let valid = std::fs::read(path)
            .ok()
            .and_then(|raw| serde_json::from_slice::<serde_json::Value>(&raw).ok())
            .is_some_and(|value| value.get("schema_version").is_some());
        if !valid || !private_permissions(path) {
            return (
                diagnostics::CheckState::Fail,
                "E006: índice local corrupto o con permisos inseguros; elimínalo para reconstruirlo"
                    .into(),
            );
        }
    }
    (
        diagnostics::CheckState::Pass,
        "índices presentes, parseables y privados".into(),
    )
}

#[cfg(unix)]
fn private_permissions(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o077 == 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn private_permissions(path: &std::path::Path) -> bool {
    path.exists()
}

/// [API] SCRIPT CHECK MODE
///
/// Emits one summary line per finding and returns the highest severity as the
/// exit code, making the command safe for scripts and hooks.
fn check(status: &providers::Status) -> i32 {
    if !config::load_errors().is_empty() {
        println!("error     configuración inválida; ejecuta `lazysubs-eye doctor`");
        return 3;
    }
    let config = config::get();
    let mut code = if status.providers.is_empty() { 3 } else { 0 };
    if status.providers.is_empty() {
        println!("error     no hay providers configurados o disponibles");
    }
    for provider in &status.providers {
        if let Some(err) = &provider.error {
            println!("error     {} — {err}", provider.name);
            code = code.max(3);
        }
        if let Some(since) = provider.stale_since {
            println!("warning   {} — datos stale desde {since}", provider.name);
            code = code.max(1);
        }
        for window in &provider.windows {
            let (level, exit) = if window.used_percent >= config.critical_at {
                ("critical", 2)
            } else if window.used_percent >= config.warning_at {
                ("warning", 1)
            } else {
                continue;
            };
            let reset = window
                .resets_at
                .map(|t| format!(" → {}", output::countdown(t)))
                .unwrap_or_default();
            println!(
                "{level:<9} {} {} {:.0}%{reset}",
                provider.name, window.label, window.used_percent
            );
            code = code.max(exit);
        }
    }
    if code == 0 {
        println!("ok");
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(stale_since: Option<i64>, error: Option<&str>) -> providers::Status {
        providers::Status {
            fetched_at: 1,
            providers: vec![providers::ProviderStatus {
                id: "p".into(),
                name: "Provider".into(),
                icon: "*".into(),
                plan: None,
                account: None,
                windows: vec![],
                reset_credits_available: None,
                stale_since,
                error: error.map(str::to_owned),
            }],
        }
    }

    #[test]
    fn check_uses_only_the_documented_exit_codes() {
        assert_eq!(
            check(&providers::Status {
                fetched_at: 1,
                providers: vec![]
            }),
            3
        );
        assert_eq!(check(&status(Some(1), None)), 1);
        assert_eq!(check(&status(None, Some("falló"))), 3);
    }
}
