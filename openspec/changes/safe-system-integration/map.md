# Mapa: integración segura con el sistema

status: partial
scope_status: bounded
change: safe-system-integration
phase: map
skill_resolution: pending
budget_consumed: {tokens: 0, reads: 0}

## Ledger (pendiente de exploración)

## Decisión principal

Separar `install()` y `uninstall()` en cuatro fases: preflight → plan →
validación de marcadores → ejecución, con rollback automático y modo
sandbox para pruebas. La resolución del binario se hace durante `install`.

## Arquitectura actual y seams

| Pieza actual | Hecho | Cambio acotado |
|---|---|---|
| `src/install.rs` `install()` | Ejecuta todas las operaciones secuencialmente; sin preflight; sin rollback | Separar en preflight → plan → execute; añadir rollback |
| `src/install.rs` `uninstall()` | Elimina lo que encuentra sin verificar marcadores ni warn de reglas manuales | Añadir validación de marcadores y detección de reglas manuales |
| `src/install.rs` | Comandos de integración pueden usar un nombre o path incorrecto | Usar `current_exe()` durante install y persistir la ruta absoluta |
| Waybar/launcher | Comandos de polling y click | Generarlos con la ruta absoluta resuelta |
| Backups | Ya existen con `.bak.<epoch>` | Usar BackupManager para tracking y rollback |

## Flujo afectado

```
// Antes
install()
  → modify waybar config
  → modify hyprland.conf
  → reload waybar  ← SI ESTO FALLA, WAYBAR Y HYPRLAND QUEDAN INCONSISTENTES

// Después
install()
  → preflight_install()        ← falla aquí si algo no está
  → build_install_plan()
  → execute_plan()
      → backup waybar config
      → backup hyprland.conf
      → modify waybar config    ← si falla aquí...
      → modify hyprland.conf
      → reload waybar
      → rollback waybar backup  ← ...se restaura waybar backup
      → rollback hyprland backup
```

## Archivos concretos

| Archivo | Rol | Símbolos/seam | Cambio |
|---|---|---|---|
| `src/install.rs` | Install/uninstall | `install()`, `uninstall()`, `InstallError`, `InstallContext` | Separar fases; añadir preflight, plan, rollback |
| `src/install.rs` | Backup manager | `BackupManager` (nuevo) | Crear para tracking y rollback |
| `src/install.rs` | Resolución de binario | `resolve_binary_path()` (nuevo) | Usar current_exe() |
| `src/install.rs` | Validación de marcadores | `validate_markers()`, `check_manual_rules()` (nuevo) | Verificar ownership antes de uninstall |
| `src/install.rs` | Tipos de plan | `InstallPlan`, `FileChange`, `BackupSpec`, `CommandSpec` | Estructurar el plan para dry-run |

## Puntos de riesgo

1. **Rollback que no restaura todo**: si el rollback no funciona correctamente,
   el sistema queda en estado inconsistente. **Mitigación**: tests de rollback
   con inyección de errores.
2. **Markers editados pero no detectados**: si el usuario editó una línea entre
   marcadores sin cambiar el marker, la validación no lo detecta. **Mitigación**:
   guardar el hash del contenido original en el marker comment.
3. **current_exe() falla**: si el binario fue movido o borrado, current_exe()
   puede fallar. **Mitigación**: preflight check de binario.
4. **Sandbox que no refleja el sistema real**: diferencias entre el sandbox y
   el sistema real pueden hacer que install en real falle cuando sandbox pasó.
   **Mitigación**: el sandbox es solo para probar los pasos de lógica, no el
   ambiente real.

## Estrategia de pruebas

### Unit tests

- Tests de preflight: binary not found, dir not writable, waybar missing.
- Tests de marker validation: intactos, editados, missing.
- Tests de backup manager: creación, rollback, segunda vez con epoch+1.
- Tests de resolve_binary_path.

### Integration tests

- install completo en temp dir; uninstall idempotente.
- install con fallo en paso 3; verificar rollback de pasos 1-2.
- uninstall con reglas manuales entre marcadores; warn.
- dry-run sin escrituras y ejecución real dentro de un sandbox XDG/HOME.

### Regresión tests

- `cargo test` existente pasa.
- Los tests de install usan temp dirs y mocks.

## Rollback

Revertir los cambios de install.rs al commit anterior. Los archivos del sistema
que install.rs tocó tienen backups `.bak.<epoch>` que se pueden restaurar
manualmente si el rollback falló.

## Dependencias con otros paquetes

| Paquete | Relación |
|---------|----------|
| `secure-local-persistence` | Punto de integración compatible, pero no bloqueante: este paquete puede usar la persistencia existente y adoptar después la versión endurecida |

**Nota**: la implementación de install/uninstall se beneficia de `atomic_save` con permisos y symlinks seguros, pero puede avanzar en paralelo.

## Siguiente fase

Pasar a `sdd-design` con este mapa. Esta fase no ejecutó build ni tests.
