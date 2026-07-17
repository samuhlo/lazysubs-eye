//! [CORE] WAYBAR AND HYPRLAND INTEGRATION
//!
//! FLOW: discover the user's XDG config → preflight readable/writable base files
//! → snapshot each target → atomically publish one owned edit → reload consumers.
//! If a later mutation fails, rollback copies snapshots in reverse mutation order.
//!
//! This module edits user config as text instead of reserializing JSONC: comments,
//! ordering, and unrelated formatting are user-owned and must survive untouched.
//! Every insertion carries `lazysubs-eye-begin` / `lazysubs-eye-end` markers (or
//! `// lazysubs-eye` on single lines). Those markers define uninstall's ownership
//! boundary; unmarked manual integration is reported, never deleted. A unique
//! `<file>.bak.<epoch>[.N]` snapshot is made before every attempted publish.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const MODULE_KEY: &str = "custom/ai-usage";
const WINDOWRULE: &str = "windowrule = tag +floating-window, match:class org.omarchy.lazysubs-eye";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallError {
    BinaryNotFound,
    WaybarConfigNotFound,
    WaybarStyleNotFound,
    ConfigNotWritable,
    OwnershipConflict,
    RollbackFailed(Vec<String>),
    MarkerMismatch,
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BinaryNotFound => write!(f, "no se pudo resolver el binario en ejecución"),
            Self::WaybarConfigNotFound => write!(f, "no existe la configuración de waybar"),
            Self::WaybarStyleNotFound => write!(f, "no existe style.css de waybar"),
            Self::ConfigNotWritable => write!(f, "la configuración de waybar no es escribible"),
            Self::OwnershipConflict => write!(f, "hay reglas manuales dentro de los marcadores"),
            Self::RollbackFailed(files) => write!(f, "rollback incompleto: {}", files.join(", ")),
            Self::MarkerMismatch => write!(f, "los marcadores están incompletos o editados"),
        }
    }
}

impl std::error::Error for InstallError {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct InstallPlan {
    pub files_to_modify: Vec<String>,
    pub backups_to_create: Vec<String>,
    pub commands_to_run: Vec<String>,
    pub files_to_delete: Vec<String>,
}

struct ConfigPaths {
    waybar_config: PathBuf,
    waybar_style: PathBuf,
    hyprland_conf: PathBuf,
}

/// [FLOW] MUTATION JOURNAL
///
/// Each successful backup records `(destination, snapshot)` before its atomic
/// replacement. On failure, restore in reverse mutation order: a later file may
/// have made an earlier file's new state observable to a reloaded integration.
/// FAILURE MODE: rollback is best-effort; callers retain both the original error
/// and the affected paths if restoring a snapshot also fails.
#[derive(Default)]
struct BackupManager {
    entries: Vec<(PathBuf, PathBuf)>,
}

impl BackupManager {
    fn new() -> Self {
        Self::default()
    }
    fn backup(&mut self, path: &Path) -> Result<PathBuf> {
        let copy = backup(path)?;
        self.entries.push((path.to_owned(), copy.clone()));
        Ok(copy)
    }

