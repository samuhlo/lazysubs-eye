# A. Proposal — integración segura con el sistema

## Intent

Hacer que `lazysubs-eye install` y `lazysubs-eye uninstall` sean operaciones
transaccionales, verificables antes de ejecutarse (dry-run), con preflight
checks, que detecten ownership real de los archivos que modifican, que no
toquen reglas manuales añadidas por el usuario fuera de los marcadores, y que
tengan un modo de escape seguro cuando algo salga mal.

## Spec

### R1. Preflight checks antes de cualquier modificación

Antes de ejecutar cualquier paso de `install` o `uninstall`, el sistema
**MUST** ejecutar una fase de preflight que verifique:

1. El binario existe y es ejecutable.
2. El directorio de configuración existe o puede crearse.
3. Waybar config existe o la ruta proporcionada es válida.
4. El directorio de datos de lazysubs-eye (`~/.local/state/lazysubs-eye`) es
   escribible.
5. La ruta devuelta por `std::env::current_exe()` existe y es ejecutable.

Si cualquier preflight check falla, **MUST** retornar un error actionable
antes de modificar nada. El usuario **MUST** ver exactamente qué falló y
por qué.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Binario no encontrado | `current_exe()` no resuelve un ejecutable válido | Se ejecuta `install` | Error accionable con la ruta comprobada y las opciones de instalación |
| Config de waybar ausente | No existe `~/.config/waybar/config` ni `~/.config/waybar/config.jsonc` | Se ejecuta `install` | Error: "No se encontró config de waybar. Especifica --waybar-config <ruta>" |
| Directorio no escribible | `~/.local/state/lazysubs-eye` existe pero es 000 | Se ejecuta `install` | Error: "No se puede escribir en ~/.local/state/lazysubs-eye. Verifica permisos." |

### R2. Plan y dry-run

`lazysubs-eye install --dry-run` **MUST** mostrar el plan completo de cambios
sin ejecutar ninguna modificación. El plan **MUST** incluir:

1. Archivos que se modificarán (con path absoluto).
2. Backups que se crearán (con extensión `.bak.<epoch>`).
3. Comandos que se ejecutarán (waybar reload, hyprctl, etc.).
4. Archivos que se eliminarán en uninstall (con path).
5. Reglas que se añadirán/eliminará en waybar y hyprland.conf.

El formato **MUST** ser legible y accionable: un humano debe poder leer
el plan y entender exactamente qué cambiará.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Dry-run exitoso | Todos los preflight checks pasan | Se ejecuta `install --dry-run` | Se imprime el plan de cambios; ningún archivo se modifica |
| Dry-run con fallo | Preflight check falla | Se ejecuta `install --dry-run` | Se imprime el error del preflight; ningún archivo se modifica |

### R3. Ruta real del binario para clicks

La TUI se abre al hacer click en el módulo de waybar. Durante `install`, el
programa **MUST** resolver la ruta real del binario ejecutable usando
`std::env::current_exe()` o una lectura de `/proc/self/exe`, no una ruta
hardcodeada. Si el binario se
instaló en `/usr/bin/lazysubs-eye` pero el windowrule apunta a `~/.local/bin/lazysubs-eye`,
el click no funcionará.

La ruta resuelta **MUST** quedar incrustada en los comandos de polling y click
generados por `install`. Si el binario se mueve después, `doctor` debe detectar
la integración rota y una nueva ejecución de `install` debe repararla.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Binario en ~/.local/bin | Se instaló con install | Se hace click en waybar | Hyprland lanza `~/.local/bin/lazysubs-eye` |
| Binario en /usr/bin | `install` se ejecuta desde `/usr/bin/lazysubs-eye` | Se genera la integración | Los comandos usan `/usr/bin/lazysubs-eye` |
| Binario movido después | La ruta incrustada ya no existe | Se ejecuta `doctor` | Se informa cómo reparar la integración |

### R4. Ownership y marcadores

`install` **MUST** añadir marcadores `lazysubs-eye-begin` y `lazysubs-eye-end`
alrededor de las reglas que añade, en waybar config y hyprland.conf. Estos
marcadores **MUST** permitir que `uninstall` identifique exactamente qué líneas
fueron añadidas por lazysubs-eye vs. cuáles fueron añadidas manualmente por el
usuario.

El sistema **MUST** verificar que no existen reglas de lazysubs-eye fuera de los
marcadores antes de hacer uninstall. Si existen, **MUST** warn pero no borrar
sin confirmación.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Install primero | No existen marcadores | Se ejecuta `install` | Se añaden marcadores y reglas entre ellos |
| Install repetido | Ya existen marcadores con las mismas reglas | Se ejecuta `install` | idempotente; no duplica reglas |
| Install con reglas manuales entre marcadores | El usuario añadió una regla manual entre los marcadores | Se ejecuta `install` | Se detecta conflicto; se retorna error con las líneas en conflicto |
| Uninstall limpio | Existen marcadores con reglas | Se ejecuta `uninstall` | Se eliminan solo las líneas entre marcadores |
| Uninstall con reglas manuales | El usuario añadió reglas entre marcadores antes de install | Se ejecuta `uninstall` | Se warn sobre las líneas manuales; se preguntan antes de borrar |

