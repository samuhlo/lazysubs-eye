# Tasks — codex-reset-credits

status: ready
blocked_by: none

## // 001. Extracción tolerante y propagación del collector Codex

- [ ] 1.1 (RED) Añadir pruebas unitarias en el módulo de tests de `src/providers/codex.rs` para deserializar `RateLimitSnapshot` y cubrir `availableCount` positivo, `0`, objeto/campo ausente y `null`; incluir negativo, fraccionario, textual y un entero mayor que `u64` como valores inválidos que producen error.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Fija el dominio `Option<u64>` y evita confundir ausencia con cero antes de tocar el esquema JSON-RPC.
  - learn: En serde, `Option` distingue `Some(0)` de `None`, mientras que un valor fuera del dominio debe fallar explícitamente.
  - architecture: El seam de protocolo permanece privado en `src/providers/codex.rs`; los tests no lanzan `codex app-server`.
  - avoid: No probar el transporte real ni convertir entradas inválidas en `None` o `0`.
  - verify: `cargo test providers::codex`

- [ ] 1.2 (GREEN) Extender los tipos `Deserialize` internos de `src/providers/codex.rs` (`RateLimitSnapshot` y tipo anidado) con `rateLimitResetCredits.availableCount`, y mapearlo al construir el `ProviderStatus` de `collect` sin alterar `primary`, `secondary` ni `planType`.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Hace que el collector conserve el dato que Codex ya devuelve.
  - learn: Los renombres serde aíslan el contrato camelCase del protocolo del modelo Rust.
  - architecture: `codex.rs` es dueño de deserializar y aplanar ambos niveles opcionales; no se cambia el lifecycle ni el timeout.
  - avoid: No modificar el protocolo, la selección de id `2`, las ventanas ni el manejo de errores.
  - verify: `cargo test providers::codex`

- [ ] 1.3 (TRIANGULATE) Añadir casos de transformación snapshot→`ProviderStatus` que comprueben preservación de plan/ventanas, `Some(3)`, `Some(0)` y `None`; comprobar también que una respuesta inválida sigue la ruta de error existente.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Demuestra que la extracción no se limita a deserializar, sino que llega al estado compartido sin regresiones.
  - learn: Una prueba de transformación pura aporta evidencia sin credenciales, red ni filesystem.
  - architecture: La conversión pura queda separada solo hasta donde sea necesario, manteniendo `collect` como orquestador del proceso.
  - avoid: No duplicar fixtures en pruebas de integración ni relajar el tipo para aceptar datos ambiguos.
  - verify: `cargo test providers::codex`

- [ ] 1.4 (REFACTOR) Consolidar nombres, fixtures y helpers del seam JSON-RPC en `src/providers/codex.rs`, eliminando duplicación sin ampliar el alcance.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Deja la prueba de protocolo legible y mantiene un único camino de mapeo.
  - learn: Refactorizar después de la triangulación preserva la evidencia GREEN antes de simplificar.
  - architecture: La representación del protocolo sigue encapsulada en Codex y no se filtra a TUI ni Waybar.
  - avoid: No hacer un refactor general de providers ni introducir fixtures externos.
  - verify: `cargo test providers::codex`

## // 002. Modelo compartido, estados sin soporte y compatibilidad serde/cache

- [ ] 2.1 (RED) Añadir pruebas en `src/providers/mod.rs` y/o `src/cache.rs` para `ProviderStatus::err`, estado Claude y deserialización de un `Status` de caché antigua sin `reset_credits_available`; exigir `None` y conservación de campos históricos.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Protege la compatibilidad de caché y evita créditos obsoletos en errores o providers sin soporte.
  - learn: Un campo opcional con default implícito permite leer JSON anterior sin migración.
  - architecture: `ProviderStatus` define el contrato serializable; `cache::load` continúa usando `serde_json::from_str::<Status>` directamente.
  - avoid: No añadir versión de esquema, migración ni acceso a `$HOME` en tests unitarios.
  - verify: `cargo test`

- [ ] 2.2 (GREEN) Incorporar `reset_credits_available: Option<u64>` en `src/providers/mod.rs` con `#[serde(default, skip_serializing_if = "Option::is_none")]`; inicializarlo ausente en `ProviderStatus::err` y en el literal de éxito de `src/providers/claude.rs`.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Define el contrato público aditivo y asegura que estados sin dato no inventen cero ni emitan ruido `null`.
  - learn: `skip_serializing_if` mantiene la salida previa cuando el dato no existe, mientras `default` acepta cachés antiguas.
  - architecture: El modelo compartido posee la semántica; Claude y errores declaran explícitamente que no producen créditos Codex.
  - avoid: No renombrar/eliminar miembros existentes ni usar `i64`, `f64` o un valor obligatorio.
  - verify: `cargo test`

