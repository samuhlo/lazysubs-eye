# Tasks — observable-health-diagnostics

status: complete
blocked_by: secure-local-persistence, reliable-history-ingestion

> Cada ciclo TDD cubre una capacidad funcional completa: RED (test que falla antes
> de implementar), GREEN (implementación mínima para pasar), TRIANGULATE (casos
> borde/adicionales), REFACTOR (limpieza, integración, revisión de comentarios).
> Al final se añaden fases separadas para documentación, suite completo, y
> preparación de apply-progress.md/verify-report.md durante ejecución.

**Contrato de exit codes (0-3, SIN 4):**
- 0: OK, datos frescos y sin umbrales activos
- 1: warning, Stale o Partial
- 2: umbral Critical
- 3: error operativo, ningún provider o config corrupta

**Decisión abierta (no implementar exit code 4):** Si notify-send falla,
esto se registra en `doctor`, diagnóstico/verbose y stderr, pero NO crea
un nuevo exit code. La decisión de añadir un exit code 4 específico para
notify-send requeriría una decisión explícita futura.

## // 001. Tipos de error con códigos cortos y validación de config

- [x] 1.1 (RED) Test que verifica `LazysubsError` enum con códigos E001...
  y Display que muestra código + descripción accionable; tests de
  ConfigValidator que verifican base_url válido, warning < critical,
  ttl > 0, history_days > 0
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::error_codes_*` — rojos;
    `cargo test --lib diagnostics::tests::config_validation_*` — rojos

- [x] 1.2 (GREEN) Implementar `LazysubsError` enum con E001 ConfigParseError,
  E002 ConfigValidationError, E003 ProviderUnavailable, E004 ProviderStale,
  E005 ProviderNotConfigured, E006 PermissionDenied, E007 BinaryNotFound,
  E008 NotifySendNotFound; implementar `ConfigValidator::validate` y su
  integración en `config::load()`
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::error_codes_*` — de rojo a verde;
    `cargo test --lib config::tests::load_with_validation_*` — verde

- [x] 1.3 (TRIANGULATE) Tests: ConfigValidationError con mensaje accionable;
  múltiples errores de validación collectados; E008 NotifySendNotFound
  no produce exit code 4 (se registra en doctor/verbose/stderr)
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::error_display_*` — verde;
    `cargo test --lib diagnostics::tests::notify_send_*` — verde:
    notify-send ausente → visible en doctor; exit code sin cambios (0-3)

- [x] 1.4 (REFACTOR) Documentar qué significa cada código; verificar que
  Display no expone secrets; sanitizar mensajes SQLite/HTTP
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `cargo clippy --all-targets -- -D warnings`; revisión visual

## // 002. --check con exit codes diferenciados

- [x] 2.1 (RED) Test de integración que invoca `lazysubs-eye --check` y
  verifica exit codes: 0 (ready), 1 (warning/stale/partial), 2 (critical),
  3 (unavailable/not configured/config error);
  test que verifica que notify-send failure NO produce exit code 4
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --test cli_check` — rojos para cada exit code

- [x] 2.2 (GREEN) Implementar `check_status() -> CheckResult` que inspeccione
  cada provider y retorne struct con `overall: ExitCode` (0-3) y
  `providers: Vec<ProviderStatus>`; modificar entry point para que
  `--check` use `check_status()` y retorne exit code correspondiente
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --test cli_check` — de rojo a verde:
    ready → 0; stale → 1; critical → 2; unavailable/not configured → 3

- [x] 2.3 (TRIANGULATE) Tests: --check con provider mixto (uno ready, otro stale);
  --check cuando todos fallan; --check sin config; output sin credenciales
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::check_status_*` — verde;
    `cargo test --test cli_check` — verde

- [x] 2.4 (REFACTOR) Verificar que exit code viene de `check_status()`,
  no hardcoded; documentar el contrato 0-3 en scope.md; actualizar --help
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `lazysubs-eye --help | grep -A 10 EXITCODES`; revisión de scope.md

## // 003. Comando doctor y doctor --json

- [x] 3.1 (RED) Test que verifica `doctor` ejecuta checks: config parseable,
  providers configurados, paths existen, binary info, database health,
  permisos, last error; `doctor --json` devuelve JSON estructurado
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::doctor_*` — rojos;
    `cargo test --lib diagnostics::tests::doctor_json_*` — rojos

- [x] 3.2 (GREEN) Implementar `doctor() -> DoctorReport` con `Vec<DoctorCheck>`
  (name, status: Pass/Fail/Warn, message); implementar `--json` con
  Serialize y serde_json::to_string_pretty
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::doctor_*` — de rojo a verde;
    `cargo test --lib diagnostics::tests::doctor_json_*` — verde

