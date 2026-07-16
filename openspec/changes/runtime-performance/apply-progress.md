# Apply progress — runtime-performance

Estado: implementado. Pi hace stat+fingerprint y cero opens sin cambios;
familias provider corren en paralelo con HTTP 3/5/3 s y budget global 8 s;
pendientes degradan a stale/error; refreshes se coalescen; SQLite procesa
batches; budgets y baseline viven en `src/performance.rs` y `perf/baseline.json`.

Archivos: providers, `pi_tokens.rs`, `tui.rs`, `history.rs`, `performance.rs`,
baseline y script de budgets. Tolerancia CI: 10%.
Siguiente paso: conservar el baseline como referencia y archivar el change.
