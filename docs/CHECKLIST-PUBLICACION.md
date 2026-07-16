# Checklist de publicación — lazysubs-eye

Checklist operativo reutilizable para cada fase de release: **Beta técnica**,
**RC** y **Stable**. Cada ítem pide evidencia concreta, comando o enlace;
**no afirma que ya pasó** hasta que se verifique en la sesión actual.

---

## A. Alcance y versionado

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Versión en `Cargo.toml` coincide con tag Git | `git describe --tags` vs `cargo metadata --format-version 1 --no-deps | jq '.packages[0].version'` | [ ] | [ ] | [ ] |
| Versión en PKGBUILD coincide con tag | `git tag` y contenido de `packaging/aur/PKGBUILD` | [ ] | [ ] | [ ] |
| Tag Git firmado o commit objetivo verificado | `git tag -v "$TAG"` o `git rev-parse "$TAG^{commit}"` | [ ] | [ ] | [ ] |
| Changelog actualizado desde última release | `git log --oneline v*..HEAD` revisiado | [ ] | [ ] | [ ] |
| Decisiones abiertas de la fase están resueltas | Enlaces al plan, apply progress y verify reports aplicables | [ ] | [ ] | [ ] |
| No hay features flags activados por defecto no documentados | `grep -r 'default = true' Cargo.toml` vacío | [ ] | [ ] | [ ] |

---

## B. Seguridad y privacidad

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Archivos de config (`~/.config/lazysubs-eye/`) son 0600 | `stat -c '%a' ~/.config/lazysubs-eye/config.toml` | [ ] | [ ] | [ ] |
| Archivos de historial (`~/.local/state/lazysubs-eye/history.db`) son 0600 | `stat -c '%a' ~/.local/state/lazysubs-eye/history.db` | [ ] | [ ] | [ ] |
| Directorios de estado y caché son 0700 | `stat -c '%a' ~/.cache/lazysubs-eye ~/.local/state/lazysubs-eye` | [ ] | [ ] | [ ] |
| No hay API keys ni tokens en logs con `--verbose` | `lazysubs-eye --verbose 2>&1 | grep -i 'key\|token\|secret'` vacío | [ ] | [ ] | [ ] |
| La política elegida para symlinks evita reemplazos silenciosos y escapes de ruta | Verify report de `secure-local-persistence` + tests de la política elegida | [ ] | [ ] | [ ] |
| Notify state no contiene secretos | Revisión sanitizada de `~/.cache/lazysubs-eye/notify-state.json` | [ ] | [ ] | [ ] |
| La ruta de OpenCode tiene una única fuente de verdad documentada | Test de resolución + sección de configuración/troubleshooting | [ ] | [ ] | [ ] |

---

## C. Integridad

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Tests pasan completos | `cargo test --locked` | [ ] | [ ] | [ ] |
| Formato sin cambios | `cargo fmt --check` | [ ] | [ ] | [ ] |
| Clippy sin warnings | `cargo clippy --all-targets -- -D warnings` | [ ] | [ ] | [ ] |
| Gate de vulnerabilidades adoptado pasa (RC+) | Comando y política elegidos en el verify report | — | [ ] | [ ] |
| Checksums sha256 de binarios publicado | `sha256sum release/*` y verificación publicada | [ ] | [ ] | [ ] |
| Reproducibilidad verificada, si se adoptó | Entorno reproducible documentado y `cmp --silent bin1 bin2` | — | N/A o [ ] | N/A o [ ] |

---