    #[allow(dead_code)] // [NOTE] Connected when the full plan uses the transaction executor.
    fn rollback(&mut self) -> std::result::Result<(), InstallError> {
        let mut failed = Vec::new();
        for (destination, copy) in self.entries.iter().rev() {
            if std::fs::copy(copy, destination).is_err() {
                failed.push(crate::diagnostics::sanitize_error(
                    destination.display().to_string(),
                ));
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(InstallError::RollbackFailed(failed))
        }
    }
}

/// [FLOW] INSTALL PREFLIGHT
///
/// Discovery resolves the expected XDG paths, then this phase proves Waybar's two
/// required base files exist and can be opened for writing before any backup exists.
/// Hyprland is optional by contract, so its absence is not an install failure.
/// FAILURE MODE: permission/open failures stop here; they must not leave backups,
/// generated module fragments, or a half-installed Waybar pair behind.
fn preflight_install(paths: &ConfigPaths) -> Vec<InstallError> {
    let mut issues = Vec::new();
    if !paths.waybar_config.is_file() {
        issues.push(InstallError::WaybarConfigNotFound);
    }
    if !paths.waybar_style.is_file() {
        issues.push(InstallError::WaybarStyleNotFound);
    }
    for path in [&paths.waybar_config, &paths.waybar_style] {
        if path.exists() && std::fs::OpenOptions::new().write(true).open(path).is_err() {
            issues.push(InstallError::ConfigNotWritable);
            break;
        }
    }
    issues
}

/// [FLOW] UNINSTALL PREFLIGHT
///
/// Removal is stricter only where ownership markers already exist. A malformed
/// marker pair or manual rule inside an owned block stops the transaction rather
/// than guessing which user text is safe to remove.
fn preflight_uninstall(paths: &ConfigPaths) -> Vec<InstallError> {
    let mut issues = Vec::new();
    for path in [
        &paths.waybar_config,
        &paths.waybar_style,
        &paths.hyprland_conf,
    ] {
        if !path.exists() {
            continue;
        }
        if std::fs::OpenOptions::new().write(true).open(path).is_err() {
            issues.push(InstallError::ConfigNotWritable);
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            if text.contains("lazysubs-eye-begin") || text.contains("lazysubs-eye-end") {
                if let Err(issue) = validate_markers(path) {
                    issues.push(issue);
                }
            }
        }
    }
    issues
}

fn require_preflight(paths: &ConfigPaths) -> Result<()> {
    let issues = preflight_install(paths);
    if issues.is_empty() {
        return Ok(());
    }
    bail!(
        "preflight falló: {}",
        issues
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ")
    )
}

/// [FLOW] OMARCHY CAPABILITY DETECTION
///
/// Detects `~/.local/share/omarchy` or the `omarchy` binary in PATH. This is a
/// behavior switch, not a requirement: it selects theme-aware CSS, Omarchy's
/// launch-or-focus action, and its Waybar restart command. Generic desktops keep
/// working through freedesktop/process fallbacks instead of inheriting Omarchy-only
/// assumptions.
fn is_omarchy() -> bool {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if data.map(|d| d.join("omarchy").exists()).unwrap_or(false) {
        return true;
    }
    which("omarchy").is_some()
}

/// [DATA] PATH LOOKUP
///
/// Returns the first PATH directory containing a regular file named `name`.
/// TRADE-OFF: this mirrors shell lookup without invoking a shell or parsing its
/// aliases. It is capability discovery only; the external command remains a user
/// environment boundary and may still fail when executed.
fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// [DATA] WAYBAR CONFIG RESOLUTION
///
/// Waybar configuration may be `config.jsonc` under Omarchy or standard `config`.
/// Prefer an existing file; otherwise return `.jsonc` so the missing-file error
/// points to the expected path.
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

/// [DATA] USER CONFIG ROOT
///
/// Prefer XDG_CONFIG_HOME, then the conventional `$HOME/.config`. All writes stay
/// below this caller-selected root; the installer does not create a system-wide
/// config or rewrite an Omarchy source tree. If neither environment variable is
/// available, path discovery fails before preflight or mutation.
fn config_paths() -> Result<ConfigPaths> {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .context("ni XDG_CONFIG_HOME ni HOME están definidos")?;
    Ok(config_paths_at(&config_home))
}

fn config_paths_at(config_home: &Path) -> ConfigPaths {
    let waybar_dir = config_home.join("waybar");
    ConfigPaths {
        waybar_config: resolve_waybar_config(&waybar_dir),
        waybar_style: waybar_dir.join("style.css"),
        hyprland_conf: config_home.join("hypr/hyprland.conf"),
    }
}

/// [API] MODULE EXECUTABLE PATH
///
/// Resolve the running binary before writing an `exec` value. `current_exe` is
/// canonicalized first to avoid persisting a transient symlink spelling; `/proc`
/// is only a platform fallback. The final path must name an executable regular file.
/// Paths below `$HOME` are stored with literal `$HOME`, which Waybar expands through
/// its shell, keeping copied user config portable across machines. This does not
/// attempt to sandbox Waybar's shell: the chosen binary is explicitly trusted input.
pub fn resolve_binary_path() -> std::result::Result<PathBuf, InstallError> {
    let path = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
        .or_else(|| std::fs::read_link("/proc/self/exe").ok())
        .filter(|path| path.is_file())
        .ok_or(InstallError::BinaryNotFound)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&path)
            .map(|metadata| metadata.permissions().mode() & 0o111 == 0)
            .unwrap_or(true)
        {
            return Err(InstallError::BinaryNotFound);
        }
    }
    Ok(path)
}

