mod cache;
mod opencode_tokens;
mod output;
mod pi_tokens;
mod providers;
mod tokens;
mod tui;

use std::io::IsTerminal;

const DEFAULT_TTL_SECS: i64 = 60;

const HELP: &str = "\
lazysubs — monitor de cuotas de suscripciones de IA (Claude Code, Codex)

Uso: lazysubs [tui|--json|--waybar] [--no-cache] [--ttl <segundos>]

  tui         interfaz de terminal (por defecto si stdout es una tty)
  --json      volcado completo del estado (por defecto sin tty)
  --waybar    JSON de una línea para un módulo custom de waybar
  --no-cache  fuerza una consulta fresca a los providers
  --ttl N     validez de la cache en segundos (por defecto 60)
";

fn main() {
    let mut mode = if std::io::stdout().is_terminal() {
        "tui"
    } else {
        "json"
    };
    let mut use_cache = true;
    let mut ttl = DEFAULT_TTL_SECS;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "tui" | "--tui" => mode = "tui",
            "--json" => mode = "json",
            "--waybar" => mode = "waybar",
            "--no-cache" => use_cache = false,
            "--ttl" => {
                ttl = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_TTL_SECS)
            }
            "-h" | "--help" => {
                print!("{HELP}");
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

    let status = if use_cache { cache::load(ttl) } else { None }.unwrap_or_else(|| {
        let fresh = providers::collect_all();
        cache::save(&fresh);
        fresh
    });

    match mode {
        "waybar" => println!("{}", output::waybar(&status)),
        _ => println!("{}", output::pretty(&status)),
    }
}
