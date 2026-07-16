# Tasks — safe-system-integration

status: complete
blocked_by: none

> Cada ciclo TDD cubre una capacidad funcional completa: RED (test que falla antes
> de implementar), GREEN (implementación mínima para pasar), TRIANGULATE (casos
> borde/adicionales), REFACTOR (limpieza, integración, revisión de comentarios).
> Al final se añaden fases separadas para documentación, suite completo, y
> preparación de apply-progress.md/verify-report.md durante ejecución.

## // 001. Tipos de error y preflight checks

- [x] 1.1 (RED) Test que verifica `InstallError` enum con variants
  `BinaryNotFound`, `ConfigNotWritable`, `WaybarConfigNotFound`,
  `OwnershipConflict`, `RollbackFailed`, `MarkerMismatch`; y que preflight_install
  y preflight_uninstall detectan cada caso
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::install_error_*` — rojos;
    `cargo test --lib install::tests::preflight_*` — rojos

- [x] 1.2 (GREEN) Implementar `InstallError` enum en `src/install.rs` y
  `preflight_install(ctx: &InstallContext) -> Vec<InstallError>` que verifique:
  binario ejecutable, config dirs writables, waybar config exists
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::preflight_*` — de rojo a verde:
    binary not found → error; dir not writable → error; waybar config missing → error

- [x] 1.3 (TRIANGULATE) Tests adicionales: preflight_uninstall verifica marcadores
  existen y archivos son escribibles; múltiples errores se collectan en un vector
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::preflight_uninstall_*` — verde:
    markers missing → error; files not writable → error

- [x] 1.4 (REFACTOR) Revisar comentarios de InstallError y preflight;
  verificar que errores son accionables y no exponen información sensible
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `cargo clippy --all-targets -- -D warnings`; revisión visual

## // 002. Plan, dry-run y sandbox mode

- [x] 2.1 (RED) Test que verifica `InstallPlan` struct serializa correctamente;
  dry-run no modifica archivos; sandbox modifica solo el sandbox
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::install_plan_*` — rojos;
    `cargo test --lib install::tests::sandbox_*` — rojos

- [x] 2.2 (GREEN) Implementar `InstallPlan` con campos `files_to_modify`,
  `backups_to_create`, `commands_to_run`, `files_to_delete`; separar fase de
  plan de fase de ejecución en `install()`
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::install_plan_*` — de rojo a verde;
    `cargo test --lib install::tests::install_dry_run_*` — verde: dry-run no modifica

- [x] 2.3 (TRIANGULATE) Tests: sandbox con dry-run no toca real; sandbox sin
  dry-run modifica solo el sandbox; execute_plan aplica cambios reales
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::sandbox_*` — verde

- [x] 2.4 (REFACTOR) Verificar que dry-run serializa el plan completo;
  factorizar lógica para no duplicar entre dry-run y execute
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo clippy --all-targets -- -D warnings`; revisión de diff

## // 003. Resolución del binario durante install

- [x] 3.1 (RED) Test que verifica `resolve_binary_path()` retorna la ruta real
  del binario en ejecución; falla si ni `current_exe()` ni `/proc/self/exe`
  funcionan
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::resolve_binary_*` — rojos

- [x] 3.2 (GREEN) Implementar `resolve_binary_path() -> Result<PathBuf, InstallError>`
  que use `std::env::current_exe()` o `/proc/self/exe`; modificar
  la generación de polling y click para usar la ruta resuelta
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::resolve_binary_*` — de rojo a verde;
    `cargo test --lib install::tests::windowrule_uses_resolved_path` — verde

- [x] 3.3 (TRIANGULATE) Tests: binario en PATH diferente, symlink al binario,
  binario renombrado pero en ejecución
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::resolve_binary_edge_*` — verde

- [x] 3.4 (REFACTOR) Documentar por qué se evita hardcodear ~/.local/bin;
  verificar que todos los comandos generados usan la ruta resuelta durante install
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; `cargo clippy --all-targets -- -D warnings`

## // 004. Ownership y validación de marcadores