- [x] 3.3 (TRIANGULATE) Tests: doctor con config corrupta; doctor sin providers;
  doctor con notify-send fallando (visible, sin exit code 4); doctor --json
  con sistema sano
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::doctor_*` — verde;
    verify que notify-send failure aparece en doctor output y stderr/verbose

- [x] 3.4 (REFACTOR) Documentar qué checks hace doctor; verificar que ningún
  check expone secrets; guardar último error en meta para doctor
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de doctor; grep por secrets en output

## // 004. Errores accionables y sanitización

- [x] 4.1 (RED) Test que verifica `sanitize_error(err) -> String` reemplaza
  paths con ~, remueve tokens/keys, normaliza mensajes SQLite; test que
  grep no encuentra format!("{e}") sobre errores relevantes sin sanitizar
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::sanitize_*` — rojos;
    grep por format!("{e}") sin sanitizar → debería fallar

- [x] 4.2 (GREEN) Implementar `sanitize_error<T: Display>(err: T) -> String`;
  reemplazar todos los format!("{e}") sobre errores de std/fs/SQLite con
  sanitize_error()
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::sanitize_*` — de rojo a verde:
    path completo → ~; API key expuesta → [REDACTED]; SQLite error → generic
    grep no encuentra format!("{e}") sobre errores relevantes

- [x] 4.3 (TRIANGULATE) Tests: ruta con `/home/usuario/...` → `~/...`; errores
  que contienen URLs → sanitizadas; mensajes de error multilínea
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::sanitize_*` — verde

- [x] 4.4 (REFACTOR) Verificar over-sanitize no oculta información útil;
  under-sanitize no expone secrets; documentar regex de sanitización
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de casos edge; `cargo clippy --all-targets -- -D warnings`

## // 005. --verbose y logs de diagnóstico

- [x] 5.1 (RED) Test que verifica `--verbose` emite stderr: collector started,
  cache checkpoint, refresh decision; sin --verbose no se ven logs
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::verbose_*` — rojos

- [x] 5.2 (GREEN) Implementar logging con `--verbose` que use tracing o log
  con filtro de nivel; `--verbose` activa el nivel de diagnóstico equivalente
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::verbose_*` — de rojo a verde:
    con --verbose se ven logs; sin --verbose no se ven

- [x] 5.3 (TRIANGULATE) Tests: verbose con notify-send failure visible en
  stderr; verbose con config inválida; verbose con provider unavailable
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib diagnostics::tests::verbose_*` — verde

- [x] 5.4 (REFACTOR) Verificar que logs no revelan secrets; documentar
  qué se loguea en verbose
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: grep por secrets en logs; revisión de comentarios

## // 006. Suite completo y preparación

- [x] 6.1 (VERIFY) Ejecutar suite completo: `cargo test --locked`
  - skills: `ein-discipline`
  - verify: todos los tests pasan

- [x] 6.2 (VERIFY) Quality gates finales: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
  - skills: `ein-discipline`
  - verify: sin errores

- [x] 6.3 (VERIFY) Preparar apply-progress.md con: tareas completadas, archivos tocados,
  decisiones técnicas (incluyendo decisión de no usar exit code 4 para notify-send),
  riesgos, siguiente paso
  - skills: `ein-discipline`
  - verify: apply-progress.md existe y está completo tras ejecución

- [x] 6.4 (VERIFY) Preparar verify-report.md con: comandos ejecutados, output relevante,
  evidencia de que cada gate pasó, verificación de que exit codes son 0-3
  - skills: `ein-discipline`
  - verify: verify-report.md existe y contiene evidencia tras verificación

## // 007. Documentación

- [x] 7.1 (DOCS) Documentar exit codes en --help: 0 OK, 1 warning/stale/partial,
  2 critical, 3 error operativo/no configurado/config inválida
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `lazysubs-eye --help | grep -A 10 EXITCODES`

- [x] 7.2 (DOCS) Documentar decisión de no usar exit code 4: notify-send failure
  visible en doctor/verbose/stderr pero sin nuevo exit code
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de scope.md y design.md

- [x] 7.3 (VERIFY) Verificar mediante tests de integración que el contrato
  público solo usa 0 OK, 1 warning, 2 critical y 3 error
  - skills: `ein-discipline`
  - verify: `cargo test --test cli_check`