fn exec_path() -> std::result::Result<String, InstallError> {
    let exe = resolve_binary_path()?;
    let home = std::env::var("HOME").ok();
    Ok(match (exe, home) {
        (exe, Some(home)) => {
            let exe = exe.to_string_lossy().into_owned();
            match exe.strip_prefix(&home) {
                Some(rest) => format!("$HOME{rest}"),
                None => exe,
            }
        }
        (exe, None) => exe.to_string_lossy().into_owned(),
    })
}

/// [API] GENERIC TUI LAUNCH
///
/// Outside Omarchy, prefer freedesktop `xdg-terminal-exec`; otherwise use the
/// first known terminal with its required invocation. `None` means none found.
fn fallback_launch_for(exec: &str, has_xdg_term: bool, terminal: Option<&str>) -> Option<String> {
    if has_xdg_term {
        return Some(format!("xdg-terminal-exec {exec}"));
    }
    // [API] foot and kitty accept the command directly; alacritty and ghostty require -e.
    match terminal? {
        "foot" => Some(format!("foot {exec}")),
        "kitty" => Some(format!("kitty {exec}")),
        term => Some(format!("{term} -e {exec}")),
    }
}

fn detect_launch(exec: &str) -> Option<String> {
    let has_xdg = which("xdg-terminal-exec").is_some();
    let terminal = ["foot", "alacritty", "kitty", "ghostty"]
        .into_iter()
        .find(|t| which(t).is_some());
    fallback_launch_for(exec, has_xdg, terminal)
}

