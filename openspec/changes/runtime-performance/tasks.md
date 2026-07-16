# Tasks — runtime-performance

status: complete
blocked_by: reliable-history-ingestion

> Cada ciclo TDD cubre una capacidad funcional completa: RED (test que falla antes
> de implementar), GREEN (implementación mínima para pasar), TRIANGULATE (casos
> borde/adicionales), REFACTOR (limpieza, integración, revisión de comentarios).
> Al final se añaden fases separadas para documentación, suite completo, y
> preparación de apply-progress.md/verify-report.md durante ejecución.

## // 000. Baseline y presupuestos

- [x] 0.1 (SPIKE) Medir baseline con fixtures representativos y elegir
  presupuestos por provider, globales y de batch; registrar entorno, variación
  y justificación antes de implementar timeouts o gates
  - skills: `ein-discipline`, `performance`
  - verify: baseline y decisiones registradas en design.md, scope.md y map.md

## // 001. Fingerprint incremental para Pi

- [x] 1.1 (RED) Test que verifica `check_fingerprint` hace stat + compare
  sin abrir archivo; changed → open triggered; unchanged → no open
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib pi_tokens::tests::fingerprint_*` — rojos

- [x] 1.2 (GREEN) Implementar `PiFileStore::check_fingerprint(path, cached_mtime,
  cached_size, cached_ino) -> FingerprintResult` con std::fs::metadata();
  modificar `PiFileStore::scan` para usar check_fingerprint antes de parsear
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib pi_tokens::tests::fingerprint_*` — de rojo a verde:
    unchanged → no open; changed → open triggered
    `cargo test --lib pi_tokens::tests::incremental_scan_*` — verde:
    100 archivos sin cambios → 0 parseos; 1 cambiado → solo 1 parseo

- [x] 1.3 (TRIANGULATE) Tests: fingerprint con mtime igual pero size diferente;
  fingerprint con ino diferente; archivo truncado
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib pi_tokens::tests::fingerprint_edge_*` — verde

- [x] 1.4 (REFACTOR) Documentar por qué se guardan 3 campos (mtime, size, ino);
  verificar que fingerprint se persiste (no se pierde al restart)
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por fingerprint en persistencia

## // 002. Providers en paralelo con budget global

- [x] 2.1 (RED) Test que verifica `collect_all_parallel` spawnea threads por
  provider; uno lento no bloquea los demás; budget global agota y cancela pendientes
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib providers::tests::parallel_collect_*` — rojos

- [x] 2.2 (GREEN) Implementar `collect_all_parallel(providers: &[Provider],
  budget_ms: u64) -> CollectedResults` con std::thread::spawn; configurar
  timeouts individuales y presupuesto global elegidos en el baseline
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib providers::tests::parallel_collect_*` — de rojo a verde:
    uno lento no bloquea; budget agota → cancela pendientes
    `cargo test --lib providers::tests::timeouts_*` — verde:
    timeout expira → provider degrada a Stale

- [x] 2.3 (TRIANGULATE) Tests: un provider rápido retorna antes de que otro
  comece; budget se agota a mitad; todos los providers fallan
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib providers::tests::parallel_edge_*` — verde

- [x] 2.4 (REFACTOR) Documentar por qué se usa budget global y no por-provider;
  verificar que collect_all_parallel es el nuevo default
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por collect_all en main

## // 003. Coalescing de refreshes

- [x] 3.1 (RED) Test que verifica `RefreshScheduler::maybe_schedule_refresh`
  coalesce refreshes solapados; dos refreshes solapados → solo uno en ejecución
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::refresh_coalescing_*` — rojos

- [x] 3.2 (GREEN) Implementar `RefreshScheduler` con Arc<AtomicBool> refreshing;
  si refreshing.load() es true, almacenar pending refresh; cuando activo
  termina, ejecutar pending
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::refresh_coalescing_*` — de rojo a verde:
    dos refreshes solapados → solo uno en ejecución

