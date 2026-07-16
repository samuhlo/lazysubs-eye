# Alcance: rendimiento medido y optimizado

## SCOPE PACKET

```yaml
scope: Eliminar O(E²) en tokens Pi, implementar providers en paralelo con
  timeouts, añadir streaming para SQLite, y establecer budgets de rendimiento
  con regresión detectable en CI.
change_name: runtime-performance
budget_allocated:
  max_tokens: 18000
  max_reads: 20
  max_runtime_ms: 800000
webfetch: false
strict_tdd: true
artifact_language: es
```

## Resultado esperado

Rendimiento predecible: waybar cacheado <10ms, primer render TUI <150ms y
escaneo incremental <500ms como objetivos iniciales. El presupuesto global y
los timeouts se fijan después de medir el baseline. Ninguna operación puede
bloquear indefinidamente.

## Hechos de partida

Estos hechos están verificados:

- lazysubs-eye es Rust y usa `cargo test`.
- Pi tokens tiene O(E²): cada refresh re-enumera y re-fingerprints todos los archivos.
- Providers se llaman secuencialmente.
- No hay budgets de rendimiento documentados.
- No hay tests de rendimiento en CI.

## Criterios de aceptación

1. Steady state Pi: <10ms para 100 sesiones sin cambios.
2. Refresh global: cumple el presupuesto elegido después del baseline.

Valores cerrados tras baseline: refresh global 8 s, HTTP 3/5/3 s
(connect/read/write), batch streaming 128, tolerancia CI 10%. El detalle y el
entorno están en `perf/baseline.json`.
3. Budget de waybar: <10ms verificable.
4. Tests de regresión en CI.

## Fuera de alcance

- Reescritura async completa con Tokio.
- Cache distribuido o memoria compartida.
- Profile-guided optimization.

## No objetivos

- No cambiar el formato de salida de waybar/json.
- No cambiar los contracts de los providers.
