//! Subcomandos `install` / `uninstall`: integración con waybar y Hyprland.
//!
//! Edita los configs del usuario por texto (no se reserializa el JSONC, así
//! se conservan sus comentarios y formato). Todo lo insertado va delimitado
//! por marcadores `lazysubs-eye-begin` / `lazysubs-eye-end` (o `// lazysubs-eye` en
//! líneas sueltas) para que `uninstall` pueda revertirlo con seguridad.
//! Antes de tocar un fichero se guarda un backup `<fichero>.bak.<epoch>`.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const MODULE_KEY: &str = "custom/ai-usage";
const WINDOWRULE: &str = "windowrule = tag +floating-window, match:class org.omarchy.lazysubs-eye";

struct ConfigPaths {
    waybar_config: PathBuf,
    waybar_style: PathBuf,
    hyprland_conf: PathBuf,
}

fn config_paths() -> Result<ConfigPaths> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("ni XDG_CONFIG_HOME ni HOME están definidos")?;
    Ok(ConfigPaths {
        waybar_config: config_home.join("waybar/config.jsonc"),
        waybar_style: config_home.join("waybar/style.css"),
        hyprland_conf: config_home.join("hypr/hyprland.conf"),
    })
}

/// Ruta del binario para el `exec` del módulo. Si cae bajo $HOME se escribe
/// con `$HOME` literal (waybar lo expande vía shell) para que el config sea
/// portable entre máquinas.
fn exec_path() -> String {
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok());
    let home = std::env::var("HOME").ok();
    match (exe, home) {
        (Some(exe), Some(home)) => {
            let exe = exe.to_string_lossy().into_owned();
            match exe.strip_prefix(&home) {
                Some(rest) => format!("$HOME{rest}"),
                None => exe,
            }
        }
        (Some(exe), None) => exe.to_string_lossy().into_owned(),
        (None, _) => "lazysubs-eye".to_string(),
    }
}

fn module_definition(exec: &str, signal: u8) -> String {
    format!(
        r#"  // lazysubs-eye-begin
  "{MODULE_KEY}": {{
    "exec": "{exec} --waybar",
    "return-type": "json",
    "interval": 60,
    "signal": {signal},
    "on-click": "omarchy-launch-or-focus-tui lazysubs-eye",
    "on-click-right": "{exec} --no-cache --waybar >/dev/null && pkill -RTMIN+{signal} waybar"
  }}"#
    )
}

fn style_block() -> String {
    // Colores neutros: heredan del tema activo de Omarchy vía @foreground
    // (definido por el @import del tema en style.css). warning/critical llevan
    // hex propios porque los temas solo exponen foreground/background.
    "\n/* lazysubs-eye-begin */\n\
     #custom-ai-usage {\n  margin: 0 8px;\n}\n\n\
     #custom-ai-usage.warning {\n  color: #e5c07b;\n}\n\n\
     #custom-ai-usage.critical {\n  color: #e06c75;\n}\n\n\
     #custom-ai-usage.error {\n  color: alpha(@foreground, 0.6);\n}\n\
     /* lazysubs-eye-end */\n"
        .to_string()
}

/// Inserta la entrada en `modules-right` y la definición del módulo en el
/// config JSONC de waybar. Devuelve `None` si la estructura no es la esperada
/// (en ese caso el llamador imprime el snippet para instalación manual).
fn waybar_config_with_module(config: &str, module_def: &str) -> Option<String> {
    let key_pos = config.find("\"modules-right\"")?;
    let open = key_pos + config[key_pos..].find('[')?;
    let close = open + find_matching_bracket(&config[open..])?;

    // Primero la inserción en la posición más tardía para no invalidar índices.
    let after_close = config[close + 1..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())?;
    let (def_at, def_text) = match after_close.1 {
        // "modules-right": [...] , → la definición va tras la coma, con coma final
        ',' => (
            close + 1 + after_close.0 + 1,
            format!("\n{module_def},\n  // lazysubs-eye-end"),
        ),
        // "modules-right": [...] } → era el último miembro: coma antes, sin coma final
        '}' => (close + 1, format!(",\n{module_def}\n  // lazysubs-eye-end")),
        _ => return None,
    };
    let mut out = String::with_capacity(config.len() + def_text.len() + 64);
    out.push_str(&config[..def_at]);
    out.push_str(&def_text);
    out.push_str(&config[def_at..]);

    // Entrada al principio de modules-right (con coma solo si el array no está vacío).
    let array_empty = config[open + 1..close].trim().is_empty();
    let entry = if array_empty {
        format!("\n    \"{MODULE_KEY}\" // lazysubs-eye\n  ")
    } else {
        format!("\n    \"{MODULE_KEY}\", // lazysubs-eye")
    };
    out.insert_str(open + 1, &entry);
    Some(out)
}

