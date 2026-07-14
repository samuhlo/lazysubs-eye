mod cache;
mod config;
mod install;
mod notify;
mod opencode_tokens;
mod output;
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
  install     integra lazysubs-eye en waybar y Hyprland (idempotente, con backups)
  uninstall   revierte la integración
  --json      volcado completo del estado (por defecto sin tty)
  --waybar    JSON de una línea para un módulo custom de waybar
  --check     resume el estado y sale con 0 ok / 1 warning / 2 critical / 3 error
  --no-cache  fuerza una consulta fresca a los providers
  --ttl N     validez de la cache en segundos (por defecto 60)
  --signal N  señal RTMIN+N del módulo waybar en install (por defecto 11)
  --version   muestra la versión

Config opcional en ~/.config/lazysubs-eye/config.toml (umbrales, ttl,
providers, iconos, notificaciones); ver README.
";

fn main() {
    let mut mode = if std::io::stdout().is_terminal() {
        "tui"
    } else {
        "json"
    };
    let mut use_cache = true;
    let mut ttl = config::get().ttl;
    let mut signal = DEFAULT_SIGNAL;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "tui" | "--tui" => mode = "tui",
            "install" => mode = "install",
            "uninstall" => mode = "uninstall",
            "--json" => mode = "json",
            "--waybar" => mode = "waybar",
            "--check" => mode = "check",
            "--no-cache" => use_cache = false,
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

    if mode == "tui" {
        if let Err(e) = tui::run() {
            eprintln!("error en la TUI: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    if mode == "install" || mode == "uninstall" {
        let result = if mode == "install" {
            install::install(signal)
        } else {
            install::uninstall()
        };
        if let Err(e) = result {
            eprintln!("error en {mode}: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    let status = if use_cache { cache::load(ttl) } else { None }.unwrap_or_else(|| {
        let fresh = providers::collect_all();
        cache::save(&fresh);
        notify::check(&fresh);
        fresh
    });

    match mode {
        "waybar" => println!("{}", output::waybar(&status)),
        "check" => std::process::exit(check(&status)),
        _ => println!("{}", output::pretty(&status)),
    }
}

/// Modo `--check` para scripts y hooks: imprime un resumen de una línea por
/// hallazgo y devuelve el peor nivel como exit code.
fn check(status: &providers::Status) -> i32 {
    let config = config::get();
    let mut code = 0;
    for provider in &status.providers {
        if let Some(err) = &provider.error {
            println!("error     {} — {err}", provider.name);
            code = code.max(3);
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
