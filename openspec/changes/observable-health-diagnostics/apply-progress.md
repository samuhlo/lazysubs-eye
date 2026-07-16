# Apply progress — observable-health-diagnostics

Estado: implementado. Se añadieron E001–E008, saneado central, validación
semántica al cargar, contrato `--check` 0–3, `doctor` humano/JSON con config,
providers, paths, binario, DB, índices, permisos, último error y notify-send,
además de `--verbose` sin secretos.

Archivos: `src/diagnostics.rs`, `src/config.rs`, `src/main.rs`, `src/notify.rs`,
`tests/cli_check.rs`, `tests/cli_smoke.rs`, README y docs del change.
Decisión: notify-send nunca crea exit 4; `history_days=0` conserva “sin límite”.
Siguiente paso: archivar el change después de integrar el conjunto.
