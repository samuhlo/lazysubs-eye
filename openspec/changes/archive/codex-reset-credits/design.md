# Diseño: créditos de reinicio de Codex

## A. Proposal

### Intención

Exponer el contador opcional `rateLimitResetCredits.availableCount` que Codex ya entrega, conservarlo en el estado y la caché, incluirlo en `--json` cuando exista y mostrarlo como una fila adicional únicamente en el panel Codex de la TUI.

### Problema

El collector actual descarta ese contador al convertir la respuesta JSON-RPC en `ProviderStatus`. Como consecuencia, ni el estado serializable ni la TUI pueden informar cuántos créditos de reinicio quedan. La ampliación cruza un contrato compartido por caché, JSON, Waybar y TUI: un campo requerido rompería cachés antiguas; una condición de layout distinta de la condición de renderizado recortaría la nueva fila; y reutilizar el dato en Waybar cambiaría una salida que debe permanecer estable.

### Alcance

Incluye:

- Un contador `reset_credits_available: Option<u64>` en `ProviderStatus`.
- La lectura tolerante a ausencia y `null` de `rateLimits.rateLimitResetCredits.availableCount`.
- La propagación del contador desde el collector Codex al estado, la caché, `--json` y la TUI.
- Una fila TUI exacta: `Créditos de reinicio disponibles: <n>`.
- Compatibilidad de lectura con cachés anteriores y pruebas unitarias sin proceso Codex real.

No incluye:

- Cambios en el protocolo, timeout o ciclo de vida de `codex app-server`.
- Notificaciones, historial, costes, proyectos, providers nuevos o rediseño general de la TUI.
- Cambios en ventanas, planes, umbrales, configuración de Waybar o tratamiento existente de errores.
- Mostrar créditos en Claude o en la salida Waybar.

### Áreas afectadas

| Área | Responsabilidad prevista |
|---|---|
| `src/providers/mod.rs` | Definir el campo opcional y garantizar ausencia en estados de error. |
| `src/providers/codex.rs` | Deserializar el objeto anidado y trasladar el contador al estado Codex. |
| `src/providers/claude.rs` | Construir el estado Claude con el campo ausente. |
| `src/tui.rs` | Decidir visibilidad, texto y altura de la fila opcional con una condición compartida. |
| `src/output.rs` | Probar el JSON completo aditivo y que Waybar ignora el dato. |
| `src/cache.rs` | Mantener el contrato de serde usado al leer y guardar `Status`; no se prevé migración. |

### Riesgos

- Codex podría devolver un valor negativo, fraccionario, textual o mayor que `u64`; al no ser un contador válido, la deserialización fallará y seguirá la ruta normal de error del provider, sin convertirlo en cero ni ocultarlo como dato ausente.
- El nombre `reset_credits_available` pasa a ser contrato público de `--json` y caché.
- Si la altura y el renderizado no comparten exactamente el mismo predicado, la fila puede quedar recortada o dejar espacio vacío.
- Consumidores de `--json` que rechacen claves desconocidas podrían necesitar aceptar la nueva clave aditiva cuando Codex informe el dato.

### Rollback

Revertir conjuntamente el campo del modelo, la extracción Codex y la fila TUI. Las cachés escritas con la nueva clave seguirán siendo legibles por el modelo anterior porque serde ignora campos desconocidos por defecto; si fuese necesario, borrar `status.json` fuerza una recolección limpia. Waybar no requiere rollback al no cambiar su contrato.

### Criterios de éxito

- Un contador positivo y el valor `0` llegan sin alteración desde la respuesta Codex hasta estado, caché, `--json` y TUI.
- Ausencia o `null` en cualquier nivel opcional producen `None`, sin fila ni cero inventado.
- Una caché anterior sin la clave se deserializa correctamente.
- Un error Codex continúa generando el estado y presentación de error actuales, sin créditos.
- Waybar conserva exactamente sus cuatro campos y su lógica actual.
- La altura TUI aumenta una sola línea exactamente cuando la fila se renderiza.

