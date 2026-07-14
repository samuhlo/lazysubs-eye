# Alcance: uso diario de tokens de OpenCode

## SCOPE PACKET

```yaml
scope: Integrar el uso local diario de tokens y coste de OpenCode en lazysubs como una sección TUI independiente, leyendo su SQLite/WAL de forma eficiente y segura. La ampliación seguirá la forma de «Pi hoy» sin alterar Claude, Codex, Pi, JSON ni Waybar.
change_name: opencode-daily-token-usage
budget_allocated:
  max_tokens: 25000
  max_reads: 45
  max_runtime_ms: 1200000
webfetch: true
strict_tdd: true
artifact_language: es
```

## Resultado esperado

La TUI incorpora una tabla independiente **«OpenCode hoy»** que resume el consumo del día local, agrupado por proveedor y modelo. Cada fila presenta, cuando OpenCode los persista, tokens de entrada, salida, lectura de caché, escritura de caché, total y coste registrado.

La recogida debe ser local, privada, no bloqueante y compatible con una base SQLite activa en modo WAL. Una instalación sin OpenCode, una base ausente o una lectura temporalmente no disponible no debe retrasar ni romper el resto de lazysubs.

## Hechos de partida

Estos hechos están verificados y no necesitan redescubrirse en fases posteriores salvo que el código o la evidencia oficial los contradigan:

- lazysubs es un binario Rust; usa `cargo test` y `strict_tdd: true`.
- La TUI usa ratatui y refresca periódicamente sin bloquear las consultas existentes.
- La funcionalidad completada «Pi hoy» es la referencia de producto, no una fuente que deba fusionarse con OpenCode.
- El ejecutable local es `~/.local/bin/opencode`; `opencode --version` se cuelga y no constituye una superficie de integración fiable.
- La instalación actual mantiene `~/.local/share/opencode/opencode.db` (aprox. 61 MB) con archivos `-wal` y `-shm` activos.
- La base contiene, entre otras, las tablas `message`, `part`, `session`, `session_message` y `session_input`; existe además un `storage/` heredado.
- El OpenCode instalado usa actualmente SQLite. La migración desde JSON heredado queda fuera salvo prueba de que sea necesaria para la versión activa.
- La configuración SDD vigente define `cargo test` como runner de pruebas y no declara suites separadas de integración o E2E.

## Alcance funcional

### Descubrimiento y disponibilidad

- Resolver la base desde el directorio de datos XDG (`XDG_DATA_HOME`) y documentar el valor por defecto equivalente a `~/.local/share/opencode/opencode.db`.
- Tratar como estados normales y recuperables: OpenCode no instalado, ruta ausente, base ausente, permisos insuficientes, esquema no compatible y base temporalmente ocupada o no disponible.
- No ejecutar, controlar ni consultar el proceso o API de OpenCode.

### Lectura segura y privada

- Abrir SQLite mediante semántica normal de solo lectura compatible con WAL.
- No copiar la base ni sus archivos WAL/SHM; no mutar, migrar, crear índices, hacer checkpoint ni adquirir bloqueos exclusivos.
- Consultar únicamente metadatos de uso necesarios: marca temporal, proveedor, modelo, categorías de tokens y coste persistido.
- Prohibir la lectura de credenciales, autenticación, cuentas, prompts, contenido de mensajes o partes, argumentos/resultados de herramientas y filas crudas no necesarias.
- Persistir o emitir exclusivamente agregados de uso y, si hiciera falta, un cursor local mínimo del collector; nunca contenido de OpenCode ni filas crudas.

### Semántica del agregado

- Definir «hoy» con los límites del día del calendario local y convertirlos de forma explícita al formato temporal persistido por OpenCode.
- Incluir solo uso de respuestas de asistente que la evidencia de esquema/fuente identifique como contabilizable.
- Agrupar por la pareja proveedor/modelo con identificadores persistidos por OpenCode.
- Mapear explícitamente las categorías de OpenCode a las columnas de «Pi hoy»: entrada, salida, lectura de caché, escritura de caché, total y coste.
- Distinguir entre un valor real igual a cero y una métrica ausente. Si el esquema no persiste una categoría o coste, la UI debe mostrar su ausencia de forma honesta, sin inventar `0`.
- Evitar doble conteo cuando una misma información esté representada en más de una tabla o forma del esquema.

