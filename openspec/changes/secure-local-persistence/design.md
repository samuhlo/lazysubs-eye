# A. Proposal — escritura atómica segura y permisos de archivos

## Intent

Asegurar que toda escritura de lazysubs-eye (config, caché, historial, estado de
notificaciones) sea atómica, use permisos restrictivos (0600 para archivos,
0700 para directorios), respete la existencia de symlinks con una política
explícita, y funcione correctamente en entornos multiproceso. La capa debe
ser reusable por todos los módulos sin duplicación.

## Spec

### R1. Permisos de archivos y directorios

Todo archivo escrito por lazysubs-eye en `~/.config/lazysubs-eye/`,
`~/.cache/lazysubs-eye/` y `~/.local/state/lazysubs-eye/` **MUST** tener permisos
**0600** (solo lectura/escritura para el propietario). Todo directorio recién
creado **MUST** tener permisos **0700**. La mascara umask del proceso **MUST
NOT** ser la fuente de estos permisos: se fijan explícitamente con `chmod`
tras `std::fs::create_dir` o `std::fs::write`.

**No objetivos**: no se modifican permisos de archivos de providers
(crednetials de Claude/Codex), que son ownership del usuario y se leen solo.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Directorio nuevo | No existe `~/.cache/lazysubs-eye` | Se llama `cache::atomic_save` | El directorio se crea con 0700 antes de escribir |
| Archivo nuevo | `~/.config/lazysubs-eye/config.toml` no existe | Se persiste config | El archivo se escribe con 0600 tras atomic rename |
| Archivo existente | `~/.config/lazysubs-eye/config.toml` ya existe con 0600 | Se reescribe | Se mantienen 0600; no se degradan a 0644 |
| Permisos preexistentes distintos | Un archivo tiene 0640 por otro proceso | Se reescribe | Se eleva a 0600; se loguea el cambio si `--verbose` |
| Directorio sin permisos | `~/.local/state/lazysubs-eye/` existe con 0755 | Se escribe history.db | Se eleva a 0700 tras verificar que pertenece al usuario |

### R2. Escritura atómica segura

`atomic_save(path, data)` **MUST** usar el patrón write-to-temp + sync + rename
atómico sobre el mismo filesystem. El archivo temporal **MUST** crearse en el
mismo directorio que el destino final para garantizar el mismo filesystem.
Tras el rename, **MUST** sincronizar el directorio contenedor (`fsync` del
fd del directorio en Unix) antes de retornar éxito.

Si el rename falla, el archivo de destino original **MUST** permanecer
intacto. Si la escritura del temporal falla, **MUST** eliminar el temporal
y retornar error sin modificar el destino.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Éxito normal | El directorio y destino existen | Se llama `atomic_save` | El temp se escribe, sincroniza y renombra; se retorna Ok |
| Fallo de escritura | Disco lleno o permisos | Se escribe el temp | El temp se elimina; se retorna Err; destino intacto |
| Fallo de rename | El destino es un directorio | Se intenta rename | Se retorna Err; temp eliminado; destino intacto |
| Fallo de fsync dir | El directorio no puede sincronizarse | Se sincroniza | Se retorna un error de durabilidad; el destino contiene datos completos, pero el commit no se declara durable |

### R3. Política de symlinks explícita

Antes de implementar este requisito se realizará un spike que elegirá entre
rechazar destinos symlink, seguir de forma segura su destino u otra política
documentada. La política elegida **MUST NOT** reemplazar silenciosamente el
symlink ni permitir que una escritura escape del ámbito esperado. También
**MUST** definir el comportamiento de padres symlink, cadenas y ciclos.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Destino symlink | `config.toml` es un symlink | Se llama `atomic_save` | Se aplica la política registrada sin reemplazar silenciosamente el symlink |
| Escape de ruta | El destino resuelto queda fuera del ámbito permitido | Se intenta escribir | Se rechaza sin modificar destino ni symlink |
| Cadena o ciclo | El destino contiene una cadena o ciclo | Se intenta escribir | Se resuelve o rechaza de acuerdo con límites explícitos y sin bucles |

### R4. Estado multiproceso y locks

Para archivos compartidos entre procesos, como `status.json` entre waybar y la
TUI, **MUST** implementarse exclusión mutua entre procesos. Un spike elegirá
entre lock advisory, lockfile u otra alternativa Unix y registrará portabilidad,
liberación tras crash y estrategia de timeout.

