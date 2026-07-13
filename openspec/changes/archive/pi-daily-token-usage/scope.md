# Alcance: consumo diario de tokens de Pi/EIN

## SCOPE PACKET

```yaml
scope: >-
  Añadir a lazysubs contabilidad local del consumo diario de sesiones Pi/EIN,
  agrupada por provider/modelo y visible en la TUI, mediante un parser e índice
  incremental que evite volver a leer todo el almacén de sesiones en cada refresco.
budget_allocated:
  max_tokens: 20000
  max_reads: 40
  max_runtime_ms: 180000
```

## Resultado esperado

lazysubs mostrará una sección compacta **`Pi/EIN hoy`** con el uso registrado en las entradas de asistente de las sesiones Pi del día local actual. La vista inicial se agrupará por `provider` y `model` e incluirá entrada, salida, lectura y escritura de caché, total de tokens y coste registrado.

El cálculo incluirá tanto las sesiones principales como las sesiones Pi anidadas que crea EIN. La actualización será local, tolerante a archivos incompletos y desacoplada del refresco de providers y de la agregación actual de tokens de Claude.

## Contrato de datos de entrada

- Raíz predeterminada: `~/.pi/agent/sessions/`, recorrida de forma recursiva para incluir sesiones principales y subagentes EIN.
- Formato: archivos JSONL de sesiones Pi.
- Única fuente contable: entradas de mensaje de tipo asistente que tengan `provider`, `model`, `usage` y timestamp Unix en milisegundos.
- Métricas de `usage`: `input`, `output`, `cacheRead`, `cacheWrite`, `totalTokens` y `cost.total`.
- Frontera temporal: día natural de la zona horaria local del proceso.
- Identidad: el id estable de la entrada de asistente se usará para evitar dobles conteos entre originales, forks y clones.
- Los usos de `pi --no-session` no son recuperables. Las ubicaciones arbitrarias pasadas mediante `--session-dir` no se autodetectarán.

## Incluido

- Descubrir recursivamente los JSONL bajo la raíz predeterminada de sesiones Pi.
- Agregar únicamente entradas de asistente pertenecientes al día local actual.
- Incluir sesiones normales de Pi y sesiones anidadas de subagentes EIN sin tratarlas como fuentes distintas.
- Agrupar el resultado por la pareja `provider`/`model`.
- Exponer por grupo los contadores de entrada, salida, lectura de caché, escritura de caché y total, además de la suma del coste registrado.
- Persistir en la caché de lazysubs un cursor o índice incremental suficiente para reanudar cada archivo desde un offset de bytes seguro.
- Leer únicamente el sufijo nuevo de archivos append-only ya indexados.
- Detectar archivos nuevos, reemplazados, truncados o incompatibles con el cursor guardado y reconstruirlos de forma segura.
- Deduplicar por id estable de entrada incluso cuando un fork o clon nuevo copie historial ya contabilizado.
- Reiniciar o reconstruir la vista al cambiar el día local, sin arrastrar consumo del día anterior.
- Presentar en la TUI una sección `Pi/EIN hoy` con cifras abreviadas legibles y coste claramente etiquetado.
- Hacer que archivos vacíos, desaparecidos, malformados o con una última línea todavía en escritura degraden sin bloquear la TUI ni invalidar el agregado válido restante.
- Mantener la obtención y entrega del estado Pi separada del refresco remoto de providers y del cálculo local actual de Claude.

## No incluido

- Soporte para OpenCode; tendrá un SDD independiente.
- Historial de siete días u otros rangos temporales.
- Desglose por proyecto, cwd, sesión, agente o conversación.
- Cuotas, límites, allowance restante o llamadas a APIs remotas.
- Autodescubrimiento de directorios Pi arbitrarios configurados con `--session-dir`.
- Recuperación de ejecuciones efímeras con `pi --no-session`.
- Locking, backoff o tratamiento de `Retry-After` en el refresco de providers.
- Rediseño amplio de la TUI.
- Alteraciones funcionales de los collectors de Claude/Codex, el contrato Waybar o el panel actual de tokens de Claude.

## Criterios de aceptación

### Exactitud

- [ ] Se contabilizan solo entradas de asistente con uso registrado y timestamp dentro del día local actual.
- [ ] Las sesiones Pi principales y las sesiones anidadas de EIN bajo la raíz predeterminada contribuyen al mismo agregado.
- [ ] Cada fila de la TUI identifica `provider` y `model` y muestra entrada, salida, lectura de caché, escritura de caché, total de tokens y coste registrado.
- [ ] Los totales por grupo son la suma de los campos registrados; no se estiman precios ni tokens ausentes.
- [ ] Dos copias de una misma entrada con el mismo id estable cuentan una sola vez, aunque estén en archivos distintos por fork o clone.
- [ ] El cambio de fecha local elimina del estado visible el consumo del día anterior y reconstruye el día nuevo.

### Incrementalidad y rendimiento

