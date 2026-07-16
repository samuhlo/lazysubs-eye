# Mapa: escritura atómica segura y permisos de archivos

status: partial
scope_status: bounded
change: secure-local-persistence
phase: map
skill_resolution: pending
budget_consumed: {tokens: 0, reads: 0}

## Ledger (pendiente de actualización tras exploración)

## Decisión principal

Ampliar `cache::atomic_save` con cuatro garantías adicionales:
(1) permisos restrictivos 0600/0700 tras cada escritura,
(2) política de symlinks elegida mediante spike,
(3) fsync del directorio tras rename,
(4) exclusión entre procesos con el mecanismo elegido mediante spike.

El símbolo central es `atomic_save` en `src/cache.rs`. El lock vive en
`src/file_lock.rs` nuevo. Los módulos existentes (`output.rs`, `install.rs`,
`history.rs`, `notify.rs`) no requieren cambios funcionales; solo verificaciones
de regresión.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado |
|---|---|---|
| `src/cache.rs` `atomic_save` | Write-to-temp + rename; sin chmod; sin validación de symlinks; sin fsync | Ampliar con los cuatro requisitos de R1-R4 |
| `src/cache.rs` `atomic_save` | Interfaz pública: `fn atomic_save(path: &Path, data: &[u8]) -> Result<(), Error>` | Mantener firma; añadir `atomic_save_locked` con lock opcional |
| `src/output.rs` `save_status` | Llama a `cache::atomic_save` para status.json | Cambiar a `atomic_save_locked` con LockMode::Blocking |
| `src/install.rs` | Escribe configs de waybar, hyprland, symlinks | Sin cambios; atomic_save ya usado; los permisos de estos archivos están fuera del scope (ownership del usuario) |
| `src/history.rs` | Escribe `history.db` con SQLite | Sin cambios; atomic_save no se usa aquí; SQLite tiene sus propios pragmas |
| `src/notify.rs` | Escribe `notify-state.json` con atomic_save | Sin cambios; el archivo es privado de lazysubs-eye; aplicar permisos |

## Flujo afectado

```
atomic_save(path, data)
  1. Inspeccionar destino y padres sin reemplazar symlinks
  2. Aplicar la política registrada por el spike
     → Violación: return Err(SymlinkPolicyViolation)
  3. Crear temp en el mismo directorio (O_EXCL para seguridad)
  4. Escribir datos con permisos 0600 desde el inicio
  5. ¿sync del temp exitoso?
     → No: eliminar temp, return Err
  6. Rename temp → destino (atómico en el mismo filesystem)
  7. ¿rename exitoso?
     → No: eliminar temp, return Err
  8. fsync del directorio (fd del padre)
  9. chmod destino a 0600 (o 0700 si es directorio)
  10. return Ok

atomic_save_locked(path, data, mode, timeout)
  1. FileLock::acquire(path.with_extension("lock"), mode, timeout)
  2. ¿Error? → return Error (LockNotAvailable o LockTimeout)
  3. atomic_save(path, data)
  4. Liberar lock (FileLockGuard::drop)
  5. return resultado de atomic_save
```

## Archivos concretos

| Archivo | Rol | Símbolos/seam | Cambio |
|---|---|---|---|
| `src/cache.rs` | Escritura atómica reusable | `atomic_save`, `AtomicSaveError`, helper `set_permissions_restrictive` | Ampliar atomic_save con R1-R4; añadir tipos de error |
| `src/file_lock.rs` (nuevo, si la decisión lo requiere) | Exclusión multiproceso | `FileLock`, `FileLockGuard`, `LockMode`, `FileLock::acquire` | Implementar el mecanismo elegido |
| `src/output.rs` | Escritura de status.json | `save_status` | Cambiar a `atomic_save_locked` |
| `src/install.rs` | Escritura de configs de sistema | `install`, `uninstall` | Sin cambios funcionales; verificar regresión |
| `src/notify.rs` | Escritura de notify-state.json | `persist_notify_state` | Sin cambios; verificar que atomic_save se usa |
| `src/history.rs` | SQLite conjournal_mode=WAL | `ensure_table`, insertas | Sin cambios; SQLite tiene sus propios mecanismos |

## Puntos de riesgo

1. **Regresión en atomic_save**: el cambio es aditivo pero altera el flujo
   interno. Un error podría romper todas las escrituras de lazysubs-eye.
   **Mitigación**: tests de regresión existentes siguen pasando.

2. **Overhead de fsync**: fsync(~1-5ms) en cada escritura añade latencia.
   No afecta al caso de uso (waybar refresh cada 60s; TUI cada 30s).
   **Mitigación**: budgetear; si supera 50ms, hacer fsync asíncrono.

3. **Portabilidad del lock**: el mecanismo elegido puede ser Unix-only.
   **Mitigación**: documentar soporte y fallback durante el spike.

4. **Permisos en temp**: el temp se crea con 0600 desde el inicio, pero
   en algunos filesystems el rename puede no preservar los permisos.
   **Mitigación**: crear y verificar el temporal con 0600 antes del rename;
   comprobar el destino como defensa adicional.

5. **Deadlock**: si un proceso adquiere lock y falla sin liberar (panic,
   kill -9), el lock queda colgado hasta reboot.
   **Mitigación**: timeout y liberación tras crash definidos por la decisión.

## Estrategia de pruebas

### Unit tests (dentro de los módulos)

- Tests de `set_permissions_restrictive`: archivo 0644 → 0600, dir 0755 → 0700.
- Tests de la política de symlinks: destino, padres, cadena, ciclo y escape.
- Tests de `FileLock::acquire`: exitoso, LockNotAvailable, LockTimeout, Drop.
- Tests de `atomic_save` con permisos: archivo preexistente con otros permisos.
- Tests de `atomic_save` con symlinks rechazados.
- Tests de atomicidad: fallo tras temp escrito, destino original intacto.

### Integration tests (test de dos procesos)

- Dos procesos: uno adquiere lock blocking, el otro intenta non-blocking →
  primero Ok, segundo LockNotAvailable.
- Un proceso escribe con atomic_save_locked, el otro lee → ambos ven
  archivo completo (no partial write).
- Un proceso con lock recibe terminación → el lock se libera según el mecanismo elegido.

### Regresión tests

- `cargo test` existente pasa sin modificación.
- Comportamiento de output.rs con waybar byte-equivalente (el lock no
  cambia el formato de status.json).

## Rollback

El rollback consiste en revertir `cache.rs` a la versión anterior y eliminar
`file_lock.rs`. Los archivos de estado existentes mantienen sus permisos
actuales; el rollback no intenta "degradarlos". Los módulos existentes
(`output.rs`, `install.rs`, `history.rs`, `notify.rs`) no requieren cambios.

Para rollback sin recompilar: git revert del commit de este paquete.
El estado del sistema vuelve al anterior en el siguiente build.

## Dependencias con otros paquetes

| Paquete | Relación |
|---------|----------|
| `safe-system-integration` | Usa `atomic_save` para configs de sistema; si los permisos se elevan antes de install.rs, el uninstall puede fallar al intentar escribir. Coordinar orden. |
| `observable-health-diagnostics` | El lock de status.json afecta al diagnóstico de concurrencia; observable puede necesitar mostrar "lock contention" como métrica. |
| `runtime-performance` | fsync añade overhead; si performance budgets no se cumplen, considerar async fsync. |

## Siguiente fase

Pasar a `sdd-design` con este mapa. Antes de apply, el diseño debe convertir
cada requisito R1-R5 en tasks numeradas con ciclos RED→GREEN→triangulación.
Esta fase no ejecutó build ni tests.