Antes de escribir, el proceso **MUST** adquirir lock exclusivo. Tras el
rename atómico y fsync del directorio, **MUST** liberar el lock. Si el lock
no está disponible y el modo es no bloqueante, **MUST** retornar
`Err(LockNotAvailable)` sin bloquear.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Lock disponible | Ningún proceso posee el lock | Se intenta adquirir | Se adquiere y se retorna `Ok(Guard)` |
| Lock ocupado (no bloqueante) | Otro proceso posee el lock | Se intenta adquirir | Se retorna `Err(LockNotAvailable)` |
| Lock ocupado (bloqueante) | Otro proceso posee el lock | Se intenta adquirir | Se espera hasta el timeout elegido y retorna `Err(LockTimeout)` si se agota |
| Fallo de lock durante escritura | El lock se pierde (proceso killado) | Se está escribiendo | Se aborta la escritura; se retorna `Err(LockLost)`; archivo destino intacto |
| Lock multiproceso real | Dos procesos de lazysubs-eye activos | Ambos intentan escribir status.json | Solo uno escribe; el otro espera o recibe el error definido por la política |

### R5. Permisos cross-platform y degradación

En Unix, **MUST** usar `std::fs::set_permissions` para fijar los permisos tras
cada escritura. Fuera de Unix, el comportamiento debe quedar documentado como
no soportado o degradado hasta que exista una implementación equivalente
verificada.

Si no se pueden garantizar permisos privados en Unix, la escritura de config,
caché privada o historial **MUST** fallar antes de publicar el nuevo archivo.
No se permite continuar silenciosamente con secretos potencialmente legibles.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Unix normal | Se escribió ~/.cache/lazysubs-eye/index.json | Se llama chmod 0600 | Se fija; se retorna Ok |
| Plataforma no Unix | Se intenta persistir | No existe implementación equivalente | Se usa el fallback documentado o se retorna un error accionable |
| Filesystem sin permisos | El filesystem no admite owner permission bits | Se prepara la escritura | Se retorna error accionable antes del rename |
| Permisos no reducibles | El destino no puede hacerse privado | Se prepara la escritura | Se aborta sin degradar el archivo existente |

## Decisions

1. **Política de symlinks — rechazar**: se rechaza el destino y cualquier
   padre que sea un symlink. No se resuelven cadenas ni se siguen escapes: una
   cadena o ciclo contiene necesariamente un symlink y devuelve
   `SymlinkPolicyViolation` sin modificar nada. Es la política más simple de
   auditar para rutas locales privadas.
2. **Lock por archivo, no global**: el lock se adquiere por archivo, no
   hay un lock global. Esto permite escrituras concurrentes a archivos
   distintos.
3. **Timeout elegido tras el spike**: un proceso no debe esperar
   indefinidamente, pero el valor se justificará con pruebas de contención.
4. **Fallo cerrado en permisos privados**: la atomicidad no compensa una fuga
   de credenciales; si no se puede garantizar 0600/0700, no se publica el archivo.

## Success Criteria

- Todo archivo escrito por cualquier módulo de lazysubs-eye tiene permisos 0600.
- Todo directorio creado tiene permisos 0700.
- La política de symlinks elegida evita reemplazos silenciosos y escapes de ruta.
- Dos procesos concurrentes no pueden escribir el mismo archivo simultáneamente.
- El archivo destino nunca queda truncado ni corrupto ante fallos.
- Los tests usan archivos temporales con permisos verificados.

---

# B. Decisions de diseño adicionales

## Decisiones y trade-offs

1. **Exclusión entre procesos — `flock(2)` advisory**: waybar y TUI son
   procesos distintos, por lo que se usa un lock de fichero exclusivo por
   destino (`<destino>.lock`) en Unix. El kernel lo libera al terminar el
   proceso, incluso tras `kill -9`; se usa modo no bloqueante o sondeo cada
   10 ms hasta el timeout. No es portable a Windows y no protege frente a
   programas que ignoren los advisory locks.
2. **No lock en archivos de caché de providers**: status.json es el único
   archivo compartido; los índices de Pi y OpenCode son privados de lazysubs-eye.
3. **Permisos explícitos tras rename**: no basta con crear el temp con 0600;
   tras el rename el archivo puede perder esos permisos en algunos filesystems.
   Se llama chmod tras rename.
4. **Fsync del directorio**: es necesario para garantizar durabilidad en
   sistemas con write-back caches. Sin él, un crash podría perder el rename.