### R5. Uninstall no toca reglas manuales

`uninstall` **MUST** operar únicamente dentro de los marcadores
`lazysubs-eye-begin/end`. Las reglas añadidas por el usuario fuera de los
marcadores **MUST NOT** ser eliminadas, modificadas ni movidas. Si el usuario
 tiene reglas manuales que se confunden con las de lazysubs-eye, **MUST** warn
explícitamente con los números de línea.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Uninstall con reglas manuales fuera de marcadores | Hyprland.conf tiene reglas lazysubs-eye entre marcadores + reglas manuales en otro lugar | Se ejecuta `uninstall` | Solo se eliminan las líneas entre marcadores |
| Uninstall con modificación manual de marcador | El usuario editó una línea dentro de los marcadores | Se ejecuta `uninstall` | Se elimina la regla editada; se warn sobre la edición |
| Uninstall sin marcadores | Nunca se ejecutó install, o los marcadores se borraron | Se ejecuta `uninstall` | Error: "No se encontraron marcadores de lazysubs-eye. Deshaz los cambios manualmente." |

### R6. Operación transaccional y rollback

`install` **MUST** ejecutarse como una transacción: si cualquier paso falla
después de modificar un archivo, **MUST** hacer rollback de todos los archivos
ya modificados a su estado anterior. El rollback **MUST** usar los backups
creados con el sufijo `.bak.<epoch>`.

El rollback **MUST** ejecutarse automáticamente ante cualquier error, sin
acción del usuario. Si el rollback mismo falla, **MUST** retornar un error
que liste exactamente qué archivos quedaron en estado modificado y cómo
deshacerlos manualmente.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Éxito normal | Todos los preflight checks pasan y todas las operaciones succeed | Se ejecuta `install` | Todos los archivos se modifican; se crean backups |
| Fallo en paso 3 de 5 | El paso 3 falla (waybar reload) | Se ejecuta install | Pasos 1-2 se rollback usando backups; paso 3 retorna error; los archivos vuelven a su estado anterior |
| Fallo de rollback | rollback stesso falla (disk full) | Se ejecuta install | Error con la lista de archivos en estado modificado; se sugiere cómo deshacer manualmente |

### R7. Backups sin colisión

Los backups **MUST** crearse con un sufijo que incluya el epoch Unix:`
`.bak.<epoch>`. Si ya existe un backup con el mismo epoch, se usa el
siguiente segundo entero. Los backups nunca se sobrescriben.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Primer backup | No existe backup previo | Se ejecuta `install` | Se crea `~/.config/waybar/config.jsonc.bak.1721000000` |
| Backup repetido en el mismo segundo | install se ejecutó dos veces en el mismo segundo | Se ejecuta install por tercera vez | Se crea `.bak.1721000001` (siguiente segundo) |

### R8. Pruebas en sandbox

Antes de cada install/uninstall, el sistema **MUST** poder ejecutarse en un
modo de sandbox que use archivos temporales y no toque el sistema real. El
sandbox **MUST** ser verificable: el plan que se ejecutaría en el sistema real
es el mismo que se ejecutó en el sandbox.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Sandbox mode | Se redirigen HOME/XDG a `/tmp/test-env` y se ejecuta `install` | Se ejecuta | Los archivos del sandbox se modifican; la configuración real no se toca |
| Sandbox dry-run | Se ejecuta `install --dry-run --sandbox /tmp/test-env` | Se ejecuta | Plan se imprime; archivos en /tmp/test-env no se modifican |

## Decisions

1. **Marcadores sobre diff**: se eligen marcadores textuales porque funcionan
   con cualquier formato de archivo (JSON, toml, shell). Un diff algorítmico
   es más complejo y puede fallar con formats no lineales.
2. **Rollback automático**: ante un fallo, rollback automático es mejor que
   rollback manual porque el usuario puede no saber cómo deshacer. El costo es
   complejidad del código.
3. **Preflight checks exhaustivos**: cada check es una inversión que evita
   rollback. Son económicas (lecturas de filesystem) comparados con el costo de
   un rollback.

## Success Criteria

- install --dry-run muestra plan preciso sin modificar nada.
- install con fallo en paso 3 hace rollback de pasos 1-2 a su estado anterior.
- uninstall no elimina reglas manuales fuera de marcadores.
- El binario se resuelve durante `install` y su ruta absoluta se incrusta en los comandos generados.
- Sandbox mode permite probar install/uninstall sin tocar el sistema real.
- Los tests usan temp dirs y no el sistema real.

---

# B. Decisions de diseño adicionales

## Decisiones y trade-offs

1. **Resolver binario durante install**: los comandos externos no pueden llamar
   a `current_exe()` en el momento del click. Si el binario se mueve, `doctor`
   detecta la ruta obsoleta y `install` la regenera.
2. **Preflight antes de cualquier modificación**: el costo de un preflight
   fallido es bajo comparado con un install a medias. Cada paso debe ser
   verificable antes de ejecutarse.
3. **Markers con validación de contenido**: no basta con buscar los markers;
   hay que verificar que el contenido entre ellos es el que lazysubs-eye puso.
   Si el usuario editó una línea, hay que warn.