- [ ] Tras crear el índice, un archivo append-only conocido se lee desde su último offset seguro y no desde el byte cero.
- [ ] Un refresco periódico no vuelve a efectuar un escaneo recursivo del contenido completo de los 713 JSONL medidos (~176 MB).
- [ ] Los archivos nuevos se incorporan; los truncados, reemplazados o cuyo cursor deja de ser válido se recuperan mediante un reescaneo seguro y acotado al archivo afectado.
- [ ] El cursor solo avanza hasta la última entrada JSONL completa procesada, de modo que una escritura en curso pueda retomarse después sin perder ni duplicar uso.
- [ ] El índice persistido permite que una nueva ejecución continúe incrementalmente sin reconstruir todo el almacén durante cada ciclo de 60 segundos.
- [ ] El cálculo Pi no introduce una segunda exploración completa en background ni bloquea el arranque o el renderizado de la TUI.

### Robustez y privacidad

- [ ] La ausencia de la raíz, un directorio vacío, archivos que desaparecen durante el recorrido, permisos insuficientes, líneas malformadas o una cola incompleta producen estado vacío/parcial o un error aislado, no un fallo global.
- [ ] El parser limita la deserialización a los metadatos necesarios para identificar, fechar, agrupar, deduplicar y sumar uso.
- [ ] Prompts, respuestas, credenciales y contenido conversacional no se muestran, registran ni persisten en la caché incremental.
- [ ] Entradas sin id estable, timestamp válido, provider/modelo o usage utilizable se omiten de forma segura y no contaminan los totales.

### Compatibilidad

- [ ] Claude y Codex conservan su refresco, errores y presentación actuales.
- [ ] La salida Waybar mantiene sus claves, texto, tooltip, clase, porcentaje y umbrales actuales.
- [ ] La caché de estado existente sigue siendo legible; la persistencia incremental de Pi no rompe ni cambia el significado de sus campos actuales.
- [ ] El panel actual de tokens diarios de Claude conserva sus cifras y comportamiento.
- [ ] El refresco manual y el auto-refresco siguen entregando estado a la TUI sin esperar en el hilo de renderizado.

## Restricciones de arquitectura

1. **Incremental por defecto:** el coste de contenido de un refresco normal debe depender de bytes nuevos o archivos afectados, no del tamaño acumulado de `~/.pi/agent/sessions/`.
2. **Separación de responsabilidades:** descubrir/indexar sesiones Pi, agregar uso diario y presentar la sección TUI deben mantener límites claros; Pi no se modelará como un provider remoto de cuotas.
3. **Una sola producción de estado:** el resultado Pi debe viajar a la TUI por el flujo de estado/background existente o una extensión equivalente, sin lanzar recorridos duplicados desde el render.
4. **Persistencia mínima:** el índice guardará solo identidad técnica, cursores y datos de uso necesarios para exactitud diaria; nunca contenido de mensajes.
5. **Compatibilidad aditiva:** cualquier evolución de estructuras serializadas será opcional o tendrá una migración tolerante para cachés previas.

## Escala y presupuesto de rendimiento

La referencia local medida para orientar diseño y pruebas posteriores es:

| Señal | Medición |
|---|---:|
| Archivos JSONL totales | 713 |
| Tamaño acumulado | ~176 MB |
| Archivos modificados hoy | 86 |
| Tamaño de los archivos modificados hoy | ~21 MB |
| Entradas de asistente con uso hoy | 1.552 |
| Intervalo actual de refresco | 60 s |

Estas cifras justifican el índice incremental. No convierten esta fase en una optimización general del almacén ni autorizan un escaneo completo periódico.

## Riesgos acotados

- Los forks y clones pueden copiar bloques históricos completos; deduplicar solo por archivo u offset inflaría el consumo.
- Un archivo puede truncarse o sustituirse conservando la misma ruta; confiar únicamente en longitud o mtime puede dejar un cursor inválido.
- La última línea puede estar parcialmente escrita durante el refresco; avanzar el cursor antes de una línea completa perdería datos.
- El cambio de zona horaria, horario de verano o fecha mientras la TUI está abierta puede invalidar la frontera diaria y exigir reconstrucción.
- Un índice corrupto o de una versión anterior debe poder descartarse y reconstruirse sin inutilizar la caché principal.
- Sumar `cost.total` requiere conservar suficiente precisión para no introducir errores visibles de redondeo.
- El estado Pi y el panel Claude manejan métricas parecidas pero contratos distintos; fusionarlos prematuramente podría romper compatibilidad o semántica.

## Preguntas para resolver en map/design

- Qué señal mínima y portable distinguirá append, truncado y reemplazo de un archivo en el sistema objetivo.
- Qué esquema/versionado tendrá el índice y si convivirá en `status.json` o en un archivo de caché separado.
- Cómo se conservará el conjunto de ids vistos del día con memoria y tamaño acotados sin perder deduplicación entre archivos.
- Cómo entrará el agregado Pi en el estado background de la TUI sin acoplarlo a `collect_all()` ni recalcular Claude.
- Qué representación numérica y formato visual preservarán el coste registrado con precisión y lectura compacta.

## Verificación diferida

Las fases posteriores deberán diseñar y ejecutar casos para bootstrap, append, archivo nuevo, fork/clone duplicado, truncado, reemplazo, línea parcial, JSON malformado, índice corrupto o antiguo, raíz ausente, medianoche local, agrupación provider/modelo, precisión de coste y compatibilidad de TUI/Claude/Codex/Waybar/caché. Esta fase no ejecuta builds ni tests.
