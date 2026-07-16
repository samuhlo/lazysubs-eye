# Mapa: UX adaptable de la TUI

status: partial
scope_status: bounded
change: adaptive-tui-ux
phase: map
skill_resolution: pending
budget_consumed: {tokens: 0, reads: 0}

## Decisión principal

Unificar el modelo de estados en todos los paneles, añadir scroll, guard RAII
para loading, respetar NO_COLOR y UTF-8 fallback, y hacer los estados
distinguibles para daltónicos.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado |
|---|---|---|
| `src/tui.rs` | Estados existentes pero no uniformes | `PanelState` enum + cada panel lo usa |
| `src/tui.rs` | Sin scroll; contenido se corta | `layout_with_scroll()` |
| `src/tui.rs` | Sin guard RAII para loading | `LoadingGuard` |
| Colores ANSI | Siempre activos | `should_use_color()` checks NO_COLOR |

Decisión de señales: `Ctrl+C` es evento de salida en raw mode y
`RestoreTerminal` cubre todos los retornos/unwind. No se instala handler SIGTERM
en esta versión.
| Íconos UTF-8 | No hay fallback ASCII | `icon_to_ascii()` |
| Estados por color | Solo color | Carácter + color |
| Sin help | No existe | `draw_help()` |

## Archivos concretos

| Archivo | Cambio |
|---|---|
| `src/tui/state.rs` (nuevo) | `PanelState` enum |
| `src/tui/layout.rs` (nuevo) | `layout_with_scroll()` |
| `src/tui/guard.rs` (nuevo) | `LoadingGuard` |
| `src/tui/color.rs` (nuevo) | `should_use_color()` |
| `src/tui/ascii.rs` (nuevo) | `icon_to_ascii()` |
| `src/tui.rs` | Unificar estados; loading con guard; help overlay |

## Puntos de riesgo

1. **Breaking change en estados**: si los tipos existentes de cada panel se
   usan en JSON output, cambiar a `PanelState` puede romper. **Mitigación**:
   verificar que JSON/Waybar no dependen de los tipos internos de cada panel.
2. **Scroll stateful**: si el usuario hace scroll en un panel y cambia de
   panel, ¿el scroll se reset? **Decisión**: scroll es por panel, no global.

## Rollback

Revertir los cambios de cada archivo. El modelo de estados vuelve a ser
inconsistente.

## Dependencias con otros paquetes

| Paquete | Relación |
|---------|----------|
| `reliable-history-ingestion` | Depende del modelo de estados `IngestState` y la estructura de backfill para mostrar estados de ingesta en la TUI |
| `observable-health-diagnostics` | Aporta la semántica normalizada de salud y errores que la TUI presenta mediante `PanelState` |

**Nota**: el diseño puede avanzar en paralelo; la implementación requiere los contratos de ambos paquetes.

## Siguiente fase

Pasar a `sdd-design` con este mapa. Esta fase no ejecutó build ni tests.
