# Mapa: diagnóstico de salud y observabilidad

status: partial
scope_status: bounded
change: observable-health-diagnostics
phase: map
skill_resolution: pending
budget_consumed: {tokens: 0, reads: 0}

## Decisión principal

Separar los exit codes de `--check` (0-3 SOLAMENTE, SIN 4), añadir `doctor`,
validar config semánticamente, sanitizar todos los errores, y documentar
los exit codes. notify-send failure visible en doctor/verbose/stderr pero
sin nuevo exit code.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado |
|---|---|---|
| `src/main.rs` `--check` | Contrato 0 OK, 1 warning, 2 critical, 3 error, pero casos operativos ambiguos | Extraer `check_status()` y preservar el contrato con estados explícitos |
| `src/config.rs` | Solo parseo, sin validación semántica | Añadir `ConfigValidator` |
| Errores distribuidos | Mensajes raw de SQLite/HTTP | Centralizar sanitización con `sanitize_error()` |
| `--help` | Sin sección de exit codes | Añadir sección EXITCODES (0-3) |
| Sin `doctor` | No existe | Crear comando nuevo |
| Sin `--verbose` | No existe | Implementar logging con filtro |
| Sin binary name fix | `lazysubs` (historico) | N/A — ya se usa `lazysubs-eye` |

## Archivos concretos

| Archivo | Rol | Cambio |
|---|---|---|
| `src/main.rs` | Entry point | `check_status()`; exit codes 0-3; `doctor` command; `lazysubs-eye` |
| `src/error.rs` (nuevo) | `LazysubsError` enum con códigos E001... |
| `src/config.rs` | Validación semántica | `ConfigValidator` |
| `src/diagnostics.rs` (nuevo) | `DoctorReport`, `sanitize_error`, `check_status` |

## Puntos de riesgo

1. **Exit codes conflictivos**: si los exit codes existentes ya se usan de otra forma,
   cambiarlos rompe scripts existentes. **Mitigación**: mapear a los mismos
   valores públicos: 0=OK, 1=warning, 2=critical, 3=error.
2. **Over-sanitization**: si sanitization es muy agresiva, los errores son
   inútiles. **Mitigación**: tests de cada función de sanitization.
3. **Doctor en entorno sin waybar/hyprland**: puede fallar checks que no
   aplican. **Mitigación**: cada check tiene modo skip si no aplica.
4. **Exit code 4**: no se introduce. notify-send failure se registra en
   doctor/verbose/stderr. **Decisión futura** si se necesita exit code 4.

## Rollback

Revertir main.rs, config.rs y los nuevos módulos error/diagnostics. El
comportamiento de --check vuelve al anterior.

## Dependencias

| Paquete | Relación |
|---------|----------|
| `secure-local-persistence` | Implementación depende de los tipos `AtomicSaveError` y `FileLock` para sanitización de errores y locking de status.json |
| `reliable-history-ingestion` | Implementación depende de los estados `IngestState` para `--check` semántico y `doctor` |

**Nota**: el diseño de este paquete puede avanzar sin dependencias; la implementación completa requiere los contratos de `secure-local-persistence` y `reliable-history-ingestion`.

## Siguiente fase

Pasar a `sdd-design` con este mapa. Esta fase no ejecutó build ni tests.