/// Offset del `]` que cierra el `[` en el que empieza `s`, saltando strings.
fn find_matching_bracket(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Elimina las líneas marcadas con `lazysubs-eye` y los bloques
/// `lazysubs-eye-begin`…`lazysubs-eye-end` (inclusive). Devuelve el texto limpio y si
/// hubo cambios.
fn strip_marked(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut in_block = false;
    let mut changed = false;
    for line in text.split_inclusive('\n') {
        if in_block {
            changed = true;
            if line.contains("lazysubs-eye-end") {
                in_block = false;
            }
            continue;
        }
        if line.contains("lazysubs-eye-begin") {
            in_block = true;
            changed = true;
            continue;
        }
        if line.contains("// lazysubs-eye") || line.contains("# lazysubs-eye") {
            changed = true;
            continue;
        }
        out.push_str(line);
    }
    (out, changed)
}

fn backup(path: &Path) -> Result<PathBuf> {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let bak = path.with_extension(format!(
        "{}bak.{epoch}",
        path.extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    std::fs::copy(path, &bak).with_context(|| format!("no pude crear el backup de {path:?}"))?;
    Ok(bak)
}

fn write_with_backup(path: &Path, contents: &str) -> Result<()> {
    let bak = backup(path)?;
    // Escritura atómica: waybar vigila estos ficheros (reload_style_on_change)
    // y el propio install reinicia servicios justo después; un write no
    // atómico puede dejar que lean el fichero a medias.
    crate::cache::atomic_save(path, contents.as_bytes())
        .with_context(|| format!("no pude escribir {path:?}"))?;
    println!("  ✓ {} (backup: {})", path.display(), bak.display());
    Ok(())
}

fn reload() {
    println!("recargando…");
    let waybar = Command::new("omarchy").args(["restart", "waybar"]).output();
    if !waybar.map(|o| o.status.success()).unwrap_or(false) {
        println!("  ⚠ no pude ejecutar `omarchy restart waybar`; reinicia waybar a mano");
    }
    let hypr = Command::new("hyprctl").arg("reload").output();
    if hypr.map(|o| o.status.success()).unwrap_or(false) {
        if let Ok(out) = Command::new("hyprctl").arg("configerrors").output() {
            let errors = String::from_utf8_lossy(&out.stdout);
            let errors = errors.trim();
            if !errors.is_empty() && !errors.contains("no errors") {
                println!("  ⚠ hyprctl configerrors:\n{errors}");
            }
        }
    } else {
        println!("  ⚠ no pude ejecutar `hyprctl reload`; recarga Hyprland a mano");
    }
}

pub fn install(signal: u8) -> Result<()> {
    if !(1..=30).contains(&signal) {
        bail!("--signal debe estar entre 1 y 30 (RTMIN+N)");
    }
    let paths = config_paths()?;
    let exec = exec_path();
    let mut changed = false;

    // waybar config.jsonc
    let config = std::fs::read_to_string(&paths.waybar_config)
        .with_context(|| format!("no pude leer {:?}", paths.waybar_config))?;
    if config.contains(MODULE_KEY) {
        println!(
            "  · módulo waybar ya presente, no toco {}",
            paths.waybar_config.display()
        );
    } else {
        match waybar_config_with_module(&config, &module_definition(&exec, signal)) {
            Some(updated) => {
                write_with_backup(&paths.waybar_config, &updated)?;
                changed = true;
            }
            None => {
                println!(
                    "  ⚠ no reconozco la estructura de {}; añade esto a mano:\n\n\
                     \"{MODULE_KEY}\" en modules-right, y el módulo:\n{}\n",
                    paths.waybar_config.display(),
                    module_definition(&exec, signal)
                );
            }
        }
    }

    // waybar style.css
    let style = std::fs::read_to_string(&paths.waybar_style)
        .with_context(|| format!("no pude leer {:?}", paths.waybar_style))?;
    if style.contains("#custom-ai-usage") {
        println!(
            "  · estilos waybar ya presentes, no toco {}",
            paths.waybar_style.display()
        );
    } else {
        write_with_backup(&paths.waybar_style, &format!("{style}{}", style_block()))?;
        changed = true;
    }

    // hyprland.conf (ventana flotante para la TUI)
    let hypr = std::fs::read_to_string(&paths.hyprland_conf)
        .with_context(|| format!("no pude leer {:?}", paths.hyprland_conf))?;
    if hypr.contains("org.omarchy.lazysubs-eye") {
        println!(
            "  · windowrule ya presente, no toco {}",
            paths.hyprland_conf.display()
        );
    } else {
        let sep = if hypr.ends_with('\n') { "" } else { "\n" };
        write_with_backup(
            &paths.hyprland_conf,
            &format!("{hypr}{sep}\n{WINDOWRULE} # lazysubs-eye\n"),
        )?;
        changed = true;
    }

    if changed {
        reload();
        println!("listo. El módulo aparece en waybar; click izquierdo abre la TUI flotante.");
    } else {
        println!("nada que hacer: la integración ya estaba completa.");
    }
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let paths = config_paths()?;
    let mut changed = false;

    for path in [&paths.waybar_config, &paths.waybar_style] {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let (clean, modified) = strip_marked(&text);
        if modified {
            write_with_backup(path, &clean)?;
            changed = true;
        }
        let leftover = if path == &paths.waybar_config {
            MODULE_KEY
        } else {
            "#custom-ai-usage"
        };
        if clean.contains(leftover) {
            println!(
                "  ⚠ {} contiene una integración instalada a mano (sin marcadores); \
                 elimina \"{leftover}\" manualmente",
                path.display()
            );
        }
    }

    if let Ok(text) = std::fs::read_to_string(&paths.hyprland_conf) {
        let kept: String = text
            .split_inclusive('\n')
            .filter(|l| !l.contains("org.omarchy.lazysubs-eye"))
            .collect();
        if kept.len() != text.len() {
            write_with_backup(&paths.hyprland_conf, &kept)?;
            changed = true;
        }
    }

    if changed {
        reload();
        println!("integración revertida.");
    } else {
        println!("nada que revertir.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STOCK: &str = r#"{
  "reload_style_on_change": true,
  "modules-left": ["custom/omarchy"],
  "modules-right": [
    "group/tray-expander",
    "bluetooth",
    "battery"
  ],
  "battery": {
    "format": "{icon}"
  }
}"#;

    #[test]
    fn inserta_en_config_stock() {
        let def = module_definition("$HOME/.local/bin/lazysubs-eye", 11);
        let out = waybar_config_with_module(STOCK, &def).unwrap();
        assert!(out.contains("\"custom/ai-usage\", // lazysubs-eye"));
        assert!(out.contains("// lazysubs-eye-begin"));
        assert!(out.contains("\"signal\": 11,"));
        // la entrada queda la primera del array
        let entry = out.find("\"custom/ai-usage\",").unwrap();
        let tray = out.find("\"group/tray-expander\"").unwrap();
        assert!(entry < tray);
        // sigue siendo JSON válido una vez quitados los comentarios
        let json: String = out
            .lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
        serde_json::from_str::<serde_json::Value>(&json).expect("JSON inválido tras insertar");
    }

    #[test]
    fn inserta_cuando_modules_right_es_el_ultimo_miembro() {
        let config = "{\n  \"modules-right\": [\n    \"battery\"\n  ]\n}";
        let def = module_definition("lazysubs-eye", 11);
        let out = waybar_config_with_module(config, &def).unwrap();
        let json: String = out
            .lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
        serde_json::from_str::<serde_json::Value>(&json).expect("JSON inválido tras insertar");
    }

    #[test]
    fn inserta_en_array_vacio_sin_coma_colgante() {
        let config = "{\n  \"modules-right\": [],\n  \"clock\": {}\n}";
        let out =
            waybar_config_with_module(config, &module_definition("lazysubs-eye", 11)).unwrap();
        assert!(out.contains("\"custom/ai-usage\" // lazysubs-eye"));
    }

    #[test]
    fn config_sin_modules_right_devuelve_none() {
        assert!(waybar_config_with_module("{}", "x").is_none());
    }

    #[test]
    fn strip_revierte_la_insercion() {
        let def = module_definition("lazysubs-eye", 11);
        let out = waybar_config_with_module(STOCK, &def).unwrap();
        let (clean, changed) = strip_marked(&out);
        assert!(changed);
        assert_eq!(clean, STOCK);
    }

    #[test]
    fn strip_sin_marcadores_no_cambia_nada() {
        let (clean, changed) = strip_marked(STOCK);
        assert!(!changed);
        assert_eq!(clean, STOCK);
    }

    #[test]
    fn strip_elimina_bloque_css() {
        let style = format!("* {{ color: red; }}\n{}", style_block());
        let (clean, changed) = strip_marked(&style);
        assert!(changed);
        assert!(!clean.contains("custom-ai-usage"));
        assert!(clean.contains("* { color: red; }"));
    }

    #[test]
    fn brackets_anidados_y_strings_con_corchetes() {
        let s = r#"[ "a]b", ["c"], "d" ]x"#;
        assert_eq!(find_matching_bracket(s), Some(s.len() - 2));
    }
}