### Rendimiento y concurrencia

- Consultar solo filas estrechas correspondientes al día actual mediante índices existentes siempre que sea posible.
- No realizar un escaneo completo de una base de decenas de MB en cada refresco de 60 segundos.
- Si no existe un acceso indexado suficiente, diseñar un watermark/cursor y una caché incremental acotados en el espacio propio de lazysubs, sin modificar la base de OpenCode.
- Suprimir scans duplicados cuando coincidan refrescos automáticos o manuales.
- Mantener estado, trabajo en segundo plano y errores de OpenCode independientes del refresco de providers, tokens Claude y uso Pi.

### Presentación TUI

- Añadir una sección claramente titulada **«OpenCode hoy»** y separada de **«Pi hoy»**.
- Seguir la legibilidad y columnas de la tabla Pi donde las semánticas coincidan, incluyendo proveedor/modelo como identidad de agrupación.
- Representar estado vacío, no disponible y error recuperable sin bloquear la interacción ni borrar los datos válidos de las otras secciones.
- No incorporar el agregado OpenCode a JSON, Waybar ni a un total combinado ambiguo.

## Criterios de aceptación

1. Con `XDG_DATA_HOME` configurado, el collector busca la base bajo ese directorio; sin él utiliza el valor por defecto documentado del entorno del usuario.
2. Si OpenCode o su base no existen, lazysubs conserva su comportamiento actual y la TUI comunica la ausencia sin fallar.
3. La lectura de una base activa con WAL es de solo lectura y no copia, escribe, migra, indexa, hace checkpoint ni bloquea exclusivamente ningún archivo de OpenCode.
4. Una fixture SQLite temporal con registros dentro y fuera del día local produce únicamente el agregado del día local.
5. La fixture demuestra agrupación independiente por proveedor/modelo y evita doble conteo.
6. Entrada, salida, lectura de caché, escritura de caché, total y coste se corresponden con campos respaldados por evidencia oficial; una métrica no persistida aparece como ausente, no como cero inventado.
7. La consulta selecciona solo columnas y filas de metadatos de uso necesarias y dispone de una estrategia acotada comprobable para refrescos repetidos.
8. Dos solicitudes de refresco solapadas no lanzan scans OpenCode duplicados.
9. Una lectura OpenCode lenta o fallida no retrasa el refresco de providers, tokens Claude ni uso Pi, y no bloquea la TUI.
10. La TUI muestra «OpenCode hoy» separada de «Pi hoy» con estados de datos, vacío y error comprensibles.
11. Claude, Codex, caché de providers, «Pi hoy», `--json` y `--waybar` mantienen su comportamiento y contratos actuales.
12. Las pruebas no acceden a `~/.local/share/opencode/opencode.db`, no dependen de OpenCode instalado y no contienen prompts, credenciales ni filas reales.
13. El desarrollo sigue TDD estricto con una base/schema fixture temporal: RED antes de GREEN, triangulación de bordes y refactor posterior.

## Fuera de alcance

- Cuota, allowance restante, límites remotos o APIs remotas de OpenCode.
- Lanzar, controlar, actualizar o reparar OpenCode.
- Leer autenticación, credenciales, cuentas, prompts, mensajes, partes, herramientas o cualquier contenido bruto.
- Modificar el esquema, datos, índices, WAL o configuración de OpenCode.
- Migrar el almacenamiento JSON heredado, salvo que el map pruebe que la versión actual lo requiere; en ese caso se deberá reabrir el alcance antes de implementarlo.
- Vista por proyecto, historial, siete días o rediseño general de la TUI.
- Cambios al backoff o locking del refresco de providers existente.
- Exponer OpenCode por JSON o Waybar.
- Fusionar Pi y OpenCode en un único total.

## Investigación acotada para `sdd-map`

La siguiente fase debe resolver estas preguntas sin leer datos sensibles:

