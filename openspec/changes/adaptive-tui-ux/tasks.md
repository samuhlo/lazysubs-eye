# Tasks — adaptive-tui-ux

status: complete
blocked_by: reliable-history-ingestion, observable-health-diagnostics

> Cada ciclo TDD cubre una capacidad funcional completa: RED (test que falla antes
> de implementar), GREEN (implementación mínima para pasar), TRIANGULATE (casos
> borde/adicionales), REFACTOR (limpieza, integración, revisión de comentarios).
> Al final se añaden fases separadas para documentación, suite completo, y
> preparación de apply-progress.md/verify-report.md durante ejecución.

## // 000. Decisión de idioma de la UI (SPIKE)

- [x] 0.1 (SPIKE) Elegir una sola lengua para la UI v1: español o inglés.
  README en inglés y docs internos en español ya están decididos; UI bilingüe
  y terceros idiomas quedan fuera de v1.
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: decisión documentada en design.md y scope.md; mapa actualizado

## // 001. Modelo de estados uniforme

- [x] 1.1 (RED) Test que verifica `PanelState` enum con variants Loading,
  Ready, Empty, Partial, Unavailable, Stale, NotConfigured; cada panel
  retorna PanelState
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::panel_state_*` — rojos;
    `cargo test --lib tui::tests::all_panels_state_*` — rojos

- [x] 1.2 (GREEN) Definir PanelState en src/tui/state.rs; modificar cada
  panel (Claude, Pi, OpenCode, History) para usar PanelState
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::panel_state_*` — de rojo a verde;
    `cargo test --lib tui::tests::all_panels_state_*` — verde

- [x] 1.3 (TRIANGULATE) Tests: cada estado renderiza correctamente;
  PanelState serializa a JSON correctamente
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::panel_state_serde_*` — verde

- [x] 1.4 (REFACTOR) Documentar por qué se unifica el modelo de estados;
  verificar que PanelState no pierde información específica de cada panel
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por PanelState en código

## // 002. Scroll y layout adaptable

- [x] 2.1 (RED) Test que verifica `layout_with_scroll` calcula qué líneas
  caben; contenido de 30 líneas en terminal de 10 → primeras 10 + scroll indicator
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::scroll_*` — rojos;
    `cargo test --lib tui::tests::small_terminal_*` — rojos

- [x] 2.2 (GREEN) Implementar `layout_with_scroll(area: Rect, content: &[Line])
  -> Vec<Line>`; modificar `App::draw` para usar layout_with_scroll en cada panel
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::scroll_*` — de rojo a verde:
    contenido de 30 líneas en terminal de 10 → primeras 10 + scroll indicator
    `cargo test --lib tui::tests::small_terminal_*` — verde: 80×24 → sin panic

- [x] 2.3 (TRIANGULATE) Tests: scroll con scroll indicator visible;
  scroll que llega al final; resize durante scroll
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::scroll_edge_*` — verde

- [x] 2.4 (REFACTOR) Documentar por qué se calcula en render y no en state;
  verificar que scroll es stateful por panel
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; `cargo clippy --all-targets -- -D warnings`

## // 003. Guard RAII para loading

- [x] 3.1 (RED) Test que verifica `LoadingGuard` con timeout y Drop libera
  estado loading; guard expirado llama clear_loading
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::loading_guard_*` — rojos;
    `cargo test --lib tui::tests::scan_with_guard_*` — rojos

- [x] 3.2 (GREEN) Implementar `LoadingGuard` struct con source y expire_at;
  Drop llama App::clear_loading(source); modificar begin_*_scan() para
  adquirir LoadingGuard
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::loading_guard_*` — de rojo a verde:
    guard expirado → clear_loading llamado
    `cargo test --lib tui::tests::scan_with_guard_*` — verde:
    guard adquirido; guard liberado tras resultado

- [x] 3.3 (TRIANGULATE) Tests: guard expirado tras timeout; guardLiberado
  manualmente; guard no se pierde si worker responde rápido
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::loading_guard_edge_*` — verde

- [x] 3.4 (REFACTOR) Documentar por qué se usa RAII; verificar que no hay
  memory leak si worker no responde
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de LoadingGuard

## // 004. NO_COLOR y modo monocromático

- [x] 4.1 (RED) Test que verifica `should_use_color()` retorna false con
  NO_COLOR=1 o TERM=dumb; retorna true con FORCE_COLOR=1
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::should_use_color_*` — rojos;
    `cargo test --lib tui::tests::no_color_mode_*` — rojos

