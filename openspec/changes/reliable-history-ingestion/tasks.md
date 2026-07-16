# Tasks — reliable-history-ingestion

status: complete
blocked_by: secure-local-persistence

> Cada ciclo TDD cubre una capacidad funcional completa: RED (test que falla antes
> de implementar), GREEN (implementación mínima para pasar), TRIANGULATE (casos
> borde/adicionales), REFACTOR (limpieza, integración, revisión de comentarios).
> Al final se añaden fases separadas para documentación, suite completo, y
> preparación de apply-progress.md/verify-report.md durante ejecución.

## // 001. Schema de meta y tipos de estado de ingesta

- [x] 1.1 (RED) Test que verifica `IngestState` enum con variants `Ingested`,
  `Partial`, `InProgress`, `Pending`, `Skipped`, `Failed`; y que la tabla
  `meta` persiste el estado por fuente y día
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::ingest_state_*` — rojos;
    `cargo test --lib history::tests::meta_days_*` — rojos

- [x] 1.2 (GREEN) Implementar `IngestState` enum y las claves necesarias en `meta`;
  modificar `insert_day_stats` para aceptar `IngestState` y actualizar `meta`
  dentro de la misma transacción
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::ingest_state_*` — de rojo a verde;
    `cargo test --lib history::tests::meta_days_*` — verde

- [x] 1.3 (TRIANGULATE) Tests: round-trip SQLite de `IngestState`; meta vacío
  inicializa sin estados; Partial y Failed incluyen reason sanitizada
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::ingest_state_serde_*` — verde

- [x] 1.4 (REFACTOR) Documentar el esquema de claves de meta por fuente y día;
  verificar que IngestState no expone detalles crudos de SQLite
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; `cargo clippy --all-targets -- -D warnings`

## // 002. Transacciones independientes por día y rollback

- [x] 2.1 (RED) Test que verifica `ingest_single_day` usa una transacción;
  hace rollback si hay error, conserva datos válidos previos y registra el
  estado fallido sin reemplazarlos
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::ingest_single_day_*` — rojos;
    `cargo test --lib history::tests::rollback_on_failure` — rojo

- [x] 2.2 (GREEN) Implementar `ingest_single_day(conn, day, source_data) ->
  Result<IngestState, IngestError>` con transacción de datos + meta en éxito y
  transacción separada solo de estado tras un fallo previo al commit;
  implementar `backfill(days: Range<NaiveDate>, source) -> BackfillResult`
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::ingest_single_day_*` — de rojo a verde:
  exitoso → Ingested; parse recuperable → Partial; fallo de persistencia →
  error con rollback y estado anterior intacto
    `cargo test --lib history::tests::backfill_*` — verde: el fallo en un día
    no impide procesar los días siguientes

- [x] 2.3 (TRIANGULATE) Tests: backfill con 30 días y fallo en día 15 verifica
  que 15=Partial/Failed, 16-30 se procesan y el reintento solo repite fallidos;
  doble ingesta sin duplicados
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::backfill_integration_30_days` — verde;
    `cargo test --lib history::tests::double_backfill_no_duplicates` — verde

- [x] 2.4 (REFACTOR) Verificar que éxito actualiza registros y meta juntos,
  mientras un fallo solo actualiza el estado sin reemplazar agregados válidos;
  documentar por qué las transacciones son independientes
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de `set_meta`; `cargo test --lib history::tests::meta_persistence_*`

## // 003. Backfill no bloqueante y progreso

- [x] 3.1 (RED) Test que verifica `App::new` retorna antes de que el thread
  de backfill termine; `App::render` se ejecuta sin esperar; `backfill_progress`
  se actualiza tras cada día
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::*backfill*` — rojos

- [x] 3.2 (GREEN) Mover lógica de backfill a `std::thread::spawn`; añadir
  `backfill_progress: Arc<RwLock<BackfillProgress>>` a App con campos
  `current_day`, `total_days`, `ingested_count`, `failed_count`
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::*backfill*` — de rojo a verde;
    `App::new` retorna antes de que thread termine

