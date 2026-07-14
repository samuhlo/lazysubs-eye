# lazysubs-eye

AI subscription quota monitor for [Omarchy](https://omarchy.org), lazygit-style.
Shows the rate-limit windows (5h session, weekly…) of your AI CLIs in waybar
and in a TUI, plus a per-model breakdown of the tokens you've burned today.

```
 lazysubs-eye · cuotas de IA
╭ ✳ Claude Code ─ pro ──────────────────────────────────────────────────────────╮
│ 5h                73% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━   → 3h06m │
│ semana            36% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━   → 5d21h │
│ semana · Fable    59% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━   → 5d21h │
╰───────────────────────────────────────────────────────────────────────────────╯
╭ ⬡ Codex ─ plus ───────────────────────────────────────────────────────────────╮
│ semana            80% ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━   → 5d23h │
│ Créditos de reinicio disponibles: 4                                           │
╰───────────────────────────────────────────────────────────────────────────────╯
╭ ✳ tokens hoy ─────────────────────────────────────────────────────────────────╮
│ modelo                            req    in       out      cache→   cache+    │
│ claude-fable-5                    34     17.7k    30.2k    777.9k   158.7k    │
╰───────────────────────────────────────────────────────────────────────────────╯
 q salir  r refrescar                                                  hace 59s
```

The TUI only uses ANSI colors, so it inherits your terminal theme (and thus
the active Omarchy theme) with zero configuration.

## Providers

| Provider | Data source | Requirement |
|---|---|---|
| Claude Code | OAuth endpoint `api.anthropic.com/api/oauth/usage` with the token from `~/.claude/.credentials.json` | logged in to Claude Code |
| Codex | JSON-RPC via `codex app-server` (`account/rateLimits/read`), including reset credits | `codex login` |

Daily token usage panels are also built from local data:

| Panel | Data source |
|---|---|
| Claude tokens today | JSONL transcripts in `~/.claude/projects` |
| Pi tokens today | session JSONL in `~/.pi/agent/sessions` |
| OpenCode tokens today | OpenCode SQLite database in `~/.local/state/opencode` |

Everything runs locally: nothing is sent to third parties — only the official
API of each provider is queried with your own credentials. lazysubs-eye **never
refreshes OAuth tokens** (each CLI does that itself); if a token expires it
shows a reauth notice.

## Usage

```
lazysubs-eye            # TUI if stdout is a tty; JSON otherwise
lazysubs-eye tui        # explicit TUI (q quit · r refresh; auto-refresh 60s)
lazysubs-eye install    # wire up waybar + Hyprland (idempotent, with backups)
lazysubs-eye uninstall  # revert the integration
lazysubs-eye --json     # full JSON dump of the state
lazysubs-eye --waybar   # single-line JSON for a custom waybar module
lazysubs-eye --check    # summary + exit code: 0 ok, 1 warning, 2 critical, 3 error
lazysubs-eye --no-cache # force a fresh query
lazysubs-eye --ttl 120  # cache validity (seconds, default 60)
lazysubs-eye --signal 8 # RTMIN+N signal for the waybar module (install, default 11)
lazysubs-eye --version  # print version
```

The cache lives in `~/.cache/lazysubs-eye/status.json` (cached runs take ~5 ms,
so waybar can poll every 60 s for free).

`--check` is made for scripts and hooks, e.g. warn before starting a long
agent session: `lazysubs-eye --check || echo "quota running low"`.

## Configuration

Optional, at `~/.config/lazysubs-eye/config.toml`. Every field has a default;
an invalid file never breaks the output (it warns on stderr and falls back to
defaults):

```toml
ttl = 60             # cache validity in seconds (--ttl overrides)
warning_at = 80.0    # thresholds in % — drive the waybar CSS class,
critical_at = 95.0   # the TUI gauge colors, --check and notifications
notifications = true # desktop notifications via notify-send (mako)

[providers]          # disable a provider even if its CLI is logged in
claude = true
codex = true

[icons]              # override the waybar/TUI icons
claude = "✳"
codex = "⬡"
```

### Notifications

On every fresh query (waybar polls each minute) lazysubs-eye compares each
rate-limit window against the thresholds and sends a desktop notification via
`notify-send` when a window *crosses* into warning (normal urgency) or
critical (critical urgency). It only notifies on level changes — state is kept
in `~/.cache/lazysubs-eye/notify-state.json`, re-arming when the window resets
or drops back below the threshold — so it never spams.

## Installation

From source (requires Rust; an AUR package — `lazysubs-eye-bin` — is planned,
see `packaging/aur/PKGBUILD`):

```
cargo install --path .
```

Then let lazysubs-eye wire itself into your Omarchy setup:

```
lazysubs-eye install
```

This inserts the waybar module (first in `modules-right`), theme-neutral CSS
and the Hyprland windowrule for the floating TUI, then reloads both. Every
touched file gets a `.bak.<epoch>` backup, everything inserted is fenced with
`lazysubs-eye-begin`/`lazysubs-eye-end` markers, and `lazysubs-eye uninstall` reverts it
byte for byte. Use `--signal N` if RTMIN+11 collides with another module.

## Waybar integration (manual)

What `lazysubs-eye install` sets up, if you prefer to do it by hand:

```jsonc
"custom/ai-usage": {
  "exec": "$HOME/.local/bin/lazysubs-eye --waybar",
  "return-type": "json",
  "interval": 60,
  "signal": 11,
  "on-click": "omarchy-launch-or-focus-tui lazysubs-eye",
  "on-click-right": "$HOME/.local/bin/lazysubs-eye --no-cache --waybar >/dev/null && pkill -RTMIN+11 waybar"
}
```

Emitted CSS classes: `normal`, `warning` (≥80 %), `critical` (≥95 %), `error`.
Manual refresh from any script: `pkill -RTMIN+11 waybar`.
Left click opens (or focuses) the TUI in a floating terminal. That needs this
rule in `~/.config/hypr/hyprland.conf` so the window floats centered:

```
windowrule = tag +floating-window, match:class org.omarchy.lazysubs-eye
```

## Documentation

Internal docs are in Spanish:

- [docs/ARQUITECTURA.md](docs/ARQUITECTURA.md) — how it works: structure, data sources, cache, TUI and system integration.
- [docs/ESTADO.md](docs/ESTADO.md) — project state, decisions taken and pending work.

## Roadmap

- [x] Phase 1 — core collector + `--json` / `--waybar` outputs
- [x] Phase 2 — waybar integration + floating window on Hyprland
- [x] Phase 3 — TUI (ratatui) with terminal theming + today's tokens per model
- [x] Codex reset credits · daily tokens for Pi and OpenCode
- [x] `lazysubs-eye install` / `uninstall` (one-command waybar + Hyprland setup)
- [x] CI + release binaries (static musl) + AUR PKGBUILD
- [x] Config file, threshold notifications (mako), `--check` for scripts
- [ ] Quota providers for Gemini CLI and OpenCode, history + sparklines

## License

[MIT](LICENSE)
