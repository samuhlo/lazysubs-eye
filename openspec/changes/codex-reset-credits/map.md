# Mapa: créditos de reinicio de Codex

status: complete
scope_status: bounded
change: codex-reset-credits
phase: map
skill_resolution: paths-injected
budget_source: scope.md

## Resultado

El cambio está delimitado a modelo serializable, collector Codex, constructores de estados, TUI y los límites de caché/salida. No hay pruebas Rust existentes. La siguiente fase debe diseñar primero los seams RED; esta fase no ejecutó builds ni tests.

## Flujo y archivos candidatos

```text
`account/rateLimits/read`.result
  .rateLimits.rateLimitResetCredits.availableCount
      ↓ src/providers/codex.rs
ProviderStatus.<contador opcional>
      ├─ serde: caché y --json
      ├─ src/tui.rs: fila exclusiva de Codex
      └─ src/output.rs::waybar: ignorado deliberadamente
```

| Orden | Archivo / símbolos | Superficie exacta |
|---|---|---|
| 1 | `src/providers/mod.rs`: `ProviderStatus`, `ProviderStatus::err` | Nuevo contador opcional en el modelo; errores lo dejan ausente. |
| 2 | `src/providers/codex.rs`: `RateLimitSnapshot`, tipos `Deserialize` internos, `collect` | Leer el objeto `rateLimitResetCredits` y su `availableCount`, y propagarlo. |
| 3 | `src/providers/claude.rs`: `collect` | Completar el nuevo miembro del literal con ausencia; Claude no aporta créditos Codex. |
| 4 | `src/tui.rs`: `provider_height`, `draw_provider` | Fila española condicional y altura coherente con ella. |
| 5 | `src/cache.rs`: `load`, `save`; `src/output.rs`: `pretty`, `waybar`; `src/main.rs` | Límites de compatibilidad, sin cambio funcional previsto salvo la serialización heredada de `Status`. |

`collect_all` en `src/providers/mod.rs` convierte errores de collector mediante `ProviderStatus::err`; esa ruta debe seguir produciendo un estado de error sin créditos. Los únicos literales de éxito localizados son `codex::collect` y `claude::collect`.

## Modelo, serde y caché

- `ProviderStatus` y `Status` derivan `Serialize`/`Deserialize`. `Status.providers` es tanto el contrato de caché como el que emite `--json`.
- Un nuevo `Option<T>` no renombra ni elimina claves existentes y permite que una clave ausente o JSON `null` se lea como `None`. Por ello los cachés de versiones anteriores sin la clave continúan deserializando, siempre que el miembro se mantenga opcional.
- Actualmente no se usa `skip_serializing_if`: en estados nuevos sin dato, el JSON/caché previsiblemente incluirá la nueva clave con `null`. Es aditivo, aunque no idéntico byte a byte; el diseño debe fijar el nombre público y tipo antes de implementar.
- `cache::load` hace directamente `serde_json::from_str::<Status>` y, ante fallo, usa una consulta fresca; `cache::save` serializa ese mismo estado. No hay migración ni capa intermedia.
- `ProviderStatus::err` debe inicializar ausencia para impedir que un fallo Codex presente datos obsoletos. Los literales de éxito de Codex y Claude deberán inicializar el nuevo campo por exhaustividad.

## JSON-RPC Codex y extracción

- `codex::collect` lanza `codex app-server`, envía `initialize`, `initialized` y la solicitud id `2` a `account/rateLimits/read`. Lee líneas JSON, selecciona únicamente id `2`, convierte `error` en `Err`, exige `result` y conserva timeout/limpieza con `KillOnDrop`.
- `RateLimitsResponse.rate_limits` se renombra desde `rateLimits`. `RateLimitSnapshot` contiene hoy `primary`, `secondary` y `plan_type` (`planType`); campos desconocidos son tolerados por serde.
- El punto de cambio es un miembro opcional de `RateLimitSnapshot` renombrado desde `rateLimitResetCredits`, respaldado por un tipo anidado que renombre `availableCount`. La extracción debe convertir objeto ausente, contador ausente y `null` a ausencia, nunca a `0`.
- Riesgo de implementación: el código mueve `primary` y `secondary` para formar ventanas y después usa `plan_type`. El nuevo dato debe extraerse/desestructurarse en un orden válido sin alterar plan ni ventanas.
- Desconocido: no hay fixture del protocolo que confirme rango/tipo de `availableCount`. El diseño debe elegir un entero adecuado para un contador y definir qué hacer con un valor no representable, sin confundir ausencia con cero.

## TUI y layout

- `App::draw` crea `Constraint::Length(provider_height(p))` para cada panel y entrega esa área a `draw_provider`; ambas funciones deben usar el mismo predicado.
- Hoy `provider_height` es `max(windows.len(), 1) + 2`; el dibujo retorna antes ante `error` y, sin error, dibuja una fila por ventana.
- La fila nueva solo es visible si `p.id` es Codex, no hay error y el contador es `Some`. Debe tener una etiqueta inequívoca en español que exprese créditos de reinicio disponibles.
- La altura debe sumar una línea solo con ese mismo predicado. Para cero ventanas y créditos presentes, el mínimo de una línea ya puede alojar la fila. Si la fila se integra en `Layout`, sus constraints deben incluirla.
- Riesgo: medir y renderizar con condiciones distintas recorta texto o deja espacio. Además, no se debe mostrar una fila en un estado error incluso si algún futuro productor la rellenase.

