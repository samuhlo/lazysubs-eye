# Informe de verificación — `codex-reset-credits`

## Estado

**status: pass**

La remediación queda verificada contra `f4a354f`. Los gates configurados y los checks solicitados pasan. No se modificó código durante esta fase.

**behavior_coverage: verified** — las pruebas unitarias ejercitan el flujo nuevo de parseo/mapeo Codex, incluida la respuesta JSON-RPC con `error`, el contrato serde/JSON, la compatibilidad de caché, la fila y altura TUI, y la invariancia Waybar.

## Evidencia de comandos

| Comando | Resultado | Evidencia |
|---|---|---|
| `cargo test` | PASS | 14 tests passed, 0 failed |
| `cargo build` | PASS | compilación debug completada |
| `rustfmt --check src/output.rs src/providers/claude.rs src/providers/codex.rs src/providers/mod.rs src/tui.rs` | PASS | sin salida, todos los archivos tocados por el SDD conformes |
| `git diff --check` | PASS | sin errores de whitespace |
| `git diff --cached --name-only` | PASS | no hay archivos staged |

No se ejecutó `cargo fmt --check` como gate. La evidencia previa registra drift preexistente en `src/cache.rs`, `src/main.rs` y `src/tokens.rs`, todos fuera de los archivos Rust tocados por este SDD; el formato acotado solicitado sí pasa.

## Matriz RFC 2119 y escenarios

| Req. | Cobertura | Evidencia |
|---|---|---|
| R1 | PASS | `src/providers/codex.rs`: parseo y mapeo conservan `Some(3)`. |
| R2 | PASS | Tests de Codex, serde, `pretty` y TUI conservan `Some(0)` y muestran `...: 0`. |
| R3 | PASS | Objeto omitido, objeto `{}` y campo omitido producen `None`; no se inventa cero. |
| R4 | PASS | `rateLimitResetCredits: null` y `availableCount: null` producen `None`. |
| R5 | PASS | `src/providers/mod.rs`: JSON de caché antigua sin la clave deserializa y conserva campos históricos. |
| R6 | PASS | `src/providers/codex.rs::rate_limits_result_from_response` ejercita id `2` con `error`, conserva el mensaje y alimenta `ProviderStatus::err` sin créditos; TUI/Waybar conservan rama de error. |
| R7 | PASS | `src/output.rs`: `pretty` emite número para `Some(3)`/`Some(0)`, omite `None` y verifica claves históricas. |
| R8 | PASS | `src/output.rs`: Waybar es idéntico con/sin créditos en estado normal y de error; no se alteran selección, umbrales ni cuatro campos. |
| R9 | PASS | `src/tui.rs`: seam único muestra la fila Codex sano; dos ventanas + fila + bordes = altura 5. |
| R10 | PASS | Codex sin créditos, provider no Codex y error mantienen altura 4 y no muestran la fila. |

### Escenario JSON-RPC de error (remediación)

El test `turns_json_rpc_error_responses_into_creditless_error_statuses` construye una respuesta id `2` con `error`, verifica el texto exacto `app-server devolvió error: ...` y comprueba `reset_credits_available == None`, sin lanzar `codex`, usar red ni credenciales.

## Cobertura de tareas

`tasks.md` marca completas las tareas 1.1–5.4. `apply-progress.md` documenta los ciclos RED/GREEN/TRIANGULATE/REFACTOR y la remediación del seam JSON-RPC. Los archivos de test reportados existen como módulos `#[cfg(test)]` dentro de:

- `src/providers/codex.rs`
- `src/providers/mod.rs`
- `src/output.rs`
- `src/tui.rs`

La matriz cubre positivo, cero, ausencia, `null`, entradas inválidas, caché antigua, error, JSON, Waybar y layout.

## Auditoría de alcance y calidad

- El diff de producción contra `f4a354f` está limitado a `src/providers/mod.rs`, `src/providers/codex.rs`, `src/providers/claude.rs`, `src/tui.rs` y `src/output.rs`, todas áreas previstas en el diseño. `src/cache.rs` y `src/main.rs` no recibieron cambios.
- Los cambios de formato visibles están dentro de esos archivos SDD-tocados; el `rustfmt --check` acotado pasa.
- No hay credenciales vivas ni valores de secretos en el diff. Las pruebas usan fixtures sintéticos; las referencias a tokens/endpoints son código preexistente del provider.
- Las aserciones verifican valores y contratos observables (número, ausencia de clave, texto exacto, altura y igualdad completa Waybar); no se detectan tautologías, loops fantasma ni assertions solo de tipos.

## Riesgos residuales

- **Bajo — documentación:** la fila histórica de tarea 005 en `apply-progress.md` menciona 12 tests, mientras la ejecución final actual (tras la remediación) pasa 14. Las líneas posteriores ya registran 14; no afecta al código ni a los gates.
- **Residual conocido:** no se usó el gate global `cargo fmt --check` por drift baseline fuera de alcance, según la instrucción de esta verificación.
- No quedan bloqueadores funcionales identificados.

## Findings

- Ningún blocker o finding de severidad alta/media.

```acceptance-report
{
  "criteriaSatisfied": [
    {
      "id": "criterion-1",
      "status": "satisfied",
      "evidence": "La matriz RFC R1-R10 y los escenarios están verificados con tests existentes; no hay findings blocker/high/medium."
    }
  ],
  "changedFiles": [
    "src/output.rs",
    "src/providers/claude.rs",
    "src/providers/codex.rs",
    "src/providers/mod.rs",
    "src/tui.rs",
    "openspec/changes/codex-reset-credits/tasks.md",
    "openspec/changes/codex-reset-credits/apply-progress.md",
    "openspec/changes/codex-reset-credits/verify-report.md"
  ],
  "testsAddedOrUpdated": [
    "src/providers/codex.rs",
    "src/providers/mod.rs",
    "src/output.rs",
    "src/tui.rs"
  ],
  "commandsRun": [
    {"command": "cargo test", "result": "passed", "summary": "14 passed, 0 failed"},
    {"command": "cargo build", "result": "passed", "summary": "build debug completado"},
    {"command": "rustfmt --check src/output.rs src/providers/claude.rs src/providers/codex.rs src/providers/mod.rs src/tui.rs", "result": "passed", "summary": "sin drift en archivos tocados"},
    {"command": "git diff --check", "result": "passed", "summary": "sin errores"}
  ],
  "validationOutput": [
    "R1-R10 PASS; error JSON-RPC id 2 ejercitado por seam determinista.",
    "Waybar idéntico con/sin créditos; caché antigua legible; altura TUI 5/4 según predicado.",
    "No hay archivos staged ni credenciales vivas en el diff."
  ],
  "residualRisks": [
    "Bajo: apply-progress conserva una mención histórica de 12 tests; la ejecución actual pasa 14.",
    "Drift de cargo fmt global preexistente en archivos baseline no tocados; gate global deliberadamente no bloqueante."
  ],
  "noStagedFiles": true,
  "diffSummary": "Campo opcional de créditos Codex, propagación JSON/TUI y pruebas de compatibilidad; Waybar sin cambios funcionales.",
  "reviewFindings": [
    "no blockers; no findings high/medium",
    "low: openspec/changes/codex-reset-credits/apply-progress.md — fila histórica de tarea 005 dice 12 tests; evidencia final actual dice 14"
  ],
  "manualNotes": "Verificación attested completada contra baseline f4a354f. No se modificó source durante verify."
}
```