- [ ] 2.3 (TRIANGULATE) Verificar serialización/deserialización del mismo `Status` usado por `src/cache.rs`: clave numérica para `Some(3)` y `Some(0)`, clave omitida para `None`, y todos los campos históricos sin cambio de tipo o valor.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Cierra la prueba del contrato de caché sin depender de reloj, TTL o filesystem.
  - learn: Probar el modelo serde directamente cubre tanto caché como el volcado completo que lo reutiliza.
  - architecture: No se crea una capa de compatibilidad paralela; serde sigue siendo el único límite de persistencia.
  - avoid: No aceptar `null` como sustituto de cero ni cambiar nombres públicos existentes.
  - verify: `cargo test`

- [ ] 2.4 (REFACTOR) Revisar exhaustividad de todos los literales de `ProviderStatus` localizados en `src/providers/codex.rs`, `src/providers/claude.rs` y `src/providers/mod.rs`, dejando el inicializado opcional consistente.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Evita constructores incompletos y mantiene una convención clara para futuros providers.
  - learn: La exhaustividad del compilador es una red de seguridad útil al ampliar modelos Rust.
  - architecture: Solo los productores de estado escriben el campo; `cache.rs` no recibe lógica específica de créditos.
  - avoid: No modificar `src/cache.rs` salvo lo necesario para conservar su contrato actual.
  - verify: `cargo test`

## // 003. Fila condicional y altura coherente en la TUI

- [ ] 3.1 (RED) Añadir pruebas de `src/tui.rs` para el seam puro de visibilidad/formateo y `provider_height`: Codex correcto con `Some(3)`, Codex con `Some(0)`, Codex con `None`, dos ventanas; además provider no Codex y estado con error sin fila ni incremento.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Fija que renderizado y layout comparten exactamente la condición del diseño.
  - learn: Un predicado/formateador puro permite probar TUI sin depender de un backend gráfico.
  - architecture: `src/tui.rs` será dueño de una función compartida (por ejemplo `codex_reset_credits_line`) consumida por dibujo y altura.
  - avoid: No reservar siempre una fila ni introducir un rediseño general de Ratatui.
  - verify: `cargo test tui::`

- [ ] 3.2 (GREEN) Implementar en `src/tui.rs` el seam compartido que devuelve `Créditos de reinicio disponibles: <n>` solo para Codex sin error y con `Some`; usarlo en `provider_height` y `draw_provider`, sumando exactamente una línea.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Hace visible el dato y evita que la nueva fila quede recortada o deje espacio vacío.
  - learn: Medir y dibujar desde el mismo resultado elimina divergencias de layout.
  - architecture: La presentación conoce el texto español; el modelo y collector no conocen detalles de Ratatui.
  - avoid: No mostrar créditos en Claude, en estados de error ni en otras vistas.
  - verify: `cargo test tui::`

- [ ] 3.3 (TRIANGULATE) Confirmar con pruebas de altura y contenido que dos ventanas con `Some(3)` miden 5 líneas totales (ventanas + fila + bordes), mientras `None` conserva 4 y el cero se dibuja como valor real.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Aporta evidencia observable de criterios R2, R9 y R10 sin probar el ciclo completo de la aplicación.
  - learn: Los casos de borde de layout deben verificar simultáneamente condición y resultado, no solo una rama.
  - architecture: El incremento de altura deriva del mismo seam que produce la línea, no de un segundo predicado.
  - avoid: No introducir snapshots gráficos amplios si una prueba pura cubre el contrato.
  - verify: `cargo test tui::`

- [ ] 3.4 (REFACTOR) Simplificar helpers y colocación de la fila en `draw_provider`, conservando la condición única y el orden posterior a las ventanas.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Mantiene la TUI acotada y fácil de revisar después de demostrar el comportamiento.
  - learn: El refactor de presentación debe preservar primero la evidencia de altura y texto.
  - architecture: `provider_height` y `draw_provider` siguen dependiendo del mismo seam de visibilidad.
  - avoid: No tocar constraints de paneles ajenos ni cambiar etiquetas existentes.
  - verify: `cargo test tui::`

## // 004. JSON aditivo y Waybar sin cambios funcionales

- [ ] 4.1 (RED) Añadir pruebas en `src/output.rs` que comparen `output::pretty`/serialización completa con créditos `Some(3)` y `Some(0)` frente a la ausencia, y que comparen `waybar` para el mismo estado con y sin créditos, incluyendo error.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Protege los contratos de salida que comparten el modelo pero tienen responsabilidades distintas.
  - learn: JSON completo puede crecer de forma aditiva; Waybar debe permanecer idéntico para estados equivalentes.
  - architecture: `output::pretty` expone el campo del modelo; `output::waybar` ignora deliberadamente créditos y conserva `text`, `tooltip`, `class`, `percentage`, `worst` y umbrales.
  - avoid: No probar ni implementar cambios en configuración de Waybar o Hyprland.
  - verify: `cargo test output::`