## B. Spec

### R1. Contador positivo

El sistema **DEBE** conservar un `availableCount` entero no negativo como `reset_credits_available` sin modificar su valor.

**Escenario — contador positivo**

- **Dado** un resultado JSON-RPC Codex con `rateLimits.rateLimitResetCredits.availableCount: 3`
- **Cuando** el collector construye el `ProviderStatus` de Codex
- **Entonces** `reset_credits_available` es `Some(3)`.

### R2. Cero es un valor real

El sistema **DEBE** distinguir el valor `0` de la ausencia del dato.

**Escenario — cero créditos**

- **Dado** `availableCount: 0`
- **Cuando** se deserializa y propaga la respuesta
- **Entonces** el estado conserva `Some(0)` y la TUI muestra `Créditos de reinicio disponibles: 0`.

### R3. Campo ausente

El sistema **DEBE** interpretar como ausencia tanto un objeto `rateLimitResetCredits` omitido como un objeto presente sin `availableCount`; **NO DEBE** inventar `0`.

**Escenario — campo ausente**

- **Dado** `rateLimitResetCredits: {}` sin `availableCount`
- **Cuando** se recoge el estado Codex
- **Entonces** `reset_credits_available` es `None` y no se muestra la fila opcional.

### R4. Campo nulo

El sistema **DEBE** interpretar `rateLimitResetCredits: null` y `availableCount: null` como ausencia.

**Escenario — valor nulo**

- **Dado** `rateLimitResetCredits: {"availableCount": null}`
- **Cuando** se deserializa la respuesta
- **Entonces** el estado contiene `None` y no muestra un valor ficticio.

### R5. Caché anterior

El sistema **DEBE** deserializar estados de caché creados antes de existir `reset_credits_available`.

**Escenario — caché antigua sin la clave**

- **Dado** un JSON de `Status` válido y vigente cuyos providers no contienen `reset_credits_available`
- **Cuando** se deserializa mediante el mismo contrato usado por `cache::load`
- **Entonces** la carga tiene éxito y el campo de cada provider es `None`.

### R6. Errores del provider

El sistema **DEBE** conservar la ruta de errores existente y los estados de error **DEBEN** tener `reset_credits_available: None`.

**Escenario — error JSON-RPC de Codex**

- **Dado** que la respuesta con id `2` contiene `error` en lugar de `result`
- **Cuando** `collect_all` convierte el fallo del collector en `ProviderStatus::err`
- **Entonces** se conserva el mensaje de error, no hay créditos ni fila opcional, y las presentaciones TUI/Waybar siguen su rama de error actual.

### R7. Compatibilidad de `--json`

La salida completa `--json` **DEBE** incluir `reset_credits_available` como número cuando sea `Some`, **DEBE** omitir la clave cuando sea `None` y **NO DEBE** renombrar ni cambiar el tipo o significado de ninguna clave existente.

**Escenario — evolución JSON aditiva**

- **Dado** un `Status` Codex con `reset_credits_available: Some(3)`
- **Cuando** `output::pretty` lo serializa
- **Entonces** el provider contiene `"reset_credits_available": 3` y todos los campos históricos conservan sus nombres, tipos y valores.

### R8. Compatibilidad Waybar

La salida Waybar **NO DEBE** usar los créditos para calcular ni formar `text`, `tooltip`, `class` o `percentage`; **DEBE** conservar la selección `worst`, los umbrales 80/95 y el tratamiento de errores.

**Escenario — Waybar ignora créditos**

- **Dado** el mismo `Status` y las mismas ventanas, una vez con créditos y otra sin ellos
- **Cuando** ambos se serializan con `output::waybar`
- **Entonces** los dos JSON Waybar son idénticos y contienen únicamente `text`, `tooltip`, `class` y `percentage` con los valores actuales.

### R9. TUI con fila opcional

La TUI **DEBE** mostrar la fila solo si el provider tiene id `codex`, no tiene error y el contador es `Some`; la altura del panel **DEBE** crecer exactamente una línea bajo el mismo predicado.

