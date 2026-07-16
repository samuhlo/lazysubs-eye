# Tasks — secure-local-persistence

status: complete
blocked_by: none

> Cada ciclo TDD cubre una capacidad funcional completa: RED (test que falla antes
> de implementar), GREEN (implementación mínima para pasar), TRIANGULATE (casos
> borde/adicionales), REFACTOR (limpieza, integración, revisión de comentarios).
> Al final se añaden fases separadas para documentación, suite completo, y
> preparación de apply-progress.md/verify-report.md durante ejecución.

## // 000. Decisión de symlinks

- [x] 0.1 (SPIKE) Evaluar rechazo, seguimiento seguro y otras políticas de
  symlinks; registrar comportamiento de destino, padres, cadenas, ciclos y
  escapes antes de escribir tests funcionales
  - skills: `ein-discipline`, `architecture`
  - verify: decisión y criterios observables registrados en design.md y scope.md

## // 001. AtomicSaveError y helper de permisos restrictivos

- [x] 1.1 (RED) Test que verifica `AtomicSaveError` enum con variantes
  `SymlinkPolicyViolation`, `LockNotAvailable`, `LockTimeout`, `LockLost`,
  `PermissionChangeFailed`, `Io(std::io::Error)` y que `set_permissions_restrictive`
  falla en filesystem sin soporte de permisos
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::atomic_save_error_*` — todos rojos

- [x] 1.2 (GREEN) Implementar `AtomicSaveError` enum en `src/cache.rs` y
  `set_permissions_restrictive(path: &Path, is_dir: bool) -> Result<(), AtomicSaveError>`
  que use `std::fs::set_permissions` con 0600 o 0700 según is_dir
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::permissions_*` — de rojo a verde:
    archivo 0644 → 0600; dir 0755 → 0700; filesystem incompatible → error

- [x] 1.3 (TRIANGULATE) Tests adicionales para la política elegida: destino,
  padre, cadena, ciclo y escape de ámbito
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::symlink_policy_*`

- [x] 1.4 (REFACTOR) Revisar comentarios de `atomic_save` y `set_permissions_restrictive`;
  verificar que ninguno expone detalles de implementación; limpiar imports muertos
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `cargo clippy --all-targets -- -D warnings`; revisión visual de comentarios

## // 002. Política de symlinks y atomicidad

- [x] 2.1 (RED) Test que verifica `atomic_save` cumple la política elegida sin
  reemplazo silencioso ni escape; temp file se crea con permisos 0600 desde el inicio;
  fsync del directorio tras rename; chmod final tras rename
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::atomic_save_*` — rojos para:
    symlink fuera → Err; temp sin 0600; fsync no llamado; chmod no llamado

- [x] 2.2 (GREEN) Modificar `atomic_save` para: (a) aplicar la política elegida,
  (b) escribir temp con permisos 0600 desde el inicio,
  (c) hacer fsync del directorio tras rename exitoso, (d) chmod final tras rename
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib cache::tests::atomic_save_*` — de rojo a verde

- [x] 2.3 (TRIANGULATE) Tests de integración: archivo original intacto tras fallo
  de rename; directorio sincronizado tras commit; chmod llamado tras rename exitoso
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::atomic_save_integration_*` — verde

- [x] 2.4 (REFACTOR) Verificar que `atomic_save` no expone paths crudos en errores;
  sanitizar cualquier mensaje SQLite o IO error antes de retornar
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib`; grep por mensajes de error sin sanitizar

## // 003. FileLock multiproceso: spike y decisión de implementación

- [x] 3.0 (SPIKE) Evaluar opciones de exclusión multiproceso: flock(2), lockfile
  advisory, u otra compatible Unix. Documentar trade-offs: portabilidad, timeout,
  deadlock risk, kernel support. Decisión documentada antes de implementar 3.1
  - skills: `ein-discipline`, `architecture`
  - verify: documento de spike con decisión documentada y trade-offs en design.md

- [x] 3.1 (RED) Test que verifica `FileLock`, `LockMode` (Blocking, NonBlocking),
  `FileLockGuard` y adquisición/liberación con la alternativa elegida
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib file_lock::tests::*` — todos rojos