1. ¿Cuál es la versión/esquema oficial aplicable y dónde define OpenCode los campos de uso del asistente, proveedor, modelo, timestamp y coste?
2. ¿Qué tabla y columna contienen cada categoría, y cuáles son opcionales o no persistidas?
3. ¿Qué unidad y zona temporal usa el timestamp? ¿Qué unidad/precisión usa el coste?
4. ¿Cuál es la clave estable que evita duplicados y distingue mensajes o revisiones?
5. ¿Qué índices oficiales existen sobre el rango temporal y las relaciones mínimas requeridas?
6. ¿Puede resolverse el agregado diario con un rango indexado? Si no, ¿qué watermark monotónico y caché local ofrecen el menor coste correcto?
7. ¿Qué seam actual de «Pi hoy» conviene reutilizar y qué estado debe permanecer independiente?
8. ¿Qué dependencia SQLite mínima y mantenida encaja con el proyecto Rust, si aún no existe una?

### Fuentes permitidas

- Esquema y código fuente oficiales de OpenCode mediante webfetch.
- Metadatos locales acotados: versión de esquema, `sqlite_master`, `PRAGMA table_info`, `PRAGMA index_list` y `PRAGMA index_info` solo para tablas candidatas no sensibles.
- Como máximo, planes `EXPLAIN QUERY PLAN` de consultas sin contenido y contra fixtures o metadatos permitidos.

### Fuentes prohibidas

- Cualquier `SELECT` de tablas de auth, account o credentials.
- Prompts, contenido de `message`/`part`, tool args/results o filas reales completas.
- Invocar `opencode`, incluso para obtener su versión o esquema.
- Escanear la base real para inferir datos que puedan obtenerse del esquema o fuente oficial.

## Estrategia de pruebas exigida

- Crear una fixture SQLite temporal y mínima que refleje únicamente las columnas necesarias del esquema probado.
- Cubrir límites de medianoche local, cambio de día, timestamps fuera de rango, métricas opcionales, coste ausente, varios proveedores/modelos y registros duplicables.
- Probar base ausente, ruta XDG, error de apertura/lectura y esquema incompatible.
- Probar consulta/rango o cursor incremental y supresión de refrescos concurrentes sin usar esperas frágiles.
- Añadir regresiones de render/estado para «Pi hoy» y salidas no TUI cuando sea necesario para demostrar preservación.
- Ejecutar las pruebas solo en `sdd-apply`/`sdd-verify`; esta fase no ejecuta build ni test.

## Riesgos y controles

| Riesgo | Control requerido |
|---|---|
| Deriva entre versión instalada, fuente oficial y esquema local | Correlacionar evidencia oficial con inspección local solo de metadatos y fallar con gracia ante incompatibilidad. |
| Escaneo periódico costoso | Exigir rango indexado o cursor/caché local incremental antes de implementar. |
| Contención con OpenCode y su WAL | Conexión de solo lectura, sin checkpoint/copias/escrituras y trabajo independiente. |
| Fuga de contenido sensible | Allowlist de columnas de uso; fixtures sintéticas; prohibición explícita de contenido y filas crudas. |
| Semánticas de tokens no equivalentes a Pi | Documentar el mapping campo por campo y representar ausencia, no cero. |
| Doble conteo | Identificar claves/relaciones oficiales y cubrirlas con fixture. |
| Regresión del refresco existente | Estado y ejecución independientes más pruebas de no bloqueo y regresión. |
| Documentación arquitectónica desactualizada respecto a «Pi hoy» | En `sdd-map`, tomar el código actual como autoridad y registrar la deriva sin ampliar este cambio. |

## Condiciones para pasar a diseño

`sdd-map` debe entregar evidencia suficiente del esquema y sus índices, el mapping de métricas, la semántica temporal, la estrategia de consulta acotada y los seams actuales de «Pi hoy». Si no puede demostrar una lectura diaria eficiente sin tocar la base de OpenCode, el diseño debe detenerse y proponer una alternativa local acotada dentro de lazysubs; no se permite relajar privacidad ni seguridad para avanzar.
