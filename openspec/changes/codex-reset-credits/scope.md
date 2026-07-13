# Alcance: créditos de reinicio de Codex

## SCOPE PACKET

```yaml
scope: >-
  Exponer los créditos de reinicio que Codex ya devuelve en
  `account/rateLimits/read`, representarlos como estado opcional y mostrarlos
  en el panel de Codex de la TUI sin romper JSON, Waybar, caché ni el manejo
  de errores de providers.
budget_allocated:
  max_tokens: 15000
  max_reads: 30
  max_runtime_ms: 120000
```

## Resultado esperado

El estado de un provider podrá conservar opcionalmente el número disponible en `rateLimitResetCredits.availableCount`. El collector de Codex trasladará ese valor cuando exista y la TUI lo mostrará únicamente en el panel de Codex con una etiqueta clara en español.

Si el objeto, el contador o su valor no están presentes —o llegan como `null`—, la aplicación mantendrá la presentación y el comportamiento actuales, sin inventar un cero ni mostrar información engañosa.

## Incluido

- Ampliar el modelo compartido de estado con créditos de reinicio opcionales.
- Deserializar de forma tolerante `rateLimits.rateLimitResetCredits.availableCount` desde la respuesta JSON-RPC de Codex.
- Propagar el valor al `ProviderStatus` de Codex sin alterar la conversión existente de plan y ventanas.
- Reservar una fila o contenido equivalente en el panel de Codex solo cuando haya un valor real, con una etiqueta española inequívoca.
- Ajustar el cálculo de altura del panel cuando la fila opcional sea visible.
- Mantener la compatibilidad de serialización y deserialización: no renombrar ni eliminar campos existentes y aceptar cachés anteriores que no contengan el nuevo dato.
- Mantener intactos el texto, tooltip, clase y porcentaje de Waybar respecto a los créditos de reinicio.
- Conservar la ruta actual de errores: un fallo del collector continúa produciendo un `ProviderStatus` de error y la TUI/Waybar lo presentan como hasta ahora.

## No incluido

- Notificaciones.
- Historial o sparklines.
- Providers nuevos.
- Desgloses de costes o proyectos.
- Configuración de Waybar o Hyprland.
- Rediseño general de la TUI.
- Cambios en los umbrales de rate limit.
- Cambios al protocolo JSON-RPC, al timeout o al ciclo de vida de `codex app-server`.

## Criterios de aceptación

- [ ] El modelo de estado representa los créditos de reinicio de Codex como un valor opcional.
- [ ] El collector extrae `rateLimitResetCredits.availableCount` cuando está presente y es válido.
- [ ] El panel de Codex muestra el valor disponible con una etiqueta clara en español.
- [ ] Un objeto ausente, un campo ausente o un valor `null` se traducen a ausencia de dato y no generan una fila ni un valor ficticio.
- [ ] Los JSON y cachés anteriores, sin el nuevo campo, siguen pudiendo deserializarse.
- [ ] La evolución del JSON es solo aditiva; las claves existentes y su significado no cambian.
- [ ] La salida Waybar no incorpora los créditos ni cambia sus umbrales, selección de ventana, clase o tratamiento de errores.
- [ ] Los errores del provider Codex conservan el comportamiento existente y no muestran créditos obsoletos o engañosos.

## Superficies previstas

| Área | Archivo candidato | Motivo |
|---|---|---|
| Estado compartido | `src/providers/mod.rs` | `ProviderStatus` concentra el estado serializable y la construcción de errores. |
| Collector Codex | `src/providers/codex.rs` | Contiene el esquema interno de `account/rateLimits/read` y construye el estado de Codex. |
| TUI | `src/tui.rs` | Dibuja cada provider y calcula la altura de su panel. |
| Compatibilidad | `src/cache.rs`, `src/output.rs` | Son límites que deben conservar la lectura de caché, el JSON y Waybar; no implican cambios obligatorios. |

## Guardas de compatibilidad

1. **Dato ausente:** ausencia y `null` significan `None`; no deben convertirse en `0`.
2. **Caché previa:** el nuevo campo opcional no puede impedir leer estados guardados por versiones anteriores.
3. **JSON:** la ampliación será aditiva y no modificará tipos ni nombres existentes.
4. **Waybar:** los créditos no participarán en `text`, `tooltip`, `class` ni `percentage`.
5. **Errores:** el nuevo dato no altera el cortocircuito visual ni la construcción de estados de error.

## Riesgos acotados

- La respuesta de Codex puede omitir el objeto completo o devolverlo como `null`; el mapeo debe tolerar ambas variantes.
- Una fila condicional puede quedar recortada si la altura del panel no usa la misma condición que el renderizado.
- Añadir estado serializable puede invalidar cachés antiguas si el campo no conserva semántica opcional durante la deserialización.

## Verificación diferida

Las fases posteriores deberán cubrir al menos los casos de contador presente, objeto/campo ausente, valor `null`, caché previa, salida JSON aditiva, Waybar sin cambios funcionales y provider en error. Esta fase no ejecuta builds ni tests.