- [x] 3.2 (GREEN) Crear `src/file_lock.rs` con `FileLock`, `LockMode` y `FileLockGuard`;
  implementar `FileLock::acquire(path: &Path, mode: LockMode, timeout: Duration)
  -> Result<FileLockGuard, AtomicSaveError>` según decisión del spike
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib file_lock::tests::*` — de rojo a verde:
    disponible → Ok; ocupado nb → LockNotAvailable; ocupado timeout → LockTimeout

- [x] 3.3 (TRIANGULATE) Tests multiproceso reales: dos procesos concurrentes,
  uno adquiere lock, otro recibe LockNotAvailable; guard libera en Drop;
  lock perdido durante escritura → archivo destino intacto
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib file_lock::tests::acquire_*` — inter-proceso real

- [x] 3.4 (REFACTOR) Documentar en comentarios por qué se eligió flock/advisory
  lock y sus limitaciones; verificar que no hay deadlock possible con timeout
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de `FileLock`; `cargo clippy --all-targets -- -D warnings`

## // 004. Integración con status.json y atomic_save_locked

- [x] 4.1 (RED) Test que verifica que `atomic_save_locked` adquiere el lock antes de escribir;
  dos procesos concurrentes: uno Ok, otro LockNotAvailable
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib file_lock::tests::atomic_save_locked_*` — rojos

- [x] 4.2 (GREEN) Añadir `atomic_save_locked(path: &Path, data: &[u8],
  lock_mode: LockMode, timeout_ms: u64) -> Result<(), Error>` que adquiere
  lock y luego llama a atomic_save; no cambiar la signatura de `atomic_save` original
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib file_lock::tests::atomic_save_locked_*` — de rojo a verde

- [x] 4.3 (TRIANGULATE) Tests de integración: status.json usa `atomic_save_locked`
  con el modo y timeout elegidos; idempotencia de lock; verificar
  que lock se libera incluso si atomic_save falla
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib file_lock::tests::status_json_locked_*` — verde

- [x] 4.4 (REFACTOR) Verificar que todos los sitios que usan `atomic_save`
  no necesitan lock (grep en codebase); solo status.json entre waybar y TUI
  requiere lock multiproceso
  - skills: `ein-discipline`, `architecture`
  - verify: revisión de grep; `cargo test` completo pasa sin cambios en
    comportamiento de otros callers

## // 005. Tests de permisos, symlinks y locks (suite completo)

- [x] 5.1 (RED) Tests de integración para config, status, notify-state e índices:
  permisos privados, escritura atómica y regresión de formatos
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::private_persistence_*` — rojos

- [x] 5.2 (GREEN) Integrar la persistencia segura en los archivos propiedad de
  lazysubs-eye sin alterar formatos ni archivos de configuración del sistema
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::private_persistence_*` — verde

- [x] 5.3 (TRIANGULATE) Tests de edge: filesystem sin permisos privados,
  proceso que muere mientras mantiene el lock, rename que falla
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib cache::tests::permissions_edge_*` — verde;
    `cargo test --lib file_lock::tests::edge_*` — verde

- [x] 5.4 (REFACTOR) Limpiar imports muertos; verificar comentarios explican
  POR QUÉ (no el QUÉ); calidad gates
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## // 006. Documentación

- [x] 6.1 (DOCS) Documentar la política de symlinks y permisos en comentarios de
  `atomic_save` y `FileLock`; explicar la política elegida y sus límites
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión visual de comentarios de `atomic_save` y `FileLock`

- [x] 6.2 (DOCS) Registrar en verify-report.md tras `cargo build --release`:
  tamaño del binario, nuevas dependencias (ninguna), LOC de producción
  - skills: `ein-discipline`
   - verify: `ls -lh target/release/lazysubs-eye` (binario existe tras build);
     verificar-report contiene evidencia

## // 007. Suite completo y preparación

- [x] 7.1 (VERIFY) Ejecutar suite completo: `cargo test --locked`
  - skills: `ein-discipline`
  - verify: todos los tests pasan

- [x] 7.2 (VERIFY) Quality gates finales: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
  - skills: `ein-discipline`
  - verify: sin errores

- [x] 7.3 (VERIFY) Preparar apply-progress.md con: tareas completadas, archivos tocados,
  decisiones técnicas, riesgos, siguiente paso
  - skills: `ein-discipline`
  - verify: apply-progress.md existe y está completo tras ejecución

- [x] 7.4 (VERIFY) Preparar verify-report.md con: comandos ejecutados, output relevante,
  evidencia de que cada gate pasó
  - skills: `ein-discipline`
  - verify: verify-report.md existe y contiene evidencia tras verificación
