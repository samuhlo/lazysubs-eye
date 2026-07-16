# A. Proposal — rendimiento medido y optimizado

## Intent

Eliminar la complejidad algorítmica O(E²) en tokens Pi, implementar providers
en paralelo con timeouts por operación y budget global, añadir streaming donde
corresponda, y establecer budgets de rendimiento con regresión detectable.

## Spec

### R1. Eliminación de Pi O(E²)

El scanner de tokens Pi actualmente reenumera todos los archivos de sessions
en cada refresh, hace fingerprinting completo de cada archivo antes de decidir
si cambió, y re-parsea archivos que no cambiaron. La complejidad es O(E²) en
el número de sesiones.

**MUST** implementarse con un cursor/incrementalidad que:
- Solo re-parsee un archivo si su fingerprint (mtime + size + inode) cambió.
- Use el watermark/offset ya existente para los archivos append-only.
- En steady state (ningún archivo cambió), el costo debe ser O(1): un stat
  por archivo y 0 lecturas de contenido.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Steady state (100 sesiones, ninguna cambió) | 100 archivos con mismo mtime/size | Refresh de 60s | O(100) stats + 0 parseo = <10ms |
| Un archivo cambió | 1 de 100 archivos tiene mtime nuevo | Refresh | Solo se parsea ese archivo |
| Nuevo archivo | Sesión nueva | Refresh | Se parsea solo el archivo nuevo |

### R2. Providers en paralelo con timeouts

`collect_all` actualmente llama a cada provider secuencialmente. Si uno está
lento (MiniMax con latencia de 5s), la respuesta total tarda 5s+.

**MUST** implementarse con:
- Cada provider se ejecuta en su propio thread pool o `spawn`.
- Cada provider tiene un timeout individual elegido después de medir el baseline.
- Si el timeout expira, el provider degrada a `Stale` con los datos previos.
- Existe un presupuesto global elegido y documentado: si se agota, los providers
  pendientes cancelan y degradan.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Todos rápidos | Cada provider responde dentro de su baseline | Refresh | Todos contribuyen; total dentro del presupuesto elegido |
| Uno lento | Un provider supera su presupuesto | Refresh | Ese provider degrada; los demás muestran sus datos |
| Presupuesto agotado | Providers lentos agotan el presupuesto global | Refresh | Providers pendientes cancelan; se retorna lo disponible con Stale |

### R3. Scheduling acotado y coalescing

Los refreshes de providers **MUST** coalescerse: si un refresh está en
progreso cuando llega otro, el segundo se coalesce con el primero (no se
lanza otro worker). El intervalo mínimo entre refreshes es configurable
(default 30s).

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Refresh solapado | Refresh activo; llega nuevo refresh a los 10s | Se programa | Se coalesce con el activo; no se lanza segundo worker |
| Intervalo mínimo | Refresh acaba de terminar; usuario fuerza refresh | Se ejecuta | Espera 0s (el intervalo mínimo se cuenta desde el último refresh que terminó) |

### R4. SQLite por ciclo y streaming

Para la lectura de history.db y OpenCode SQLite, **MUST** implementarse
streaming de resultados si la consulta supera el umbral elegido: se procesan
las filas en batches configurables y no se cargan todas en memoria.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| 10k registros | Consulta retorna 10k filas | Se itera | Se procesan en batches; memoria O(batch), no O(10k) |
| 50 registros | Consulta retorna 50 filas | Se itera | Se procesan en un solo batch |

### R5. Performance budgets y regresión

Primero **MUST** medirse un baseline reproducible. Después se definirán
presupuestos como ceiling values, no como resultados ya alcanzados:

| Métrica | Budget |
|---------|--------|
| Waybar cacheado (datos frescos) | < 10 ms |
| Primer render TUI | < 150 ms |
| Escaneo incremental Pi (sin cambios, 100 sesiones) | < 500 ms |
| Refresh completo (todos los providers) | Valor elegido después del baseline |
| Backfill por día | Valor elegido después del baseline |

Los budgets **MUST** verificarse en CI con tests de rendimiento que fallen
si el budget no se cumple. El baseline se registra en un archivo JSON.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Budget violado | El escaneo incremental Pi tarda 800ms | Se ejecuta en CI | Test falla; se abre issue automáticamente |
| Regresión | El escaneo incremental Pi era 200ms, ahora es 800ms | Se detecta | CI falla; se notifica |

## Decisions

1. **Thread pool con timeout**: se elige `std::thread::spawn` con `Arc<AtomicBool>`
   para timeout, no `async/await` con Tokio, porque el overhead de async no
   justifica el caso de uso (3-5 workers concurrentes).
2. **Presupuesto global con cancelabilidad**: el valor y los timeouts por
   provider se eligen después de medir. El presupuesto evita bloqueos
   indefinidos sin convertir un número no medido en contrato.
3. **Batch configurable para SQLite**: el tamaño se elige con fixtures
   representativos; el diseño no fija un valor antes de medir.

## Success Criteria

- Steady state Pi: O(100) stats + 0 parseo < 10ms.
- Providers en paralelo: refresh total dentro del budget global (medido tras baseline).
- Budget de waybar cacheado < 10ms verificable en CI.
- Tests de regresión en CI que fallen ante regresiones de rendimiento.

## Baseline y presupuestos elegidos (2026-07-16)

Entorno: Linux x86_64, fixtures locales, perfil debug para tests y release para
smoke. El registro machine-readable está en `perf/baseline.json`. Se adoptan:
10 ms para Waybar cacheado, 150 ms primer render, 500 ms incremental Pi con
100 sesiones, 8 s de presupuesto global de refresh y 500 ms por día de
backfill. HTTP usa connect/write 3 s y read 5 s; Codex conserva su timeout RPC.
SQLite procesa lotes de 128 filas (la consulta agregada de totales usa techo
256) y los gates admiten 10% de variación por ruido de CI.