/// Produces the TUI `on-click` command: Omarchy launch-or-focus there, generic
/// terminal elsewhere. `None` installs the module without a click action.
fn on_click(omarchy: bool, exec: &str) -> Option<String> {
    if omarchy {
        Some(format!("omarchy-launch-or-focus-tui {exec}"))
    } else {
        detect_launch(exec)
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
    // [UI] Omarchy defines `@foreground` through its theme import; generic
    // Waybar does not, so error uses neutral hex. Warning and critical use hex everywhere.
    let error_color = if omarchy {
        "alpha(@foreground, 0.6)"
    } else {
        "#9a9a9a"
    };
    // [DATA] No leading newline: every line is marked or enclosed, so uninstall
    // can restore bytes exactly. `install` adds the prior-CSS separator when needed.
    format!(
        "/* lazysubs-eye-begin */\n\
         #custom-ai-usage {{\n  margin: 0 8px;\n}}\n\n\
         #custom-ai-usage.warning {{\n  color: #e5c07b;\n}}\n\n\
         #custom-ai-usage.critical {{\n  color: #e06c75;\n}}\n\n\
         #custom-ai-usage.error {{\n  color: {error_color};\n}}\n\
         /* lazysubs-eye-end */\n"
    )
}

/// [DATA] TEXTUAL JSONC INSERTION
///
/// Inserts the `modules-right` entry and module definition without parsing JSONC.
/// JSONC comments and user formatting survive because only proven offsets move.
/// The bracket matcher locates the target array; definitions are inserted from the
/// later offset first so the earlier array index remains valid.
/// FAIL CLOSED: an unfamiliar member boundary returns `None`. The caller prints a
/// manual snippet rather than guessing at JSONC grammar and corrupting user config.
fn waybar_config_with_module(config: &str, module_def: &str) -> Option<String> {
    let key_pos = config.find("\"modules-right\"")?;
    let open = key_pos + config[key_pos..].find('[')?;
    let close = open + find_matching_bracket(&config[open..])?;

    // ORDERING -> insert at the latest offset first so earlier indexes remain valid.
    let after_close = config[close + 1..]
        .char_indices()
        .find(|(_, c)| !c.is_whitespace())?;
    let (def_at, def_text) = match after_close.1 {
        // Existing following member: insert after its comma and keep a trailing comma.
        ',' => (
            close + 1 + after_close.0 + 1,
            format!("\n{module_def},\n  // lazysubs-eye-end"),
        ),
        // Last member: add the preceding comma, but no trailing comma.
        '}' => (close + 1, format!(",\n{module_def}\n  // lazysubs-eye-end")),
        _ => return None,
    };
    let mut out = String::with_capacity(config.len() + def_text.len() + 64);
    out.push_str(&config[..def_at]);
    out.push_str(&def_text);
    out.push_str(&config[def_at..]);

    // Put the entry first; add a comma only when the array already has content.
    let array_empty = config[open + 1..close].trim().is_empty();
    let entry = if array_empty {
        format!("\n    \"{MODULE_KEY}\" // lazysubs-eye\n  ")
    } else {
        format!("\n    \"{MODULE_KEY}\", // lazysubs-eye")
    };
    out.insert_str(open + 1, &entry);
    Some(out)
}

/// [DATA] BRACKET MATCHER
///
/// Finds the closing `]` for `s`'s opening `[`, ignoring brackets inside quoted
/// strings so JSONC values cannot terminate the array accidentally.
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

/// [DATA] OWNED-CONTENT REMOVAL
///
/// Uninstall removes only marked single lines and inclusive begin/end blocks,
/// returning clean text plus whether an owned edit was found. Marker validation
/// happens before this destructive transform: balanced markers alone are not
/// enough when a user has placed a manual rule inside the claimed region.
/// INVARIANT: unmarked matching module/CSS text remains user-owned and survives.
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

fn markers_are_balanced(text: &str) -> bool {
    let begins = text.matches("lazysubs-eye-begin").count();
    let ends = text.matches("lazysubs-eye-end").count();
    begins == ends
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerValidation {
    Absent,
    Intact,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineConflict {
    pub line: usize,
    pub content: String,
}

pub fn check_manual_rules_between_markers(path: &Path) -> Vec<LineConflict> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let allowed_json = [
        MODULE_KEY,
        "\"exec\"",
        "\"return-type\"",
        "\"interval\"",
        "\"signal\"",
        "\"on-click\"",
        "\"on-click-right\"",
    ];
    let mut inside = false;
    let mut conflicts = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.contains("lazysubs-eye-begin") {
            inside = true;
            continue;
        }
        if line.contains("lazysubs-eye-end") {
            inside = false;
            continue;
        }
        if !inside {
            continue;
        }
        let trimmed = line.trim();
        let structural = trimmed.is_empty()
            || matches!(trimmed, "{" | "}" | "},")
            || trimmed.starts_with("#custom-ai-usage")
            || trimmed.starts_with("/*")
            || trimmed.starts_with("margin:")
            || trimmed.starts_with("color:")
            || allowed_json.iter().any(|known| trimmed.contains(known));
        if !structural {
            conflicts.push(LineConflict {
                line: index + 1,
                content: crate::diagnostics::sanitize_error(trimmed),
            });
        }
    }
    conflicts
}

pub fn validate_markers(path: &Path) -> std::result::Result<MarkerValidation, InstallError> {
    let text = std::fs::read_to_string(path).map_err(|_| InstallError::ConfigNotWritable)?;
    if !text.contains("lazysubs-eye-begin") && !text.contains("lazysubs-eye-end") {
        return Ok(MarkerValidation::Absent);
    }
    if !markers_are_balanced(&text) {
        return Err(InstallError::MarkerMismatch);
    }
    if !check_manual_rules_between_markers(path).is_empty() {
        return Err(InstallError::OwnershipConflict);
    }
    Ok(MarkerValidation::Intact)
}

fn build_install_plan(paths: &ConfigPaths, omarchy: bool) -> Result<InstallPlan> {
    let mut plan = InstallPlan::default();
    let config = std::fs::read_to_string(&paths.waybar_config)?;
    let style = std::fs::read_to_string(&paths.waybar_style)?;
    for (path, changes) in [
        (&paths.waybar_config, !config.contains(MODULE_KEY)),
        (&paths.waybar_style, !style.contains("#custom-ai-usage")),
    ] {
        if changes {
            plan.files_to_modify.push(path.display().to_string());
            plan.backups_to_create.push(path.display().to_string());
        }
    }
    if paths.hyprland_conf.exists() {
        let hypr = std::fs::read_to_string(&paths.hyprland_conf)?;
        if !hypr.contains("org.omarchy.lazysubs-eye") {
            plan.files_to_modify
                .push(paths.hyprland_conf.display().to_string());
            plan.backups_to_create
                .push(paths.hyprland_conf.display().to_string());
        }
    }
    if !plan.files_to_modify.is_empty() {
        plan.commands_to_run.push(if omarchy {
            "omarchy restart waybar".into()
        } else {
            "reload waybar".into()
        });
    }
    Ok(plan)
}

/// [FLOW] SNAPSHOT BEFORE PUBLISH
///
/// Name backups beside their source so recovery does not depend on a separate
/// writable directory. Epoch plus collision suffix prevents two writes in one
/// second from silently overwriting the only rollback copy.
fn backup(path: &Path) -> Result<PathBuf> {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = path.with_extension(format!(
        "{}bak.{epoch}",
        path.extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    let mut bak = base.clone();
    let mut suffix = 1_u64;
    while bak.exists() {
        bak = PathBuf::from(format!("{}.{}", base.display(), suffix));
        suffix += 1;
    }
    std::fs::copy(path, &bak).with_context(|| format!("no pude crear el backup de {path:?}"))?;
    Ok(bak)
}

/// [FLOW] BACKUP → ATOMIC PUBLISH
///
/// Register the snapshot before publishing the replacement. `atomic_save_system`
/// prevents a watching Waybar/Hyprland process from reading a truncated file; if
/// publish fails, the journal still has the pre-mutation bytes for rollback.
fn write_with_backup(backups: &mut BackupManager, path: &Path, contents: &str) -> Result<()> {
    let bak = backups.backup(path)?;
    // [CACHE] Atomic write: Waybar watches these files and installation reloads
    // services immediately. A direct write could expose a half-written file.
    crate::cache::atomic_save_system(path, contents.as_bytes())
        .with_context(|| format!("no pude escribir {path:?}"))?;
    println!("  ✓ {} (backup: {})", path.display(), bak.display());
    Ok(())
}

/// [FLOW] PUBLISH → RELOAD CONSUMERS
///
/// Reload only after every requested file mutation has published. Waybar always
/// needs the new module/style pair; Hyprland reloads only when its optional rule
/// changed. Reload failures are surfaced to the user but do not rewrite config:
/// disk state remains recoverable through the backups.
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
    // [API] Outside Omarchy, SIGUSR2 reloads Waybar config. If no process exists,
    // try the user service; report the final fallback to the user.
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

/// [FLOW] INSTALL TRANSACTION
///
/// The public entry point owns the rollback boundary. `install_inner` may touch
/// several user files; any error after the first publish triggers reverse restore
/// before the error escapes. A complete existing integration is idempotent and
/// produces no backups, writes, or reload.
pub fn install(signal: u8) -> Result<()> {
    let mut backups = BackupManager::new();
    match install_inner(signal, &mut backups) {
        Ok(()) => Ok(()),
        Err(error) => {
            if let Err(rollback) = backups.rollback() {
                return Err(error.context(rollback.to_string()));
            }
            Err(error)
        }
    }
}

fn install_inner(signal: u8, backups: &mut BackupManager) -> Result<()> {
    let paths = config_paths()?;
    install_with_paths(signal, &paths, backups, true)
}

/// [FLOW] DISCOVER → PREFLIGHT → PLANLESS APPLY → RELOAD
///
/// This shared engine powers the live install and sandbox mode. It validates the
/// realtime signal before touching files, then independently installs Waybar JSONC,
/// Waybar CSS, and the optional Hyprland rule. Each component is idempotent so a
/// prior partial install is repaired by adding only missing owned pieces.
/// ORDERING: reload is deferred until all mutations succeed; callers choose whether
/// process reload is allowed in their environment.
fn install_with_paths(
    signal: u8,
    paths: &ConfigPaths,
    backups: &mut BackupManager,
    reload_services: bool,
) -> Result<()> {
    if !(1..=30).contains(&signal) {
        bail!("--signal debe estar entre 1 y 30 (RTMIN+N)");
    }
    require_preflight(paths)?;
    let exec = exec_path()?;
    let omarchy = is_omarchy();
    let mut changed = false;
    if !omarchy {
        println!("  · Omarchy no detectado: uso CSS neutro y fallbacks genéricos de waybar");
    }

    // [DATA] Waybar config: `config.jsonc` in Omarchy, or standard `config`.
    let config = std::fs::read_to_string(&paths.waybar_config)
        .with_context(|| format!("no pude leer {:?}", paths.waybar_config))?;
    let click = on_click(omarchy, &exec);
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
                write_with_backup(backups, &paths.waybar_config, &updated)?;
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

    // [UI] Waybar style: generic environments use neutral CSS without `@foreground`.
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
            backups,
            &paths.waybar_style,
            &format!("{style}{sep}{}", style_block(omarchy)),
        )?;
        changed = true;
    }

    // [UI] Add the floating window rule only when Hyprland is configured.
    // Other compositors such as Sway and River intentionally receive nothing.
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
                backups,
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
        if reload_services {
            reload(omarchy, touched_hypr);
        }
        println!("listo. El módulo aparece en waybar; click izquierdo abre la TUI.");
    } else {
        println!("nada que hacer: la integración ya estaba completa.");
    }
    Ok(())
}