**Escenario — layout con créditos**

- **Dado** un provider Codex correcto, con dos ventanas y `Some(3)`
- **Cuando** se calcula y dibuja su panel
- **Entonces** la altura es `2 ventanas + 1 fila + 2 bordes = 5` y aparece `Créditos de reinicio disponibles: 3` después de las ventanas.

### R10. TUI sin fila opcional

La TUI **DEBE** mantener el contenido y la altura anteriores cuando el contador sea `None`; además **NO DEBE** mostrar la fila en providers no Codex ni en estados con error.

**Escenario — layout sin créditos**

- **Dado** un provider Codex correcto, con dos ventanas y `None`
- **Cuando** se calcula y dibuja su panel
- **Entonces** la altura sigue siendo `2 ventanas + 2 bordes = 4` y no aparece ninguna fila de créditos.

## C. Decisions

### Flujo de datos exacto

```text
Respuesta JSON-RPC id=2
└─ result
   └─ rateLimits                         → RateLimitsResponse.rate_limits
      └─ rateLimitResetCredits           → Option<RateLimitResetCredits>
         └─ availableCount               → Option<u64>
                                            │
                                            ▼
                              collector Codex aplana ambos Option
                                            │
                                            ▼
                       ProviderStatus.reset_credits_available: Option<u64>
                              ├─ serde → cache/status.json
                              ├─ serde → --json (solo si Some)
                              ├─ output::waybar → ignorado
                              └─ TUI → predicado único → texto + altura
```

El esquema interno de Codex incorporará un tipo anidado con los renombres serde `rateLimitResetCredits` y `availableCount`. Objeto ausente, campo ausente y `null` se resuelven por composición de `Option`; un número no representable como `u64` es una respuesta inválida y devuelve `Err`. La conversión a `ProviderStatus` conservará sin cambios `primary`, `secondary` y `planType`.

### Serde y compatibilidad de caché

- El campo público será `reset_credits_available: Option<u64>`.
- Tendrá semántica serde equivalente a `default` al deserializar y `skip_serializing_if = "Option::is_none"` al serializar.
- Una caché antigua sin clave se carga como `None`; no habrá versión de esquema ni migración porque el cambio es aditivo y opcional.
- Una caché nueva conserva el número cuando existe y omite la clave cuando no existe. `ProviderStatus::err` y el estado Claude siempre lo inicializan a `None`.
- No se renombra ni elimina ningún miembro existente de `Status`, `ProviderStatus` o `Window`.

### Decisión de `--json` y Waybar

El valor **sí aparece en `--json` cuando existe**, con la clave `reset_credits_available`, porque `--json` es el volcado completo del estado y ocultarlo haría que estado/caché y API observable divergieran. Cuando es `None`, la clave se omite para mantener la salida previa sin ruido `null` y facilitar compatibilidad con estados que no lo soportan.

Waybar permanece sin cambios porque es una vista compacta de urgencia basada en ventanas, no un volcado completo. Incluir créditos en cualquiera de sus cuatro campos alteraría tooltips, estilos o scripts existentes y está fuera del alcance.

### Responsabilidades y límites

| Límite | Dueño | Responsabilidad |
|---|---|---|
| Protocolo Codex | `src/providers/codex.rs` | Deserializar y mapear; no modificar transporte ni lifecycle. |
| Contrato de estado | `src/providers/mod.rs` | Tipo, serde y ausencia en errores. |
| Providers sin soporte | `src/providers/claude.rs` | Declarar ausencia; no inferir créditos. |
| Persistencia | `src/cache.rs` + serde de `Status` | Guardar/cargar el mismo modelo sin migración específica. |
| JSON completo | `src/output.rs::pretty` | Exponer la adición cuando exista. |
| Waybar | `src/output.rs::waybar` | Ignorarla deliberadamente. |
| Presentación TUI | `src/tui.rs` | Formatear, decidir visibilidad y calcular layout. |

