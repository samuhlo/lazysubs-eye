# Alcance: escritura atómica segura y permisos de archivos

## SCOPE PACKET

```yaml
scope: Asegurar escritura atómica, permisos restrictivos (0600/0700), política
  de symlinks explícita, y locks multiproceso para todos los archivos de estado
  de lazysubs-eye. La solución debe ser reusable por todos los módulos existentes
  sin duplicación de código.
change_name: secure-local-persistence
budget_allocated:
  max_tokens: 18000
  max_reads: 25
  max_runtime_ms: 900000
webfetch: false
strict_tdd: true
artifact_language: es
```

## Resultado esperado

Toda escritura de config, caché, historial y estado de notificaciones es
atómica (temp + rename + fsync), usa permisos 0600/0700 explícitos, aplica una
política segura y documentada de symlinks, y coordina accesos concurrentes.
El cambio es transparente para los módulos que ya usan `atomic_save` y
`cache.rs`.

## Hechos de partida

Estos hechos están verificados y no necesitan redescubrirse:

- lazysubs-eye es un binario Rust; usa `cargo test` y `strict_tdd: true`.
- `cache.rs` ya tiene `atomic_save` que usa write-to-temp + rename; el patrón
  existe y funciona para el caso simple.
- Los directorios de estado son `~/.config/lazysubs-eye/`, `~/.cache/lazysubs-eye/` y
  `~/.local/state/lazysubs-eye/`.
- `install.rs` escribe en waybar config, hyprland.conf y symlinks.
- `history.rs` escribe `history.db` (SQLite).
- `notify.rs` escribe `notify-state.json`.
- No hay actualmente locks multiproceso; los módulos escriben sin coordinación.
- No hay actualmente fijacion de permisos tras escribir.
- Los symlinks se siguen sin validación.

## Alcance funcional

### atomic_save mejorado

- Mantener la interfaz pública de `atomic_save(path, data)` existente.
- Añadir la política de symlinks elegida durante el spike.
- Añadir fijacion de permisos 0600 tras el rename exitoso.
- Añadir fsync del directorio tras rename.
- Devolver errores tipados: `SymlinkPolicyViolation`, `LockNotAvailable`,
  `LockTimeout`, `LockLost`, `PermissionChangeFailed`.

### Permisos

- Crear helper `set_permissions_restrictive(path: &Path, is_dir: bool)` que
  llame a chmod/set_permissions con 0600 o 0700.
- Llamar a este helper tras cada `atomic_save` exitoso.
- Verificar que los permisos preexistentes no son menos restrictivos; si lo
  son, elevarlos.
- No modificar archivos que no son de lazysubs-eye (credentials de providers).

### Locks multiproceso

- Crear una abstracción de lock con semántica de adquisición y liberación.
- Soportar modo bloqueante y no bloqueante.
- Elegir el mecanismo Unix mediante spike y documentar sus límites.
- Aplicar a `status.json` primero; otros archivos como decision futura.

### Política de symlinks

- Evaluar rechazo, seguimiento seguro u otra política.
- Exigir que no se reemplace silenciosamente un symlink ni se escape del
  ámbito esperado.
- Documentar padres symlink, cadenas, ciclos y errores antes de implementar.

### Cross-platform

- En Unix: `libc::chmod` o `std::fs::set_permissions`.
- En plataformas no Unix: documentar el estado como no soportado hasta que
  exista una implementación equivalente verificada.
- Fallo de permisos en datos privados → error antes del rename.

## Criterios de aceptación

1. Todo archivo nuevo escrito por lazysubs-eye tiene permisos 0600 tras la
   llamada a `atomic_save`.
2. Todo directorio nuevo tiene permisos 0700.
3. La política elegida evita reemplazar silenciosamente symlinks y escribir
   fuera del ámbito permitido.
4. Dos procesos simultáneos que intentan escribir el mismo archivo con lock
   activo producen exactamente uno exitoso y otro con `LockNotAvailable`.
5. Un fallo de rename tras escritura de temp deja el archivo original intacto.
6. Los tests usan archivos temporales con permisos verificados.
7. El cambio es transparente para los módulos existentes; no requieren cambios.

## Fuera de alcance

- Modificar el handling de credenciales de providers (son ownership del usuario).
- Cambiar el formato de ningún archivo de datos (config.toml, history.db).
- Añadir encryption de archivos.
- Lock de lectura (shared locks); solo exclusivo por ahora.
- Soporte de BSD flock en macOS como fallback si fcntl no está disponible.
- Soporte de Windows para permisos y locks.

## Decisiones del spike

- **Symlinks**: se rechazan el destino y cualquiera de sus padres. Esto cubre
  destino, cadena, ciclo y escape sin seguir enlaces ni reemplazarlos.
- **Lock Unix**: `flock(2)` advisory por fichero; el lock se libera al cerrar
  el descriptor o cuando muere el proceso. `status.json` usa espera acotada de
  100 ms; los demás ficheros no requieren lock multiproceso por ahora.

## Investigación acotada para sdd-map

1. ¿Qué tan grande es la superficie de `atomic_save`? ¿Cuántos sitios la
   llaman?
2. ¿Qué archivos se escriben concurrentemente entre procesos distintos?
3. ¿Hay tests existentes de `atomic_save` que debamos ampliar?
4. ¿El sandbox de tests permite chmod sin privilegios de root?
5. ¿Hay necesidad real de locks más allá de status.json?

## Riesgos y controles

| Riesgo | Control requerido |
|--------|-------------------|
| Degradación de permisos por umask | chmod explícito tras cada escritura; no confiar en umask |
| Ataque o confusión con symlinks | Spike previo; política explícita; tests de escape, cadena y ciclo |
| Condición de carrera sin lock | implementar FileLock antes de cualquier escritura multiproceso |
| Filesystem sin permisos privados | Fallar antes de publicar el nuevo archivo y conservar el destino anterior |
| Retraso de atomic_save por fsync | budgetear el overhead (~1-5ms); si excede 50ms, async el fsync |

## Condiciones para pasar a diseño

El mapa debe identificar con evidencia concreta: los sitios exactos que llaman
a `atomic_save`, los archivos que se escriben concurrentemente, y los tests
existentes que deben ampliarse. Si no hay evidencia de concurrencia real entre
procesos para ningún archivo excepto status.json, el diseño debe proponer un
lock granular mínimo.