/// [API] SANDBOX INSTALL
///
/// Runs installation against an isolated XDG tree and never reloads host
/// processes. With `dry_run`, returns only the serializable plan.
pub fn install_sandbox(config_home: &Path, signal: u8, dry_run: bool) -> Result<InstallPlan> {
    let paths = config_paths_at(config_home);
    require_preflight(&paths)?;
    let plan = build_install_plan(&paths, false)?;
    if dry_run {
        return Ok(plan);
    }
    let mut backups = BackupManager::new();
    if let Err(error) = install_with_paths(signal, &paths, &mut backups, false) {
        if let Err(rollback) = backups.rollback() {
            return Err(error.context(rollback.to_string()));
        }
        return Err(error);
    }
    Ok(plan)
}

/// Shows the installation plan without backups, file edits, or process reloads.
/// Shares read-only preflight checks with `install`.
pub fn install_dry_run(signal: u8) -> Result<()> {
    if !(1..=30).contains(&signal) {
        bail!("--signal debe estar entre 1 y 30 (RTMIN+N)");
    }
    let paths = config_paths()?;
    require_preflight(&paths)?;
    let omarchy = is_omarchy();
    let plan = build_install_plan(&paths, omarchy)?;
    println!("dry-run: no se modificará ningún archivo ni se recargará ningún servicio.");
    println!("{}", serde_json::to_string_pretty(&plan)?);
    println!("  · modo: {}", if omarchy { "Omarchy" } else { "genérico" });
    Ok(())
}