La TUI usará un único seam puro, por ejemplo `codex_reset_credits_line(&ProviderStatus) -> Option<String>`, tanto para decidir la fila como para renderizar su texto. `provider_height` derivará su incremento de ese mismo resultado. Así no se duplican los predicados `id == "codex"`, `error.is_none()` y `Some`.

### Estrategia TDD estricta

La fase de implementación seguirá ciclos RED → GREEN → TRIANGULATE → REFACTOR y ejecutará el comando configurado `cargo test` en cada transición relevante. No se requieren credenciales, red, `$HOME` real ni un proceso `codex app-server`.

Seams unitarios mínimos:

1. **Deserialización Codex privada:** fixtures `serde_json::Value` para positivo, cero, objeto/campo ausente y `null`; los tests viven como módulo hijo y acceden a los tipos privados.
2. **Transformación pura a estado:** separar solo si es necesario el mapeo `RateLimitSnapshot → ProviderStatus`, comprobando propagación sin lanzar Codex y preservación de plan/ventanas.
3. **Contrato serde:** `serde_json::from_str::<Status>` sobre una caché antigua y `output::pretty` sobre `Some`/`None`; esto prueba el mismo modelo que usa `cache::load` sin filesystem ni reloj.
4. **Error:** probar `ProviderStatus::err` con ausencia del contador; la rama JSON-RPC ya conserva su cortocircuito y no necesita proceso real.
5. **TUI:** probar el formateador/predicado puro y `provider_height` con Codex `Some`, Codex `None`, cero, provider no Codex y error. Al ser el mismo seam consumido por el render, no hace falta introducir un backend gráfico salvo que el texto no pueda aislarse.
6. **Waybar:** comparar la salida completa con y sin créditos para el mismo estado, incluyendo un caso de error.

### Alternativas rechazadas

- **Campo obligatorio con valor por defecto `0`:** confunde “Codex informó cero” con “Codex no informó el dato” y rompe la semántica requerida.
- **`Option<i64>` o `f64`:** admiten valores negativos o fraccionarios que no representan un contador disponible. `u64` expresa el dominio y falla de forma explícita ante respuestas inválidas.
- **Serializar siempre `null`:** es compatible con serde, pero modifica innecesariamente todos los providers y todos los JSON/cachés nuevos aunque no exista dato.
- **Ocultarlo también en `--json`:** contradice el papel de `--json` como estado completo y deja el nuevo dato accesible solo desde la TUI.
- **Añadirlo al tooltip Waybar:** cambia un contrato expresamente protegido y mezcla capacidad de reinicio con urgencia de ventanas.
- **Reservar siempre una fila TUI:** deja espacio vacío y altera el layout aunque el provider no entregue información.
- **Probar mediante un `codex app-server` real:** hace las pruebas lentas, dependientes de credenciales y no deterministas; los límites puros cubren el cambio.
- **Versionar o migrar la caché:** añade complejidad sin beneficio para un campo opcional que serde puede omitir y aceptar por defecto.

## D. Success Criteria

| Comprobación observable | Resultado aceptable |
|---|---|
| Codex devuelve `3` | Estado, caché y `--json` conservan `3`; TUI muestra la fila con `3`. |
| Codex devuelve `0` | Se conserva y muestra `0`; no se trata como ausencia. |
| Objeto/campo ausente o `null` | Estado `None`, clave omitida en JSON nuevo y ninguna fila TUI. |
| Caché previa sin clave | Se deserializa sin error y conserva todos los datos históricos. |
| Error del provider | Estado de error sin créditos; TUI y Waybar mantienen su rama actual. |
| JSON completo | Solo añade `reset_credits_available` numérico cuando existe; ninguna clave existente cambia. |
| Waybar | Salida byte a byte igual para estados equivalentes con y sin créditos. |
| Layout TUI | Suma una línea solo para Codex correcto con `Some`; sin fila conserva la altura previa. |
| Verificación Rust posterior | `cargo test` finaliza correctamente, sin credenciales Codex ni servicios externos. |

No se ejecutan tests ni builds en esta fase de diseño.
