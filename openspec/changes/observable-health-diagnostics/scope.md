# Alcance: diagnóstico de salud y observabilidad

## SCOPE PACKET

```yaml
scope: Añadir diagnóstico observable: --check semántico con exit codes
  diferenciados (0-3 SOLAMENTE, SIN 4), comando doctor, validación de config,
  errores accionables sin datos sensibles, y documentación de exit codes.
change_name: observable-health-diagnostics
budget_allocated:
  max_tokens: 16000
  max_reads: 20
  max_runtime_ms: 800000
webfetch: false
strict_tdd: true
artifact_language: es
```

## Resultado esperado

El usuario puede diagnosticar el estado de lazysubs-eye sin leer código:
`lazysubs-eye --check` dice exactamente si hay un problema y cuál es,
`lazysubs-eye doctor` inspecciona el entorno, y los errores son accionables.

## Hechos de partida

Estos hechos están verificados y no necesitan redescubrirse:

- lazysubs-eye es un binario Rust; usa `cargo test` y `strict_tdd: true`.
- El binario se llama `lazysubs-eye`.
- `--check` existe y retorna exit codes, pero no distingue stale de unavailable.
- No existe comando `doctor`.
- La config se parsea con serde pero no se valida semánticamente.
- Los errores de SQLite y HTTP se muestran tal cual.
- Exit codes no están documentados en `--help`.
- No hay `--verbose` para logs de diagnóstico.
- No hay sanitización de errores.

## Criterios de aceptación

1. `--check` retorna 0/1/2/3 según el estado real (0-3 SOLAMENTE, SIN 4).
2. `doctor` muestra checks sin secretos, incluyendo notify-send.
3. Config inválida produce error con código corto.
4. Ningún error expone rutas con nombre de usuario ni credenciales.
5. Exit codes en `--help` (0, 1, 2, 3 — no 4).
6. notify-send failure visible en doctor y `--verbose`, sin exit code nuevo.

## Fuera de alcance

- GUI de diagnóstico.
- Logs a archivos (solo stdout/stderr con --verbose).
- Telemetry o métricas remotas.
- Auto-remediation (solo diagnosticar, no arreglar).
- Exit code 4 (requiere decisión explícita futura, no se implementa ahora).

## No objetivos

- No agregar metrics a sistema externo.
- No hacer diagnóstico remoto.
- No cambiar los exit codes existentes de forma incompatible.
- No introducir exit code 4 para notify-send sin decisión explícita.

## Decisiones abiertas (no resueltas en este change)

- **Exit code 4 para notify-send**: si en el futuro se necesita distinguir
  notify-send failure de otros errores de sistema, requeriría decisión
  explícita con justificación. Por ahora, notify-send failure es visible
  en doctor/verbose/stderr pero no cambia el exit code.

Compatibilidad: `history_days = 0` conserva el significado público "retención
sin límite"; sólo se rechazan valores negativos. El resto de validación
semántica (URL, TTL y umbrales) falla con E002.