- [x] 4.2 (GREEN) Implementar `should_use_color() -> bool` en src/tui/color.rs
  que verifique NO_COLOR, TERM, FORCE_COLOR; modificar todos los render para
  usar should_use_color() al pintar colores
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::should_use_color_*` — de rojo a verde:
    NO_COLOR=1 → false; TERM=dumb → false; FORCE_COLOR=1 → true
    `cargo test --lib tui::tests::no_color_mode_*` — verde:
    NO_COLOR=1 → sin secuencias ANSI

- [x] 4.3 (TRIANGULATE) Tests: NO_COLOR=0 y TERM=xterm → color activo;
  combinaciones edge
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::should_use_color_edge_*` — verde

- [x] 4.4 (REFACTOR) Documentar la precedencia: FORCE_COLOR > NO_COLOR;
  verificar que ningún lugar aplica color sin check
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por should_use_color en render

## // 005. Fallback ASCII

- [x] 5.1 (RED) Test que verifica `supports_utf8()` retorna false con LANG=C;
  retorna true con LANG=en_US.UTF-8
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::utf8_support_*` — rojos;
    `cargo test --lib tui::tests::ascii_icons_*` — rojos

- [x] 5.2 (GREEN) Implementar `supports_utf8() -> bool` en src/tui/utf8.rs;
  implementar `icon_to_ascii` mapping (✳ → *, ⚠ → !, etc.)
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::utf8_support_*` — de rojo a verde:
    LANG=en_US.UTF-8 → true; LANG=C → false
    `cargo test --lib tui::tests::ascii_icons_*` — verde: ✳ → *; ⚠ → !; ✓ → [✓]

- [x] 5.3 (TRIANGULATE) Tests: LANG vacío; LANG=UTF-8 (mayúsculas);
  icono sin mapping
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::utf8_edge_*` — verde

- [x] 5.4 (REFACTOR) Documentar por qué se usa LANG y no TERM para UTF-8;
  verificar que todos los iconos tienen mapping
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por icon_to_ascii

## // 006. Estados para daltónicos

- [x] 6.1 (RED) Test que verifica Ready = [✓] (verde), Warning = [!] (amarillo),
  Critical = [✗] (rojo) además del color
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::colorblind_state_*` — rojos

- [x] 6.2 (GREEN) Modificar estilos de estados para incluir carácter además
  de color; carácter se añade al texto del estado, no al estilo
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::colorblind_state_*` — de rojo a verde:
    Ready con [✓]; Warning con [!]; Critical con [✗]

- [x] 6.3 (TRIANGULATE) Tests: carácter visible sin color; carácter visible
  con color; estados mixtos
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::colorblind_mixed_*` — verde

- [x] 6.4 (REFACTOR) Documentar qué caracteres se usan; verificar que se
  mantienen ambos canales (color + carácter)
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de estilos

## // 007. Help, señales y restauración de terminal

- [x] 7.1 (RED) Test que verifica overlay de ayuda con ? muestra teclas,
  acciones, abreviaturas, estados; cualquier key lo dismiss; Ctrl+C restaura
  terminal; panic simulado restaura terminal
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::help_*` — rojos;
    `cargo test --lib tui::tests::terminal_restore_*` — rojos

- [x] 7.2 (GREEN) Implementar `draw_help()` como overlay cuando show_help=true;
  implementar restauración de terminal ante error o panic con ratatui::restore();
  spike de señales: decidir SIGINT/SIGTERM handling
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::help_*` — de rojo a verde:
    ? → overlay visible; cualquier key → dismiss
    `cargo test --lib tui::tests::terminal_restore_*` — verde:
    Ctrl+C → terminal restaurado; panic simulado → terminal restaurado

- [x] 7.3 (TRIANGULATE) Tests: help con terminal pequeño; help con estado de
  loading; señal durante help dismisses help
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::help_edge_*` — verde

- [x] 7.4 (REFACTOR) Documentar alcance de señales decidido en spike;
  actualizar scope.md y map.md con la decisión
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de scope.md y map.md; grep por signal en decisiones

## // 008. Suite completo y preparación

- [x] 8.1 (VERIFY) Ejecutar suite completo: `cargo test --locked`
  - skills: `ein-discipline`
  - verify: todos los tests pasan

- [x] 8.2 (VERIFY) Quality gates finales: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
  - skills: `ein-discipline`
  - verify: sin errores

- [x] 8.3 (VERIFY) Preparar apply-progress.md con: tareas completadas, archivos tocados,
  decisiones técnicas (caracteres elegidos, señales), riesgos, siguiente paso
  - skills: `ein-discipline`
  - verify: apply-progress.md existe y está completo tras ejecución

- [x] 8.4 (VERIFY) Preparar verify-report.md con: comandos ejecutados, output relevante,
  evidencia de que cada gate pasó
  - skills: `ein-discipline`
  - verify: verify-report.md existe y contiene evidencia tras verificación

## // 009. Documentación

- [x] 9.1 (DOCS) Documentar en scope.md: decisión de caracteres para daltónicos,
  decisión de manejo de señales
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de scope.md y map.md
