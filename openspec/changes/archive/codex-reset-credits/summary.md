## // 000. RESUMEN
Codex ahora conserva y expone sus créditos de reinicio disponibles (`rateLimitResetCredits.availableCount`) como dato opcional. La TUI los muestra únicamente en el panel Codex cuando el valor existe, sin alterar errores, caché, JSON histórico ni Waybar.

## // 001. QUÉ CAMBIÓ
- `src/providers/mod.rs`: `ProviderStatus` incorpora `reset_credits_available: Option<u64>` con serde opcional; `ProviderStatus::err` deja el dato ausente y se cubre la compatibilidad con cachés anteriores.
- `src/providers/codex.rs`: `RateLimitSnapshot` deserializa de forma tolerante el objeto anidado; `provider_status_from_rate_limits` propaga el contador sin cambiar plan ni ventanas; `rate_limits_result_from_response` conserva los errores JSON-RPC.
- `src/providers/claude.rs`: el estado de Claude declara explícitamente ausencia de créditos Codex.
- `src/tui.rs`: `codex_reset_credits_line` centraliza visibilidad y texto; `draw_provider` y `provider_height` usan el mismo seam para renderizar y reservar la fila `Créditos de reinicio disponibles: <n>`.
- `src/output.rs`: pruebas del JSON completo y de la invariancia de Waybar; no se añadió una rama de producción a Waybar.

## // 002. CÓMO FUNCIONA POR DENTRO
La respuesta JSON-RPC de `account/rateLimits/read` se deserializa mediante `Option` anidados: objeto ausente, campo ausente o `null` producen `None`, mientras que `0` sigue siendo `Some(0)`. El collector aplana ese dato en `ProviderStatus`, que serde guarda en caché y expone en `--json` como `reset_credits_available` solo cuando hay valor; los campos existentes no cambian.

La TUI calcula la línea opcional con un único predicado: provider `codex`, sin error y contador presente. Ese resultado alimenta tanto el contenido como la altura, evitando recortes. Waybar ignora deliberadamente el nuevo campo y conserva sus cuatro campos, selección, umbrales y errores.

## // 003. DECISIONES
- Se eligió `Option<u64>`: distingue ausencia de cero y rechaza valores que no son contadores válidos.
- Se usó serde aditivo con valor por defecto al leer y omisión al serializar `None`: las cachés antiguas siguen siendo legibles y el JSON nuevo no añade `null` innecesario.
- Los errores no conservan créditos: `ProviderStatus::err` impide mostrar datos obsoletos.
- No se cambió Waybar ni el transporte/ciclo de vida de Codex; la cobertura usa seams puros, sin credenciales ni procesos externos.

## // 004. VERIFICACIÓN
- TDD RED → GREEN → TRIANGULATE → REFACTOR documentado para collector, modelo/caché, TUI y salidas.
- `cargo test`: 14 tests pasados, 0 fallos.
- `cargo build`: pasado.
- `rustfmt --check` sobre los cinco archivos Rust tocados: pasado.
- `git diff --check`: pasado; no hay archivos staged.
- Cobertura de comportamiento verificada: parseo positivo/cero/ausente/null e inválidos, caché previa, JSON, Waybar, layout TUI y error JSON-RPC.

## // 005. PENDIENTE / RIESGOS
- Bajo: existe drift de formato global únicamente en archivos baseline no tocados (`src/cache.rs`, `src/main.rs`, `src/tokens.rs`); el formato acotado de los archivos modificados pasa.
- La mención histórica de 12 tests en `apply-progress.md` queda supersedida por la ejecución final verificada de 14 tests.
- Ningún bloqueador funcional pendiente; no se realizaron afirmaciones de commit, push o PR.