- [ ] 4.2 (GREEN) Ajustar únicamente serialización/configuración necesaria para que `src/output.rs::pretty` incluya `reset_credits_available` solo cuando sea `Some`, sin modificar `src/output.rs::waybar` ni sus umbrales, selección o error.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Cumple el contrato JSON aditivo y mantiene la salida compacta estable.
  - learn: Compartir un modelo no obliga a que todas sus vistas consuman cada campo.
  - architecture: `ProviderStatus` controla la presencia serde; Waybar conserva su límite de cuatro campos.
  - avoid: No agregar créditos a texto, tooltip, clase o porcentaje.
  - verify: `cargo test output::`

- [ ] 4.3 (TRIANGULATE) Verificar que las claves históricas de `pretty` conservan nombre/tipo/valor y que la salida Waybar es idéntica con y sin créditos, tanto en estado normal como de error.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Cierra la evidencia de no regresión en consumidores externos y en la ruta de errores.
  - learn: Comparar salidas equivalentes es más fuerte que afirmar únicamente que una clave nueva existe.
  - architecture: La compatibilidad se valida en `output.rs`, no mediante cambios en consumidores.
  - avoid: No relajar aserciones a “contiene al menos” para Waybar si el contrato exige igualdad.
  - verify: `cargo test output::`

- [ ] 4.4 (REFACTOR) Mantener el diff de `src/output.rs`, `src/cache.rs` y `src/main.rs` limitado a pruebas/compatibilidad serde estrictamente necesarias, retirando imports y helpers no usados.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Preserva el alcance y evita que una ampliación aditiva se convierta en una refactorización de salidas.
  - learn: Un work unit revisable agrupa comportamiento y pruebas, no archivos por separado.
  - architecture: `main.rs` conserva el enrutamiento actual; cache y output reutilizan `Status` sin capas nuevas.
  - avoid: No cambiar el ciclo de carga/guardado ni el formato de Waybar.
  - verify: `cargo test output:: providers:: cache::`

## // 005. Regresión enfocada, evidencia final y limpieza

- [ ] 5.1 (RED) Añadir una matriz final de regresión unitaria que cubra positivo/cero/ausente/null/inválido, caché antigua, error Codex, fila/altura TUI, JSON aditivo y Waybar estable usando los seams existentes.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Reúne los criterios de aceptación sin crear una prueba end-to-end dependiente de servicios externos.
  - learn: Una regresión enfocada confirma las fronteras críticas después de implementar cada unidad.
  - architecture: Las pruebas permanecen junto a sus módulos (`codex.rs`, `mod.rs`/`cache.rs`, `tui.rs`, `output.rs`).
  - avoid: No añadir tests de proceso real, red, `$HOME` ni snapshots globales.
  - verify: `cargo test`

- [ ] 5.2 (GREEN) Corregir cualquier fallo de la matriz final manteniendo el comportamiento existente de ventanas, plan, errores y Waybar fuera del nuevo campo.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Convierte la evidencia RED en una implementación integrada y acotada.
  - learn: La corrección debe seguir el contrato aprobado, no ampliar el alcance ante una prueba conveniente.
  - architecture: Cada fix queda en el dueño de su frontera, sin compartir lógica incidental entre collector y presentación.
  - avoid: No ocultar fallos eliminando casos ni cambiar expectativas aprobadas.
  - verify: `cargo test`

- [ ] 5.3 (TRIANGULATE) Ejecutar la verificación completa y revisar que el campo aparece solo cuando corresponde, que `Some(0)` se conserva y que los casos de error no muestran créditos obsoletos.
  - skills: `ein-discipline`, `work-unit-commits`
  - why: Define la evidencia de terminado para toda la slice antes de limpiar.
  - learn: Triangular al final significa comprobar el contrato entre límites, no solo cada función aislada.
  - architecture: La ruta completa sigue siendo respuesta Codex → estado → TUI/JSON, con Waybar deliberadamente separado.
  - avoid: No ejecutar comandos distintos del runner configurado ni incorporar verificación manual fuera del alcance.
  - verify: `cargo test`

- [ ] 5.4 (REFACTOR) Limpiar nombres, comentarios de motivo y duplicación de fixtures; confirmar que no quedan cambios fuera de `src/providers/mod.rs`, `src/providers/codex.rs`, `src/providers/claude.rs`, `src/tui.rs`, `src/cache.rs`, `src/output.rs` o `src/main.rs` necesarios para este contrato.
  - skills: `ein-discipline`, `work-unit-commits`, `cognitive-doc-design`
  - why: Deja una implementación pequeña, explicable y lista para revisión.
  - learn: La limpieza final protege la comprensión del cambio y el límite de revisión.
  - architecture: Se conserva la separación entre protocolo, estado, persistencia, vistas y errores.
  - avoid: No hacer refactors amplios ni añadir documentación o configuración no solicitada.
  - verify: `cargo test`