## Salidas a conservar

| Límite | Ruta | Decisión |
|---|---|---|
| JSON completo | `main.rs` → `output::pretty` → serialización de `Status` | Hereda intencionalmente el campo opcional como adición. Claves y tipos existentes no cambian. |
| Caché | `main.rs`/`tui.rs` → `cache::load`/`save` | Hereda el modelo; debe leer la versión previa sin clave. |
| Waybar | `main.rs` → `output::waybar` | Sin cambios: no usar créditos en `text`, `tooltip`, `class` ni `percentage`; conservar 80/95, `worst` y errores. |
| Error visual | `ProviderStatus::err`, `output::waybar`, retorno temprano de `draw_provider` | Conserva la presentación actual y ausencia de créditos. |

## Pruebas y seams TDD

La búsqueda de `#[test]`, `#[cfg(test)]` y `mod tests` en todos los Rust no encontró coincidencias.

1. **Parser Codex:** probar desde un módulo hijo, o tras extraer una transformación pura, el `Deserialize`/mapeo sin iniciar `app-server`: presente, objeto ausente, campo ausente y `null`; añadir el caso fuera de rango según la política de tipo.
2. **Serde/caché:** deserializar un `Status` JSON anterior sin la clave y comprobar ausencia; serializar el nuevo y comprobar que las claves históricas conservan valor/tipo. Así se prueba el mismo contrato de caché sin `$HOME`, TTL o reloj.
3. **Propagación:** probar la transformación snapshot→`ProviderStatus`; Codex conserva el valor, Claude y `ProviderStatus::err` quedan ausentes, sin red/proceso/filesystem.
4. **TUI:** `provider_height` es el seam mínimo actual para combinaciones Codex/no-Codex, `Some`/`None`, ventanas y error. Para afirmar la etiqueta/contenido hará falta `ratatui::TestBackend` o extraer predicado/formateador; elegir el menor refactor.
5. **Waybar:** crear un `Status` con créditos y verificar que el JSON de `waybar` conserva los cuatro campos y los valores previos; incluir estado de error.

## Riesgos y siguiente fase

- La respuesta puede omitir o anular cualquiera de los dos niveles del objeto.
- El nombre/tipo del campo es un contrato público simultáneo para caché y JSON.
- La altura manual de Ratatui exige una única condición de visibilidad.
- No existen fixtures ni tests previos.

Siguiente fase recomendada: `sdd-design`, para fijar nombre/tipo, predicado único de TUI y casos RED antes de implementar.

## Ledger

ledger:
  reads:
    - { path: "/home/samuhlo/.pi/agent/skills/local/ein-discipline/SKILL.md", lines: 101, estimated_tokens: 1300 }
    - { path: "/home/samuhlo/.pi/agent/skills/local/cognitive-doc-design/SKILL.md", lines: 48, estimated_tokens: 620 }
    - { path: "openspec/changes/codex-reset-credits/scope.md", lines: 70, estimated_tokens: 1250 }
    - { path: "Cargo.toml", lines: 15, estimated_tokens: 120 }
    - { path: "src/**/*.rs (grep: ProviderStatus)", lines: 10, estimated_tokens: 280 }
    - { path: "src/**/*.rs (grep: rateLimits|rate_limit|rateLimit)", lines: 5, estimated_tokens: 180 }
    - { path: "src/**/*.rs (grep: Waybar|waybar|serde_json|to_string|from_str)", lines: 13, estimated_tokens: 370 }
    - { path: "src/**/*.rs (grep: height|Height|Rect|render)", lines: 18, estimated_tokens: 510 }
    - { path: "src/providers/mod.rs", lines: 69, estimated_tokens: 560 }
    - { path: "src/providers/codex.rs", lines: 139, estimated_tokens: 1210 }
    - { path: "src/tui.rs", lines: 274, estimated_tokens: 2350 }
    - { path: "src/cache.rs", lines: 26, estimated_tokens: 180 }
    - { path: "src/output.rs", lines: 79, estimated_tokens: 630 }
    - { path: ". (grep: test attributes/modules in *.rs)", lines: 0, estimated_tokens: 0 }
    - { path: "src/providers/claude.rs", lines: 126, estimated_tokens: 1060 }
    - { path: "src/main.rs", lines: 74, estimated_tokens: 620 }
    - { path: ". (repeat grep: test attributes/modules in *.rs)", lines: 0, estimated_tokens: 0 }
    - { path: ". (grep: ProviderStatus literals in *.rs)", lines: 4, estimated_tokens: 110 }
  webfetch_used: false
  budget_consumed: { tokens: 10740, reads: 18 }