- [x] 3.3 (TRIANGULATE) Tests: refresh pendiente se ejecuta al terminar actual;
  force=true ignora coalescing; múltiples refresh con force
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::refresh_coalescing_*` — verde

- [x] 3.4 (REFACTOR) Documentar por qué se usa AtomicBool en lugar de Mutex;
  verificar que no hay deadlock con pending refresh
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; `cargo clippy --all-targets -- -D warnings`

## // 004. Streaming SQLite con batches

- [x] 4.1 (RED) Test que verifica streaming con 10k filas procesa en batches
  del tamaño elegido; memoria O(batch), no O(total)
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib history::tests::streaming_query_*` — rojos

- [x] 4.2 (GREEN) Implementar `query_streaming<T>(conn: &Connection, sql: &str,
  batch_size: usize, mapper: F) -> Vec<T>` con rusqlite iterator sobre filas;
  colectar en Vec de batch_size, procesar, continuar
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib history::tests::streaming_query_*` — de rojo a verde:
    10k filas → batches procesados; memoria O(batch)

- [x] 4.3 (TRIANGULATE) Tests: batch en límite exacto; mapper falla en mitad;
  query vacía
  - skills: `ein-discipline`, `architecture`, `performance`
  - verify: `cargo test --lib history::tests::streaming_edge_*` — verde

- [x] 4.4 (REFACTOR) Documentar cómo se eligió `batch_size`;
  verificar que streaming se usa donde corresponde
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por query_streaming en código

## // 005. Budgets y CI de regresión

- [x] 5.1 (RED) Test que verifica `PerformanceBudget` struct tiene los
  budgets documentados; `measure_budget` mide y falla si supera budget
  - skills: `ein-discipline`, `performance`
  - verify: `cargo test --lib performance::tests::budget_*` — rojos;
    `cargo test --lib performance::tests::budget_enforcement_*` — rojos

- [x] 5.2 (GREEN) Definir `PerformanceBudget` con waybar_cached_ms, first_render_ms,
  incremental_scan_ms, refresh_global_ms; implementar `measure_budget` con
  Instant::now() + assert
  - skills: `ein-discipline`, `performance`
  - verify: `cargo test --lib performance::tests::budget_*` — de rojo a verde;
    `cargo test --lib performance::tests::budget_enforcement_*` — verde

- [x] 5.3 (TRIANGULATE) Tests: budget con warm-up runs; budget con tolerancia
  10% variance; baseline.json existe y tiene valores para cada metric
  - skills: `ein-discipline`, `performance`
  - verify: `cargo test --lib performance::tests::budget_tolerance_*` — verde;
    ls perf/baseline.json

- [x] 5.4 (REFACTOR) Documentar qué es cada budget y por qué; registrar baseline
  de rendimiento en perf/baseline.json; scripts/benchmarks/run_budgets.sh
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; baseline.json existe con valores

## // 006. Suite completo y preparación

- [x] 6.1 (VERIFY) Ejecutar suite completo: `cargo test --locked`
  - skills: `ein-discipline`
  - verify: todos los tests pasan

- [x] 6.2 (VERIFY) Quality gates finales: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
  - skills: `ein-discipline`
  - verify: sin errores

- [x] 6.3 (VERIFY) Preparar apply-progress.md con: tareas completadas, archivos tocados,
  decisiones técnicas (budgets decididos, timeouts chosen), riesgos, siguiente paso
  - skills: `ein-discipline`
  - verify: apply-progress.md existe y está completo tras ejecución

- [x] 6.4 (VERIFY) Preparar verify-report.md con: comandos ejecutados, output relevante,
  evidencia de que cada gate pasó, baseline de rendimiento
  - skills: `ein-discipline`
  - verify: verify-report.md existe y contiene evidencia tras verificación

## // 007. Documentación

- [x] 7.1 (DOCS) Documentar O(E) bootstrap y O(delta) incremental en comentarios
  del módulo; explicar por qué Pi se beneficia
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de PiFileStore

- [x] 7.2 (DOCS) Documentar los budgets y batch size elegidos tras el baseline
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: valores, entorno y justificación constan en docs y verify-report
