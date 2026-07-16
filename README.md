<div align="center">
  <br />
  <h1><code>./LAZYSUBS-EYE</code></h1>

**AI subscription observability — live quotas, local token history, Waybar and an adaptive TUI**
  <br />

[![Release](https://img.shields.io/github/v/release/samuhlo/lazysubs-eye?style=for-the-badge&label=RELEASE&color=FFCA40)](https://github.com/samuhlo/lazysubs-eye/releases/latest)
[![CI](https://img.shields.io/github/actions/workflow/status/samuhlo/lazysubs-eye/ci.yml?style=for-the-badge&label=CI&logo=githubactions&logoColor=white)](https://github.com/samuhlo/lazysubs-eye/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/LICENSE-MIT-0C0011?style=for-the-badge)](LICENSE)

  <br />
</div>

---

## // 00\_ THE_MISSION

lazysubs-eye is a Linux monitor for AI subscription quotas and token usage. It
brings the live usage windows of **Claude Code**, **Codex** and **MiniMax** into
Waybar and a lazygit-style TUI, while preserving a local token history for
Claude, Pi and OpenCode.

It is polished for [Omarchy](https://omarchy.org), but it is not tied to it: the
TUI works in any terminal and the automatic installer adapts to generic Waybar
and Hyprland setups.

> _Quota data comes from each provider's official API. Token history is processed locally and never uploaded by lazysubs-eye._

```text
 lazysubs-eye · cuotas de IA
╭ ✳ Claude Code ─ pro ──────────────────────────────────────────────────────────╮
│ [✓] 5h             73% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━      → 3h06m │
│ [✓] semana         36% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━      → 5d21h │
╰───────────────────────────────────────────────────────────────────────────────╯
╭ ⬡ Codex ─ plus ───────────────────────────────────────────────────────────────╮
│ [!] semana         80% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━      → 5d23h │
│ Créditos de reinicio disponibles: 4                                          │
╰───────────────────────────────────────────────────────────────────────────────╯
╭ ✳ tokens Claude · hoy ────────────────────────────────────────────────────────╮
│ modelo                         in       out      cache→   cache+       total  │
│ claude-fable-5                 17.7k    30.2k    777.9k   158.7k      984.5k  │
╰───────────────────────────────────────────────────────────────────────────────╯
 q salir  r refrescar  o opciones  j/k scroll                         hace 59s
```

---

## // 01\_ SYSTEM_CAPABILITIES

| AREA | IMPLEMENTATION |
| :--- | :--- |
| **Live quotas** | Usage percentage, reset countdown, plan and account for every provider |
| **Surfaces** | Adaptive TUI, Waybar JSON, full JSON and script-friendly health checks |
| **Local usage** | Per-model Claude, Pi and OpenCode tokens for today, week or month |
| **History** | SQLite retention, per-source provenance, sparklines and combined spend graphs |
| **Accounts** | Multiple accounts per provider with independent ordering and visibility |
| **Alerts** | Threshold notifications with cooldown, re-arming and immediate escalation |
| **Resilience** | Fast cache, stale fallback, bounded workers and refresh coalescing |
| **Diagnostics** | Stable exit codes, `doctor`, structured JSON and sanitized verbose logs |
| **Persistence** | Private atomic writes, advisory locks, symlink rejection and rollback |
| **Integration** | Idempotent Waybar/Hyprland install, dry-run, sandbox and uninstall |

The TUI also includes scroll, an in-app help overlay, live settings and
deterministic fallbacks for small terminals, monochrome output and non-UTF-8
locales. State is always communicated by text or symbols as well as color.

---

## // 02\_ QUICK_START

### 001 — Install

The supported **Linux x86_64** build is available from
[GitHub Releases](https://github.com/samuhlo/lazysubs-eye/releases/latest) as a
static MUSL binary with a matching SHA-256 file.

To build from source instead:

```bash
git clone https://github.com/samuhlo/lazysubs-eye.git
cd lazysubs-eye
cargo install --path . --locked
```

The repository also ships an Arch Linux
[`PKGBUILD`](packaging/aur/PKGBUILD).

### 002 — Check the environment

```bash
lazysubs-eye --version
lazysubs-eye doctor
```

Claude and Codex are discovered from their existing CLI sessions. MiniMax
requires `MINIMAX_API_KEY` or `[minimax].api_key` in the config file.

### 003 — Open the TUI

```bash
lazysubs-eye tui
```

### 004 — Wire up Waybar

Inspect the complete plan before applying it:

```bash
lazysubs-eye install --dry-run
lazysubs-eye install
```

The installer resolves the real executable, touches only the required files,
creates backups and reloads Waybar. Re-running it is safe and idempotent.

---

## // 03\_ DATA_PIPELINE

### Subscription quotas

| PROVIDER | DATA SOURCE | REQUIREMENT |
| :--- | :--- | :--- |
| **Claude Code** | `api.anthropic.com/api/oauth/usage` | Active Claude Code session |
| **Codex** | `codex app-server` → `account/rateLimits/read` | `codex login` |
| **MiniMax** | `GET /v1/token_plan/remains` | Subscription Key in config or environment |

Codex also exposes reset credits when the server includes them. lazysubs-eye
never refreshes OAuth tokens; each provider CLI remains responsible for its own
authentication lifecycle.

### Local token usage

| PANEL | LOCAL SOURCE |
| :--- | :--- |
| **Claude tokens** | `~/.claude/projects/**/*.jsonl` |
| **Pi tokens** | `~/.pi/agent/sessions/**/*.jsonl` |
| **OpenCode tokens** | SQLite database under `~/.local/state/opencode` |

The readers use cursors, fingerprints and consistent cutoffs. They do not
reparse an entire source on every refresh, and data appended during an active
scan is deferred to the next consistent snapshot.

---

## // 04\_ COMMAND_CENTER

```text
lazysubs-eye                 TUI with an interactive stdout; JSON without a TTY
lazysubs-eye tui             open the TUI explicitly
lazysubs-eye --waybar        one-line JSON for Waybar
lazysubs-eye --json          full state as JSON
lazysubs-eye --check         health summary and operational exit code
lazysubs-eye doctor          inspect local configuration and dependencies
lazysubs-eye doctor --json   structured diagnostics
lazysubs-eye --verbose       cache and collector decisions on stderr
lazysubs-eye --no-cache      force a fresh provider read
lazysubs-eye --ttl 120       override the TTL for this invocation
lazysubs-eye install         integrate Waybar and Hyprland when available
lazysubs-eye install --dry-run
lazysubs-eye install --sandbox /tmp/lazysubs-config
lazysubs-eye uninstall       remove only lazysubs-eye-owned integration
```

`--verbose` never changes stdout, so it can be combined with `--json` or
`--waybar` without breaking consumers.

### `--check` exit contract

| CODE | MEANING |
| ---: | :--- |
| `0` | Data is available, fresh and below all active thresholds |
| `1` | Warning threshold, stale data or partial ingestion |
| `2` | At least one quota window has reached the critical threshold |
| `3` | Operational error, invalid configuration or no available providers |

Example pre-flight hook for a long agent session:

```bash
if ! lazysubs-eye --check; then
  echo "Review your quotas before continuing"
fi
```

---

## // 05\_ TUI_PROTOCOL

| KEY | ACTION |
| :--- | :--- |
| `q`, `Esc`, `Ctrl+C` | Quit or close the active panel |
| `r` | Force a refresh |
| `j` / `k`, `↓` / `↑` | Scroll or navigate |
| `o` | Open settings |
| `Space`, `Enter` | Toggle a setting |
| `h` / `l`, `←` / `→` | Decrease or increase a value |
| `t`, `Tab` | Cycle today → week → month |
| `g` | Open or close the spend graph |
| `v`, `←`, `→` | Change the graph view |
| `?` | Show help |

The recommended terminal size is 80×24. Smaller terminals receive a compact
screen instead of a broken layout. Color and character selection honor
`NO_COLOR`, `FORCE_COLOR`, `TERM=dumb` and the current locale.

---

## // 06\_ CONFIGURATION

Configuration is optional. It lives at
`$XDG_CONFIG_HOME/lazysubs-eye/config.toml` or
`~/.config/lazysubs-eye/config.toml`.

```toml
ttl = 60
warning_at = 80.0
critical_at = 95.0
notifications = true
notification_cooldown = 1800
colors = true
show_account = true

[providers]
claude = true
codex = true
minimax = true

[waybar]
providers = ["claude", "codex"]
percent = true

[waybar.window]
claude = "semana"
codex = "semana"

[tui]
providers = ["claude", "codex", "minimax"]
panels = ["claude_tokens", "pi_tokens", "opencode_tokens"]

[stats]
enabled = true
default_period = "hoy" # hoy | semana | mes
history_days = 90      # 0 keeps everything
sparkline = true

[icons]
claude = "✳"
codex = "⬡"
minimax = "◆"

[minimax]
api_key = "..." # or MINIMAX_API_KEY
# base_url = "https://api.minimaxi.com"
```

Settings written from the TUI preserve comments and keys the panel does not
manage. Invalid configuration is reported by diagnostics and produces exit `3`
in `--check`; presentation modes fall back to safe defaults.

### Multiple accounts

Without `accounts` blocks, every provider uses its auto-detected account. Add
more accounts with:

```toml
[[accounts.claude]]
name = "personal"

[[accounts.claude]]
name = "work"
credentials = "~/work/.claude/.credentials.json"
icon = "❄"

[[accounts.codex]]
name = "personal"
codex_home = "~/.codex"

[[accounts.minimax]]
name = "personal"
api_key = "..."
```

Additional accounts receive composite ids such as `claude:work`. Those ids are
valid in `waybar.providers` and `tui.providers`, allowing independent ordering
and visibility on each surface.

---

## // 07\_ HISTORY_RUNTIME

On first use, a background worker backfills every day still available from the
local sources. The TUI remains interactive, displays progress and can close
without losing committed days. The next run resumes after the last contiguous
day.

Ingestion is transactional per source and day. A failed scan cannot overwrite
previous good data, while `Partial` and `Failed` states remain visible to the
TUI and `--check`.

The runtime is designed for frequent Waybar polling:

- A valid cache avoids network calls; the normal cached path takes only a few ms.
- Provider families run in parallel under a global time budget.
- Overlapping refresh requests are coalesced instead of spawning duplicate work.
- Pi processes only the newly appended suffix of each stable file.
- SQLite is consumed in bounded batches instead of loading full histories in RAM.

The measured budgets live in [`perf/baseline.json`](perf/baseline.json) and are
checked by `scripts/benchmarks/run_budgets.sh`.

---

## // 08\_ CONTROLLED_INTEGRATION

`lazysubs-eye install` follows a transactional workflow:

1. Run a complete preflight before writing anything.
2. Add `custom/ai-usage` to Waybar and theme-neutral CSS.
3. Add a floating rule only when Hyprland is present.
4. Fence owned content with explicit markers.
5. Create `.bak.<epoch>` backups and roll back if any phase fails.
6. Reload Waybar only after every file has been committed successfully.

Outside Omarchy it discovers `config.jsonc` or `config`, uses
`xdg-terminal-exec` or a known terminal, and skips integrations that do not
apply. Sway and river users can add the equivalent floating rule manually.

### Isolated sandbox

```bash
lazysubs-eye install --sandbox /tmp/lazysubs-config --dry-run
lazysubs-eye install --sandbox /tmp/lazysubs-config
```

The sandbox is treated as an isolated XDG config root and never reloads host
services.

### Manual Waybar module

```jsonc
"custom/ai-usage": {
  "exec": "$HOME/.local/bin/lazysubs-eye --waybar",
  "return-type": "json",
  "interval": 60,
  "signal": 11,
  "on-click": "xdg-terminal-exec lazysubs-eye tui",
  "on-click-right": "$HOME/.local/bin/lazysubs-eye --no-cache --waybar >/dev/null && pkill -RTMIN+11 waybar"
}
```

Emitted CSS classes: `normal`, `warning`, `critical` and `error`.

---

## // 09\_ SECURITY_MODEL

- Credentials are never written to caches, indexes or logs.
- Errors are sanitized before display or persistence.
- Private files use mode `0600`; private directories use `0700`.
- Writes use a private temp file, `fsync`, atomic rename and directory sync.
- Symlink destinations and parent chains are rejected.
- `flock` coordinates writers and the inode is verified before commit.
- The history database is created privately before SQLite opens it.

Local data follows the XDG directory model:

| CONTENT | PATH |
| :--- | :--- |
| **Configuration** | `$XDG_CONFIG_HOME/lazysubs-eye/config.toml` |
| **Provider cache** | `$XDG_CACHE_HOME/lazysubs-eye/status.json` |
| **Notification state** | `$XDG_CACHE_HOME/lazysubs-eye/notify-state.json` |
| **Last sanitized error** | `$XDG_CACHE_HOME/lazysubs-eye/last-error.json` |
| **History** | `$XDG_STATE_HOME/lazysubs-eye/history.db` |

When XDG variables are unset, the standard `~/.config`, `~/.cache` and
`~/.local/state` locations are used.

---

## // 10\_ DIAGNOSTICS

Start with:

```bash
lazysubs-eye doctor
lazysubs-eye --check
lazysubs-eye --verbose --no-cache --json >/tmp/lazysubs-status.json
```

`doctor` checks configuration, providers, paths, permissions, the executable,
history database, incremental indexes, `notify-send` and the last recorded
error. Messages use stable codes `E001`–`E008` and redact private paths,
credentials, sensitive URLs and internal SQLite details.

When a fresh request fails, lazysubs-eye temporarily keeps the last good state
as `stale` instead of blanking the panel.

---

## // 11\_ COMPATIBILITY

- Supported and tested release: **x86_64 Linux**, static MUSL binary.
- Terminal: any ANSI-capable emulator, with color and UTF-8 fallbacks.
- Waybar: optional; required only for status-bar integration.
- Hyprland and Omarchy: optional, auto-integrated when present.
- aarch64: not advertised as supported until it has a verified build job.

---

## // 12\_ DEVELOPMENT

Use stable Rust and run the same mandatory gates as CI:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
scripts/check-release-readiness.sh
scripts/benchmarks/run_budgets.sh
```

Release tags build the MUSL artifact, run a RustSec audit, verify version
consistency, execute smoke tests against the final binary and publish the
tarball with its SHA-256 checksum.

Further project documentation:

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — contribution workflow and pull-request gates.
- [`SECURITY.md`](SECURITY.md) — responsible vulnerability reporting.
- [`CHANGELOG.md`](CHANGELOG.md) — release history.
- [`docs/ARQUITECTURA.md`](docs/ARQUITECTURA.md) — architecture and data sources.
- [`docs/ESTADO.md`](docs/ESTADO.md) — project state and technical decisions.

---

<div align="center">
  <br />

<code>DESIGNED & CODED BY <a href="https://github.com/samuhlo">samuhlo</a></code>

<small>Lugo, Galicia</small>

</div>