## D. Calidad

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Todos los paquetes exigidos por el gate tienen verify report aprobado | Enlaces a cada `verify-report.md` aplicable | [ ] | [ ] | [ ] |
| No quedan tareas abiertas en los paquetes aplicables | Revisión de cada `tasks.md` listado por el gate | [ ] | [ ] | [ ] |
| Sin unwrap/expect/panic en paths de usuario | `grep -rn 'unwrap()\|expect(' src/*.rs | [ ] | [ ] | [ ] |
| Estados explícitos en TUI (Ready/Empty/Unavailable/Stale/Loading) | Revisión de `src/tui.rs` | [ ] | [ ] | [ ] |
| Errores normalizados sin paths ni mensajes SQLite crudos | `lazysubs-eye --check` con DB corrupta → mensaje genérico | [ ] | [ ] | [ ] |
| Cada panel tiene al menos un test de render | `grep 'draw_' src/tui.rs | wc -l` versus tests | [ ] | [ ] | [ ] |

---

## E. Rendimiento

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Waybar cacheado < 10 ms | `time ./target/release/lazysubs-eye --json` con datos frescos | [ ] | [ ] | [ ] |
| Primer render TUI < 150 ms | `time ./target/release/lazysubs-eye` | [ ] | [ ] | [ ] |
| Escaneo incremental local sin cambios < 500 ms | `time ./target/release/lazysubs-eye --verbose` | [ ] | [ ] | [ ] |
| Refresh global cumple el presupuesto elegido tras baseline | Benchmark con providers lentos + presupuesto registrado | — | [ ] | [ ] |
| Backfill no bloquea primer render | TUI visible antes de que backfill termine | [ ] | [ ] | [ ] |
| P1-6 (Pi O(E²)) eliminado: re-parse solo si cambió | Logs con fingerprint y diff | — | [ ] | [ ] |

---

## F. UX / Accesibilidad terminal

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| `NO_COLOR=1` desactiva colores en TUI | Prueba manual en PTY + test de render sin estilos de color | [ ] | [ ] | [ ] |
| Terminal 80×24 sin contenido cortado ni panic | Prueba manual en PTY 80×24 con captura y pasos registrados | [ ] | [ ] | [ ] |
| Loading states visibles con indicador de fuente | `grep -n 'Loading' src/tui.rs` tiene label | [ ] | [ ] | [ ] |
| Scroll funciona si hay más contenido que espacio | Revisión manual en terminal pequeño | [ ] | [ ] | [ ] |
| Fallback ASCII si terminal no soporta UTF-8 | `TERM=dumb ./target/release/lazysubs-eye` legible | [ ] | [ ] | [ ] |
| Selección no basada solo en color (daltónicos) | Revisión de `src/output.rs` + tests con `FORCE_COLOR=0` | — | [ ] | [ ] |
| Ayuda CLI documentada | `./target/release/lazysubs-eye --help` | [ ] | [ ] | [ ] |
| Overlay interactivo `?` explica teclas y abreviaturas | Prueba manual en TUI con captura | [ ] | [ ] | [ ] |

---

## G. Instalación y desinstalación

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| `install --dry-run` muestra plan sin modificar | `./target/release/lazysubs-eye install --dry-run` | [ ] | [ ] | [ ] |
| `install` crea marcadores lazysubs-eye-begin/end | `grep lazysubs-eye-begin ~/.config/waybar/config*` | [ ] | [ ] | [ ] |
| `uninstall` no toca reglas manuales fuera de marcadores | Diff byte a byte de sandbox antes/después excluyendo bloques `lazysubs-eye` | [ ] | [ ] | [ ] |
| Install idempotente (ejecutar dos veces = mismo estado) | Diff de configs tras doble install | [ ] | [ ] | [ ] |
| Uninstall idempotente (ejecutar dos veces = mismo estado) | Diff de configs tras doble uninstall | [ ] | [ ] | [ ] |
| Backup con timestamp antes de cada modify | `ls ~/.config/waybar/config*.*.bak` | [ ] | [ ] | [ ] |
| Rollback tras install fallido | Error esperado, rollback ejecutado y diff byte a byte del sandbox igual al estado inicial | [ ] | [ ] | [ ] |

---

## H. Compatibilidad

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Binario musl estático funciona en contenedor sin glibc | `docker run --rm -v $PWD:/app alpine /app/lazysubs-eye --version` | [ ] | [ ] | [ ] |
| Omarchy: windowrule con hyprland.conf creado | `grep lazysubs-eye ~/.config/hypr/hyprland.conf` | [ ] | [ ] | [ ] |
| Non-Omarchy: fallbacks de CSS y terminal usados | Sistema test sin `~/.local/share/omarchy` | — | [ ] | [ ] |
| Waybar config: funciona con `config.jsonc` y `config` | Dos installs en sandboxes distintos | — | [ ] | [ ] |

---

## I. Documentación

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| README actualizado con quickstart y muestra visual | Verificar Installation, Compatibility, Privacy y Troubleshooting; captura real obligatoria para Stable | [ ] | [ ] | [ ] |
| ARQUITECTURA.md refleja código actual | `git diff HEAD~10 docs/ARQUITECTURA.md` | [ ] | [ ] | [ ] |
| SECURITY.md existe con instrucciones de reporte | `cat docs/SECURITY.md` | — | [ ] | [ ] |
| CONTRIBUTING.md existe | `cat CONTRIBUTING.md` | — | [ ] | [ ] |
| CHANGELOG strategy documentada (keep a changelog) | `cat docs/CHANGELOG*` o sección en CONTRIBUTING | — | [ ] | [ ] |
| docs/AUDITORIA-PREPUBLICACION.md hace referencia a este plan | `grep PLAN-PUBLICACION docs/AUDITORIA-PREPUBLICACION.md` | [ ] | [ ] | [ ] |

---

## J. Empaquetado

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| PKGBUILD pasa `namcap`, si se publica en AUR | `namcap packaging/aur/PKGBUILD` | N/A o [ ] | N/A o [ ] | N/A o [ ] |
| PKGBUILD sha256 coincide con binario publicado | `sha256sum target/release/lazysubs-eye` vs PKGBUILD | [ ] | [ ] | [ ] |
| AUR package installs sin errores en clean chroot | Test en container con `extra-x86_64-build` | — | [ ] | [ ] |
| SBOM adjunto, si la decisión de Stable lo adopta | Asset y procedimiento de generación documentado | — | — | N/A o [ ] |

---

## K. GitHub release

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Tag firmado o commit objetivo verificado | `git tag -v "$TAG"` o `git rev-parse "$TAG^{commit}"` | [ ] | [ ] | [ ] |
| Release notes incluyen: cambios, breaking changes, known issues | Contenido de GitHub release | [ ] | [ ] | [ ] |
| Binario estático adjuntado | Assets de release en GitHub | [ ] | [ ] | [ ] |
| Checksums adjuntados | Assets de release en GitHub | [ ] | [ ] | [ ] |

---

## L. Smoke tests del artefacto

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| `--version` retorna 0 y muestra versión | `./lazysubs-eye --version; echo $?` → 0 | [ ] | [ ] | [ ] |
| `--help` muestra ayuda | `./lazysubs-eye --help | head -20` | [ ] | [ ] | [ ] |
| Modo JSON sin providers produce JSON válido | `./lazysubs-eye --json` validado con `jq` y schema esperado | [ ] | [ ] | [ ] |
| `--check` sin providers retorna error | `./lazysubs-eye --check`; código 3 según contrato | [ ] | [ ] | [ ] |
| Modo TUI arranca sin panic | Prueba manual en PTY con salida limpia y restauración del terminal | [ ] | [ ] | [ ] |

---

## M. Beta y soak

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Beta testers asignados y notificados | Lista en issue GitHub | [ ] | [ ] | [ ] |
| Periodo de soak (mínimo 7 días) documentado | Issue con fecha de inicio/fin | [ ] | [ ] | [ ] |
| Bugs bloqueantes de beta resueltos antes de RC | `git log --oneline` desde tag beta | — | [ ] | [ ] |
| Feedback de beta testers incorporado | Commits o issues linkeados | — | [ ] | [ ] |

---

## N. Post-release y rollback

| Ítem | Evidencia / Comando | Beta | RC | Stable |
|------|---------------------|------|----|--------|
| Procedimiento de rollback documentado | Sección en docs/ con pasos exactos | — | — | [ ] |
| Versión anterior stayed en GitHub releases | Tag y assets anteriores accesibles | — | — | [ ] |
| AUR package rollback probado | Downgrade en sistema test | — | — | [ ] |
| Comunicación de rollback lista (si aplica) | Draft de announcement | — | — | [ ] |

---

## Registro de ejecución

| Fecha | Versión | Commit | Responsable | Resultado | Enlaces de evidencia |
|-------|---------|--------|-------------|-----------|----------------------|
| — | — | — | — | — | — |
| — | — | — | — | — | — |
| — | — | — | — | — | — |

_Instrucciones_: antes de cada release, ejecutar cada sección aplicable y
registrar el resultado en esta tabla. No dejar ítems en blanco: si no aplica,
escribir "N/A" con la razón. Si falla, documentar en la columna de enlaces
el issue o commit que lo resuelve.