/// [FLOW] OWNERSHIP-CHECKED UNINSTALL
///
/// Uninstall uses the same transaction journal as install, but its authority is
/// narrower: markers identify generated Waybar text, while the Hyprland class rule
/// is the sole removable line. Manual lookalikes are preserved and reported.
pub fn uninstall() -> Result<()> {
    let mut backups = BackupManager::new();
    match uninstall_inner(&mut backups) {
        Ok(()) => Ok(()),
        Err(error) => {
            if let Err(rollback) = backups.rollback() {
                return Err(error.context(rollback.to_string()));
            }
            Err(error)
        }
    }
}

/// [FLOW] PREFLIGHT → VALIDATE OWNERSHIP → STRIP → RELOAD
///
/// Validate every present owned block before writing any file. This ordering avoids
/// removing CSS while a later config block reveals a marker conflict, and lets the
/// outer transaction restore earlier writes if an unexpected I/O failure follows.
fn uninstall_inner(backups: &mut BackupManager) -> Result<()> {
    let paths = config_paths()?;
    let issues = preflight_uninstall(&paths);
    if !issues.is_empty() {
        bail!(
            "preflight de uninstall falló: {}",
            issues
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let mut changed = false;

    for path in [&paths.waybar_config, &paths.waybar_style] {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if text.contains("lazysubs-eye-begin") || text.contains("lazysubs-eye-end") {
            validate_markers(path).map_err(anyhow::Error::new)?;
        }
        let (clean, modified) = strip_marked(&text);
        if modified {
            write_with_backup(backups, path, &clean)?;
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
            write_with_backup(backups, &paths.hyprland_conf, &kept)?;
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
        // The entry is first in the array.
        let entry = out.find("\"custom/ai-usage\",").unwrap();
        let tray = out.find("\"group/tray-expander\"").unwrap();
        assert!(entry < tray);
        // Removing JSONC comments leaves valid JSON.
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
    fn preflight_detecta_los_dos_archivos_base_ausentes() {
        let root =
            std::env::temp_dir().join(format!("lazysubs-eye-preflight-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = ConfigPaths {
            waybar_config: root.join("config.jsonc"),
            waybar_style: root.join("style.css"),
            hyprland_conf: root.join("hyprland.conf"),
        };
        let issues = preflight_install(&paths);
        assert!(issues.contains(&InstallError::WaybarConfigNotFound));
        assert!(issues.contains(&InstallError::WaybarStyleNotFound));
    }

    #[test]
    fn install_plan_serializa_todas_las_fases() {
        let plan = InstallPlan {
            files_to_modify: vec!["config".into()],
            backups_to_create: vec!["config".into()],
            commands_to_run: vec!["reload waybar".into()],
            files_to_delete: vec![],
        };
        let value = serde_json::to_value(plan).unwrap();
        assert_eq!(value["files_to_modify"][0], "config");
        assert!(value.get("backups_to_create").is_some());
        assert!(value.get("commands_to_run").is_some());
        assert!(value.get("files_to_delete").is_some());
    }

    #[test]
    fn resolve_binary_devuelve_un_ejecutable_canonico() {
        let binary = resolve_binary_path().unwrap();
        assert!(binary.is_absolute());
        assert!(binary.is_file());
    }

    #[test]
    fn marker_validation_detecta_edicion_manual() {
        let root = std::env::temp_dir().join(format!("lazysubs-markers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("config");
        std::fs::write(
            &file,
            "// lazysubs-eye-begin\n  \"exec\": \"lazysubs-eye\"\n// lazysubs-eye-end\n",
        )
        .unwrap();
        assert_eq!(validate_markers(&file), Ok(MarkerValidation::Intact));
        std::fs::write(
            &file,
            "// lazysubs-eye-begin\n  \"manual-rule\": true\n// lazysubs-eye-end\n",
        )
        .unwrap();
        assert_eq!(
            validate_markers(&file),
            Err(InstallError::OwnershipConflict)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn sandbox_dry_run_no_escribe_y_execute_es_idempotente() {
        let root = std::env::temp_dir().join(format!("lazysubs-sandbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("waybar")).unwrap();
        std::fs::write(root.join("waybar/config.jsonc"), STOCK).unwrap();
        std::fs::write(root.join("waybar/style.css"), "/* user */\n").unwrap();
        let before = std::fs::read(root.join("waybar/config.jsonc")).unwrap();
        let plan = install_sandbox(&root, 11, true).unwrap();
        assert_eq!(
            std::fs::read(root.join("waybar/config.jsonc")).unwrap(),
            before
        );
        assert_eq!(plan.files_to_modify.len(), 2);

        install_sandbox(&root, 11, false).unwrap();
        let once = std::fs::read(root.join("waybar/config.jsonc")).unwrap();
        install_sandbox(&root, 11, false).unwrap();
        let twice = std::fs::read(root.join("waybar/config.jsonc")).unwrap();
        assert_eq!(once, twice);
        assert!(String::from_utf8(once).unwrap().contains(MODULE_KEY));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn markers_incompletos_no_se_consideran_propiedad_segura() {
        assert!(markers_are_balanced(
            "/* lazysubs-eye-begin */\n/* lazysubs-eye-end */"
        ));
        assert!(!markers_are_balanced("/* lazysubs-eye-begin */"));
        assert!(!markers_are_balanced("/* lazysubs-eye-end */"));
    }

    #[test]
    fn backups_no_se_sobrescriben_en_el_mismo_segundo() {
        let root = std::env::temp_dir().join(format!("lazysubs-eye-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("config");
        std::fs::write(&file, "first").unwrap();
        let first = backup(&file).unwrap();
        std::fs::write(&file, "second").unwrap();
        let second = backup(&file).unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(second).unwrap(), "second");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn backup_manager_restaura_en_rollback() {
        let root =
            std::env::temp_dir().join(format!("lazysubs-eye-rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("config");
        std::fs::write(&file, "before").unwrap();
        let mut manager = BackupManager::default();
        manager.backup(&file).unwrap();
        std::fs::write(&file, "after").unwrap();
        manager.rollback().unwrap();
        assert_eq!(std::fs::read_to_string(file).unwrap(), "before");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rollback_failure_enumera_destinos_afectados() {
        let root =
            std::env::temp_dir().join(format!("lazysubs-rollback-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut manager = BackupManager::new();
        manager
            .entries
            .push((root.join("destination"), root.join("missing-backup")));
        assert!(matches!(
            manager.rollback(),
            Err(InstallError::RollbackFailed(files)) if files.len() == 1
        ));
        assert!(manager.backup(&root.join("absent")).is_err());
        let _ = std::fs::remove_dir_all(root);
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
        // Install appends after CSS ending in \n; uninstall must restore it
        // byte-for-byte without dangling blank lines.
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
            fallback_launch_for("lazysubs-eye", true, Some("alacritty")).as_deref(),
            Some("xdg-terminal-exec lazysubs-eye")
        );
        assert_eq!(
            fallback_launch_for("lazysubs-eye", false, Some("foot")).as_deref(),
            Some("foot lazysubs-eye")
        );
        assert_eq!(
            fallback_launch_for("lazysubs-eye", false, Some("kitty")).as_deref(),
            Some("kitty lazysubs-eye")
        );
        assert_eq!(
            fallback_launch_for("lazysubs-eye", false, Some("alacritty")).as_deref(),
            Some("alacritty -e lazysubs-eye")
        );
        assert_eq!(
            fallback_launch_for("lazysubs-eye", false, Some("ghostty")).as_deref(),
            Some("ghostty -e lazysubs-eye")
        );
        assert_eq!(fallback_launch_for("lazysubs-eye", false, None), None);
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
        // Warning and critical use dedicated hex colors in both modes.
        assert!(generic.contains("#e5c07b") && generic.contains("#e06c75"));
    }

    #[test]
    fn module_sin_on_click_sigue_siendo_json_valido() {
        let def = module_definition("lazysubs-eye", 11, None);
        assert!(!def.contains("on-click\""), "sin on-click de apertura");
        assert!(def.contains("on-click-right"));
        // Wrapped as an object member and stripped of comments -> valid JSON.
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

        // No files -> default to .jsonc.
        assert_eq!(resolve_waybar_config(&dir), dir.join("config.jsonc"));

        // Only standard `config` exists -> select it.
        std::fs::write(dir.join("config"), "{}").unwrap();
        assert_eq!(resolve_waybar_config(&dir), dir.join("config"));

        // If .jsonc also exists, it wins.
        std::fs::write(dir.join("config.jsonc"), "{}").unwrap();
        assert_eq!(resolve_waybar_config(&dir), dir.join("config.jsonc"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