- [x] 3.3 (TRIANGULATE) Tests: render muestra banner de backfill con progreso;
  dismiss con cualquier key; backfill_last_day persiste en meta para reanudación
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib tui::tests::backfill_resume_*` — verde:
    tras fallar en día 15, meta tiene backfill_last_day=14; al reanudar, empieza en 15

- [x] 3.4 (REFACTOR) Documentar el canal de progreso y la cancelación cooperativa;
  verificar que cerrar la TUI no espera indefinidamente al worker
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de App; `cargo clippy --all-targets -- -D warnings`

## // 004. Cutoff consistente

- [x] 4.1 (RED) Test que verifica seam de cutoff en pi_tokens.rs produce
  timestamp correcto; `ingest_single_day` aplica cutoff antes de SELECT
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib pi_tokens::tests::cutoff_*` — rojos;
    `cargo test --lib history::tests::cutoff_*` — rojos

- [x] 4.2 (GREEN) Extraer función reutilizable `get_cutoff_timestamp(source, day_window) -> i64`;
  aplicar en `ingest_single_day` antes de consultar registros del día
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib pi_tokens::tests::cutoff_*` — de rojo a verde;
    `cargo test --lib history::tests::cutoff_*` — verde:
    filas añadidas tras cutoff se ingieren en siguiente scan, no actual

- [x] 4.3 (TRIANGULATE) Tests: cutoff diferente por fuente; cutoff con
  timezone diferente; cutoff en día sin datos
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::cutoff_*` — verde

- [x] 4.4 (REFACTOR) Verificar que seam se reutiliza sin duplicación;
  documentar por qué cutoff se aplica antes del SELECT
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de diff; grep por get_cutoff_timestamp

## // 005. Idempotencia y fingerprinting

- [x] 5.1 (RED) Test que verifica `compute_day_fingerprint(source, day) ->
  DayFingerprint`; fingerprint diferente si source cambió; mismo fingerprint
  si no cambió
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::fingerprint_*` — rojos

- [x] 5.2 (GREEN) Implementar fingerprint con max_rowid, timestamp de última
  modificación, y file_id; modificar `ingest_single_day` para skip si
  fingerprint no cambió
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::fingerprint_*` — de rojo a verde:
    misma source → mismo fingerprint; source cambió → fingerprint diferente
    `cargo test --lib history::tests::idempotency_*` — verde:
    backfill dos veces: segunda vez todos Skipped; no hay duplicados

- [x] 5.3 (TRIANGULATE) Tests: fingerprint con mtime cambiada pero sin rows nuevas;
  fingerprint con rows nuevas; fingerprint con file_id cambiada
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::fingerprint_edge_*` — verde

- [x] 5.4 (REFACTOR) Documentar por qué se usa fingerprint de 3 campos;
  verificar que skip no modifica meta.days
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios; grep por Skipped en tests

## // 006. Retención, procedencia y estados por fuente

- [x] 6.1 (RED) Test que verifica `RetentionPolicy` por fuente configurable;
  `meta.sources` registra `{ source, first_ingest, last_ingest, record_count }`;
  `meta.days` indexado por (source, date)
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::retention_policy_*` — rojos;
    `cargo test --lib history::tests::source_provenance_*` — rojos

- [x] 6.2 (GREEN) Implementar RetentionPolicy por fuente; registrar procedencia
  de backfill en meta.sources; desglosar estado de backfill por fuente y día
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::retention_policy_*` — de rojo a verde:
    día 31 se prune según política; día 30 se mantiene
    `cargo test --lib history::tests::source_provenance_*` — verde:
    backfill de claude: sources["claude"].first_ingest = día más antiguo
    `cargo test --lib history::tests::backfill_state_by_source_*` — verde:
    claude completo, pi parcial: meta refleja ambos estados

- [x] 6.3 (TRIANGULATE) Tests: prune con acción de archivo; backfill parcial para
  una fuente y completo para otra; fuente sin datos nunca ingesta
  - skills: `ein-discipline`, `architecture`
  - verify: `cargo test --lib history::tests::*source*` — suite completo verde

- [x] 6.4 (REFACTOR) Documentar política de retención por fuente; verificar que
  prune no afecta fuentes diferentes
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios de RetentionPolicy

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
