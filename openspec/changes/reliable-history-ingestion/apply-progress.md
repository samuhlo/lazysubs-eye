# Apply progress — reliable-history-ingestion

Estado: implementado. Estado por fuente/día y fingerprint se confirman en la
misma transacción; `ingest_single_day` salta sin tocar meta o registra Failed
separadamente; backfill corre en worker, persiste progreso, se cancela entre
días y reanuda desde el último día contiguo; cutoff Pi queda acotado al tamaño
snapshot y OpenCode usa snapshot SQLite; procedencia, estado y retención son
por fuente. La TUI presenta el último estado persistido y no infiere éxito por
la mera ausencia de filas.

Archivos: `src/history.rs`, `src/pi_tokens.rs`, `src/opencode_tokens.rs`, `src/tui.rs`.
Riesgo residual: fuentes corruptas se degradan a Partial/Failed sin conservar
mensajes crudos, por diseño.
Siguiente paso: archivar el change después de integrar el conjunto.
