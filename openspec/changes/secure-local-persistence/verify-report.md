# Verify report — secure-local-persistence

- Contención multiproceso real: `multiprocess_lock_reports_real_contention`, OK.
- Lock reemplazado: `lost_lock_aborts_before_replacing_destination`, destino intacto.
- Symlink destino/padre, permisos 0600/0700 y rename fallido: OK.
- Release x86_64: 6,25 MiB (6.551.608 bytes); dependencia nueva `libc` (ya
  transitiva en gran parte del grafo); producción Rust: 11.902 LOC en la
  medición de cierre.
- Suite 160 unit + 3 integración; gates fmt/Clippy/tests: OK.
