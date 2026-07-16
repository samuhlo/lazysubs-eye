# Verify report — runtime-performance

- Parallel budget rápido/lento y orden: OK.
- Pi steady state lee 0 bytes; cutoff excluye append concurrente: OK.
- Streaming 10,000 filas, batch máximo 128: OK.
- Scheduler coalesce y conserva force pendiente: OK.
- `scripts/benchmarks/run_budgets.sh`, suite 160+3 y gates globales: OK.
