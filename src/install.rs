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

/// Omarchy detectado: existe `~/.local/share/omarchy` o hay `omarchy` en PATH.
/// Guía todos los fallbacks (CSS, on-click, recarga, windowrule).
fn is_omarchy() -> bool {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if data.map(|d| d.join("omarchy").exists()).unwrap_or(false) {
        return true;
    }
    which("omarchy").is_some()
}

/// Primer directorio del PATH que contiene un ejecutable `name`.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// El config de waybar puede llamarse `config.jsonc` (Omarchy) o `config` a
/// secas (nombre estándar de waybar). Se prefiere el que exista; si no existe
/// ninguno, se devuelve la ruta `.jsonc` para que el error sea claro.
fn resolve_waybar_config(waybar_dir: &Path) -> PathBuf {
    let jsonc = waybar_dir.join("config.jsonc");
    let plain = waybar_dir.join("config");
    if jsonc.exists() {
        jsonc
    } else if plain.exists() {
        plain
    } else {
        jsonc
    }
}

fn config_paths() -> Result<ConfigPaths> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("ni XDG_CONFIG_HOME ni HOME están definidos")?;
    let waybar_dir = config_home.join("waybar");
    Ok(ConfigPaths {
        waybar_config: resolve_waybar_config(&waybar_dir),
        waybar_style: waybar_dir.join("style.css"),
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

/// Comando terminal para lanzar la TUI fuera de Omarchy. Preferimos el
/// estándar freedesktop `xdg-terminal-exec`; si no, el primer terminal conocido
/// con su invocación correcta. `None` = ningún terminal detectado.
fn fallback_launch(has_xdg_term: bool, terminal: Option<&str>) -> Option<String> {
    if has_xdg_term {
        return Some("xdg-terminal-exec lazysubs-eye".into());
    }
    // foot y kitty aceptan el comando directo; alacritty y ghostty usan -e.
    match terminal? {
        "foot" => Some("foot lazysubs-eye".into()),
        "kitty" => Some("kitty lazysubs-eye".into()),
        term => Some(format!("{term} -e lazysubs-eye")),
    }
}

fn detect_launch() -> Option<String> {
    let has_xdg = which("xdg-terminal-exec").is_some();
    let terminal = ["foot", "alacritty", "kitty", "ghostty"]
        .into_iter()
        .find(|t| which(t).is_some());
    fallback_launch(has_xdg, terminal)
}

/// Comando `on-click` de la TUI: launch-or-focus en Omarchy, terminal genérico
/// fuera. `None` si no hay forma de abrir (se instala sin on-click).
fn on_click(omarchy: bool) -> Option<String> {
    if omarchy {
        Some("omarchy-launch-or-focus-tui lazysubs-eye".into())
    } else {
        detect_launch()
    }
}

fn module_definition(exec: &str, signal: u8, on_click: Option<&str>) -> String {
    let click_line = on_click
        .map(|c| format!("\n    \"on-click\": \"{c}\","))
        .unwrap_or_default();
    format!(
        r#"  // lazysubs-eye-begin
  "{MODULE_KEY}": {{
    "exec": "{exec} --waybar",
    "return-type": "json",
    "interval": 60,
    "signal": {signal},{click_line}
    "on-click-right": "{exec} --no-cache --waybar >/dev/null && pkill -RTMIN+{signal} waybar"
  }}"#
    )
}

fn style_block(omarchy: bool) -> String {
    // La clase `error` usa `alpha(@foreground, …)` en Omarchy (el @import del
    // tema define @foreground); fuera de Omarchy no existe esa variable, así que
    // se usa un gris hex neutro. warning/critical llevan hex propios siempre.
    let error_color = if omarchy {
        "alpha(@foreground, 0.6)"
    } else {
        "#9a9a9a"
    };
    // Sin salto inicial: cada línea del bloque lleva marcador o queda dentro de
    // begin…end, de modo que `uninstall` lo revierta byte a byte (el separador
    // con el CSS previo lo pone install según haga falta).
    format!(
        "/* lazysubs-eye-begin */\n\
         #custom-ai-usage {{\n  margin: 0 8px;\n}}\n\n\
         #custom-ai-usage.warning {{\n  color: #e5c07b;\n}}\n\n\
         #custom-ai-usage.critical {{\n  color: #e06c75;\n}}\n\n\
         #custom-ai-usage.error {{\n  color: {error_color};\n}}\n\
         /* lazysubs-eye-end */\n"
    )
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

fn reload(omarchy: bool, touched_hypr: bool) {
    println!("recargando…");
    reload_waybar(omarchy);
    if touched_hypr {
        reload_hyprland();
    }
}

fn reload_waybar(omarchy: bool) {
    if omarchy {
        let ok = Command::new("omarchy")
            .args(["restart", "waybar"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            println!("  ⚠ no pude ejecutar `omarchy restart waybar`; reinicia waybar a mano");
        }
        return;
    }
    // Fuera de Omarchy: SIGUSR2 hace que waybar recargue su config. Si no hay
    // proceso, probamos el servicio de usuario; si tampoco, lo decimos.
    let signaled = Command::new("pkill")
        .args(["-SIGUSR2", "waybar"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if signaled {
        return;
    }
    let restarted = Command::new("systemctl")
        .args(["--user", "try-restart", "waybar.service"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !restarted {
        println!("  ⚠ waybar no parece estar corriendo; arráncalo para ver el módulo");
    }
}

fn reload_hyprland() {
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
    let omarchy = is_omarchy();
    let mut changed = false;
    if !omarchy {
        println!("  · Omarchy no detectado: uso CSS neutro y fallbacks genéricos de waybar");
    }

    // waybar config (config.jsonc en Omarchy, o `config` a secas)
    let config = std::fs::read_to_string(&paths.waybar_config)
        .with_context(|| format!("no pude leer {:?}", paths.waybar_config))?;
    let click = on_click(omarchy);
    if click.is_none() {
        println!(
            "  ⚠ sin terminal detectado para abrir la TUI; instalo el módulo sin on-click \
             (instala xdg-terminal-exec o un terminal como foot/alacritty/kitty/ghostty)"
        );
    }
    if config.contains(MODULE_KEY) {
        println!(
            "  · módulo waybar ya presente, no toco {}",
            paths.waybar_config.display()
        );
    } else {
        match waybar_config_with_module(
            &config,
            &module_definition(&exec, signal, click.as_deref()),
        ) {
            Some(updated) => {
                write_with_backup(&paths.waybar_config, &updated)?;
                changed = true;
            }
            None => {
                println!(
                    "  ⚠ no reconozco la estructura de {}; añade esto a mano:\n\n\
                     \"{MODULE_KEY}\" en modules-right, y el módulo:\n{}\n",
                    paths.waybar_config.display(),
                    module_definition(&exec, signal, click.as_deref())
                );
            }
        }
    }

    // waybar style.css (CSS neutro fuera de Omarchy: sin @foreground)
    let style = std::fs::read_to_string(&paths.waybar_style)
        .with_context(|| format!("no pude leer {:?}", paths.waybar_style))?;
    if style.contains("#custom-ai-usage") {
        println!(
            "  · estilos waybar ya presentes, no toco {}",
            paths.waybar_style.display()
        );
    } else {
        let sep = if style.is_empty() || style.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        write_with_backup(
            &paths.waybar_style,
            &format!("{style}{sep}{}", style_block(omarchy)),
        )?;
        changed = true;
    }

    // hyprland.conf: la windowrule flotante solo si Hyprland está configurado.
    // En otros compositores (sway, river…) se omite (ver README).
    let mut touched_hypr = false;
    if paths.hyprland_conf.exists() {
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
            touched_hypr = true;
        }
    } else {
        println!(
            "  · sin ~/.config/hypr/hyprland.conf: omito la windowrule flotante \
             (si usas sway/river, mira \"Other Linux setups\" en el README)"
        );
    }

    if changed {
        reload(omarchy, touched_hypr);
        println!("listo. El módulo aparece en waybar; click izquierdo abre la TUI.");
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

    let mut touched_hypr = false;
    if let Ok(text) = std::fs::read_to_string(&paths.hyprland_conf) {
        let kept: String = text
            .split_inclusive('\n')
            .filter(|l| !l.contains("org.omarchy.lazysubs-eye"))
            .collect();
        if kept.len() != text.len() {
            write_with_backup(&paths.hyprland_conf, &kept)?;
            changed = true;
            touched_hypr = true;
        }
    }

    if changed {
        reload(is_omarchy(), touched_hypr);
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
        let def = module_definition(
            "$HOME/.local/bin/lazysubs-eye",
            11,
            Some("omarchy-launch-or-focus-tui lazysubs-eye"),
        );
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
        let def = module_definition("lazysubs-eye", 11, None);
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
        let out = waybar_config_with_module(config, &module_definition("lazysubs-eye", 11, None))
            .unwrap();
        assert!(out.contains("\"custom/ai-usage\" // lazysubs-eye"));
    }

    #[test]
    fn config_sin_modules_right_devuelve_none() {
        assert!(waybar_config_with_module("{}", "x").is_none());
    }

    #[test]
    fn strip_revierte_la_insercion() {
        let def = module_definition("lazysubs-eye", 11, None);
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
    fn strip_elimina_bloque_css_y_revierte_byte_a_byte() {
        // install añade el bloque tras un CSS que termina en \n; uninstall debe
        // devolverlo idéntico (sin líneas en blanco colgando).
        for omarchy in [true, false] {
            let original = "* { color: red; }\n";
            let style = format!("{original}{}", style_block(omarchy));
            let (clean, changed) = strip_marked(&style);
            assert!(changed);
            assert!(!clean.contains("custom-ai-usage"));
            assert_eq!(
                clean, original,
                "round-trip byte a byte (omarchy={omarchy})"
            );
        }
    }

    #[test]
    fn brackets_anidados_y_strings_con_corchetes() {
        let s = r#"[ "a]b", ["c"], "d" ]x"#;
        assert_eq!(find_matching_bracket(s), Some(s.len() - 2));
    }

    #[test]
    fn fallback_launch_prefiere_xdg_y_conoce_los_terminales() {
        assert_eq!(
            fallback_launch(true, Some("alacritty")).as_deref(),
            Some("xdg-terminal-exec lazysubs-eye")
        );
        assert_eq!(
            fallback_launch(false, Some("foot")).as_deref(),
            Some("foot lazysubs-eye")
        );
        assert_eq!(
            fallback_launch(false, Some("kitty")).as_deref(),
            Some("kitty lazysubs-eye")
        );
        assert_eq!(
            fallback_launch(false, Some("alacritty")).as_deref(),
            Some("alacritty -e lazysubs-eye")
        );
        assert_eq!(
            fallback_launch(false, Some("ghostty")).as_deref(),
            Some("ghostty -e lazysubs-eye")
        );
        assert_eq!(fallback_launch(false, None), None);
    }

    #[test]
    fn style_block_neutro_fuera_de_omarchy() {
        let omarchy = style_block(true);
        assert!(omarchy.contains("alpha(@foreground, 0.6)"));

        let generic = style_block(false);
        assert!(
            !generic.contains("@foreground"),
            "sin @foreground: {generic}"
        );
        assert!(generic.contains("#9a9a9a"));
        // warning/critical llevan hex propios en ambos casos
        assert!(generic.contains("#e5c07b") && generic.contains("#e06c75"));
    }

    #[test]
    fn module_sin_on_click_sigue_siendo_json_valido() {
        let def = module_definition("lazysubs-eye", 11, None);
        assert!(!def.contains("on-click\""), "sin on-click de apertura");
        assert!(def.contains("on-click-right"));
        // envuelto como miembro de objeto y sin comentarios → JSON válido
        let json: String = format!("{{{}}}", def.replace("// lazysubs-eye-begin", ""))
            .lines()
            .map(|l| match l.find("//") {
                Some(i) => &l[..i],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
        serde_json::from_str::<serde_json::Value>(&json).expect("JSON válido sin on-click");
    }

    #[test]
    fn module_con_on_click_incluye_la_linea() {
        let def = module_definition("lazysubs-eye", 11, Some("foot lazysubs-eye"));
        assert!(def.contains("\"on-click\": \"foot lazysubs-eye\","));
    }

    #[test]
    fn resolve_waybar_config_prefiere_jsonc_luego_config() {
        let dir = std::env::temp_dir().join(format!("lazysubs-wb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // sin ficheros → default .jsonc
        assert_eq!(resolve_waybar_config(&dir), dir.join("config.jsonc"));

        // solo `config` (waybar estándar) → lo elige
        std::fs::write(dir.join("config"), "{}").unwrap();
        assert_eq!(resolve_waybar_config(&dir), dir.join("config"));

        // si además existe .jsonc, gana el .jsonc
        std::fs::write(dir.join("config.jsonc"), "{}").unwrap();
        assert_eq!(resolve_waybar_config(&dir), dir.join("config.jsonc"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