- [x] 4.1 (RED) Test que verifica `validate_markers` detecta contenido editado
  entre marcadores; `check_manual_rules_between_markers` detecta reglas manuales
  añadidas por el usuario
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::marker_validation_*` — rojos;
    `cargo test --lib install::tests::manual_rules_*` — rojos

- [x] 4.2 (GREEN) Implementar `validate_markers(path: &Path) -> Result<MarkerValidation, InstallError>`
  y `check_manual_rules_between_markers(path: &Path) -> Vec<LineConflict>`
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::marker_validation_*` — de rojo a verde:
    markers intactos → Ok; content edited → MarkerMismatch
    `cargo test --lib install::tests::manual_rules_*` — verde: rules manuales → detected

- [x] 4.3 (TRIANGULATE) Tests: uninstall con reglas manuales preguntando;
  uninstall sin marcadores (versión anterior); markers faltantes tras edición
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::uninstall_with_manual_rules_*` — verde

- [x] 4.4 (REFACTOR) Verificar que uninstall no borra reglas manuales;
  documentar el formato de marcadores y la heuristic para distinguir reglas
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por marcadores en tests

## // 005. Transacción y rollback

- [x] 5.1 (RED) Test que verifica `BackupManager` crea backups con sufijo `.bak.<epoch>`;
  rollback restaura archivos en orden inverso; rollback que falla retorna lista
  de archivos afectados
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::backup_manager_*` — rojos;
    `cargo test --lib install::tests::rollback_failure_*` — rojos

- [x] 5.2 (GREEN) Implementar `BackupManager::new()`, `backup(file)`,
  `rollback()`; modificar `execute_plan` para usar BackupManager y rollback
  automático ante cualquier error
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::backup_manager_*` — de rojo a verde:
    backup crea .bak; second backup same second → .bak.epoch+1
    `cargo test --lib install::tests::rollback_*` — verde: step 3 fails → steps 1-2 restored

- [x] 5.3 (TRIANGULATE) Tests: rollback que falla él mismo retorna error con
  lista de archivos que quedaron modificados; backup de archivo que no existe
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::rollback_failure_*` — verde:
    rollback itself fails → error with affected files list

- [x] 5.4 (REFACTOR) Documentar por qué los backups no se sobrescriben;
  verificar que execute_plan hace backup antes de modificar cada archivo
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de BackupManager y execute_plan

## // 006. Tests de integración, idempotencia y calidad

- [x] 6.1 (RED) Test de integración completo: install en temp dir con dry-run +
  sandbox; uninstall idempotente; rollback tras falla; install dos veces =
  mismo estado
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::integration_*` — rojos;
    `cargo test --lib install::tests::idempotency_*` — rojos

- [x] 6.2 (GREEN) Implementar la lógica necesaria para que los tests de
  integración e idempotencia pasen
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::integration_*` — de rojo a verde;
    `cargo test --lib install::tests::idempotency_*` — verde:
    install dos veces = mismo estado; uninstall dos veces = mismo estado

- [x] 6.3 (TRIANGULATE) Tests adicionales: install con config preexistente;
  uninstall con marcadores de versión anterior; rollback parcial
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib install::tests::*` — suite completo verde

- [x] 6.4 (REFACTOR) Limpiar imports muertos; verificar comentarios;
  quality gates
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## // 007. Documentación

- [x] 7.1 (DOCS) Documentar en comentarios de install.rs: preflight antes de cualquier
  modificación; dry-run y sandbox como simulación; rollback automático
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión visual de comentarios de funciones públicas

- [x] 7.2 (DOCS) Registrar tamaño y tiempo de nueva dependencia en verify-report.md
  tras `cargo build --release` (si hay nuevas deps)
  - skills: `ein-discipline`
  - verify: verify-report.md contiene evidencia

## // 008. Suite completo y preparación

- [x] 8.1 (VERIFY) Ejecutar suite completo: `cargo test --locked`
  - skills: `ein-discipline`
  - verify: todos los tests pasan

- [x] 8.2 (VERIFY) Quality gates finales: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
  - skills: `ein-discipline`
  - verify: sin errores

- [x] 8.3 (VERIFY) Preparar apply-progress.md con: tareas completadas, archivos tocados,
  decisiones técnicas, riesgos, siguiente paso
  - skills: `ein-discipline`
  - verify: apply-progress.md existe y está completo tras ejecución

- [x] 8.4 (VERIFY) Preparar verify-report.md con: comandos ejecutados, output relevante,
  evidencia de que cada gate pasó
  - skills: `ein-discipline`
  - verify: verify-report.md existe y contiene evidencia tras verificación
