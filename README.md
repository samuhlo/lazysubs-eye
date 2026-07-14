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
| MiniMax | `GET /v1/token_plan/remains` (coding/token plan windows) | subscription key in `[minimax] api_key` or `MINIMAX_API_KEY` |

If a fresh query fails (a stray 429, a network blip) the last good data is
kept on screen for up to 30 minutes — marked as aged in the tooltip and the
TUI — instead of wiping the panel with an error.

Daily token usage panels are also built from local data:

| Panel | Data source |
|---|---|
| Claude tokens | JSONL transcripts in `~/.claude/projects` |
| Pi tokens | session JSONL in `~/.pi/agent/sessions` |
| OpenCode tokens | OpenCode SQLite database in `~/.local/state/opencode` |

Each panel can show **today, this week, or this month** — press `t` (or Tab) in
the TUI to cycle the period. Usage is recorded into a local SQLite history
(`$XDG_STATE_HOME/lazysubs-eye/history.db`) so past days survive even when the
source transcripts get pruned; the first run backfills whatever history the
sources still hold. A sparkline of the daily total sits under each panel. Turn
the whole thing off with `[stats] enabled = false` (see Configuration).

Everything runs locally: nothing is sent to third parties — only the official
API of each provider is queried with your own credentials. lazysubs-eye **never
refreshes OAuth tokens** (each CLI does that itself); if a token expires it
shows a reauth notice.

## Usage

```
lazysubs-eye            # TUI if stdout is a tty; JSON otherwise
lazysubs-eye tui        # explicit TUI (q quit · r refresh · o settings; auto-refresh 60s)
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
notification_cooldown = 1800 # min seconds between repeat notifications for
                     # the same window (escalations don't wait); 0 = off
colors = true        # false: no threshold coloring anywhere (the waybar
                     # `error` class stays — it signals breakage, not usage)
show_account = true  # show the account (email/alias) next to the plan in the
                     # TUI and the waybar tooltip; false hides it (Claude's
                     # email is read from ~/.claude.json, identity only)

[providers]          # disable a provider entirely (it isn't even queried)
claude = true
codex = true
minimax = true

[waybar]             # what the bar shows — independent of the TUI
# providers = ["claude", "minimax"]   # which ones AND their order
# percent = false                     # icons only

[tui]                # what the TUI shows
# providers = ["minimax", "claude", "codex"]
# panels = ["claude_tokens", "pi_tokens", "opencode_tokens"]

[stats]              # per-day spend history (SQLite in $XDG_STATE_HOME)
enabled = true       # false: no DB, no history panels (today-only, as before)
default_period = "hoy" # initial panel period: hoy | semana | mes
history_days = 90    # retention in days; 0 = keep everything
sparkline = true     # daily-total sparkline under each token panel

[icons]              # override the waybar/TUI icons
claude = "✳"
codex = "⬡"
minimax = "◆"

[minimax]            # MiniMax needs its token-plan Subscription Key
api_key = "..."      # or the MINIMAX_API_KEY env var
# base_url = "https://api.minimaxi.com"  # alternate host (e.g. China)
```

`[waybar] providers` and `[tui] providers` control both visibility and order,
per surface — e.g. keep the bar minimal with one provider while the TUI shows
everything. Hidden providers don't drive the bar's CSS class either. `[tui]
panels` toggles the daily-token panels (disabled panels aren't even scanned).

### Multiple accounts

By default each provider is a single auto-detected account. To watch more than
one account of the same AI (e.g. two Claude logins), declare them under
`[[accounts.<provider>]]`:

```toml
[[accounts.claude]]
name = "personal"                            # first/only account keeps id "claude"
[[accounts.claude]]
name = "work"                                # becomes id "claude:work"
credentials = "~/work/.claude/.credentials.json"
icon = "❄"                                   # optional per-account icon

[[accounts.codex]]
name = "personal"
codex_home = "~/.codex"                       # passed as CODEX_HOME to the app-server

[[accounts.minimax]]
name = "personal"
api_key = "..."                               # per-account key (or use [minimax] for one)
```

Each account becomes its own provider with a composite id (`claude:work`) and a
name like `Claude Code · work`. The first/only account keeps the plain id
(`claude`) so existing `providers` lists and notification state keep working;
those lists accept composite ids too. Without any `[[accounts.*]]` the behaviour
is unchanged. The account rows in the `o` panel (waybar/tui visibility) are built
from your configured accounts.

### In-TUI settings

Press `o` in the TUI to edit everything above interactively: arrow keys to
move, space to toggle, `←`/`→` to adjust thresholds, TTL and the history
settings. Changes apply live (the bar picks them up on its next poll) and are
written back to `config.toml` preserving your comments and any keys the panel
doesn't manage (like the MiniMax `api_key`). `t` (or Tab) cycles the token
panels through today / this week / this month.

### Notifications

On every fresh query (waybar polls each minute) lazysubs-eye compares each
rate-limit window against the thresholds and sends a desktop notification via
`notify-send` when a window *crosses* into warning (normal urgency) or
critical (critical urgency). It only notifies on level changes — state is kept
in `~/.cache/lazysubs-eye/notify-state.json`, re-arming when the window resets
or drops back below the threshold — so it never spams. Re-notifications of the
same level (rolling reset windows, dipping below and crossing again) also wait
`notification_cooldown` seconds (default 30 min); escalating to a higher level
never waits.

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
- [x] MiniMax provider · graceful degradation on transient API errors
- [x] In-TUI settings panel · notification cooldown
- [ ] Weekly/monthly usage history + stats (SQLite)
- [ ] Account display + multi-account per provider
- [ ] Any Linux with waybar (Omarchy-first)

## Contributing

Contributions are very welcome — this project gets better the more AI
subscriptions it covers, and everyone's stack is different. If your provider
isn't supported, adding it is deliberately small: a provider is one module in
`src/providers/` implementing `available()` (is it configured?) and
`collect()` (fetch quota windows), plus a couple of lines to register it in
`src/providers/mod.rs` and `src/config.rs`. See `src/providers/minimax.rs`
for a compact example with tests. Notifications, `--check`, waybar classes
and graceful degradation then work for your provider for free.

Bug reports, UI ideas and docs fixes are just as appreciated — open an issue
or a PR.

## License

[MIT](LICENSE)
