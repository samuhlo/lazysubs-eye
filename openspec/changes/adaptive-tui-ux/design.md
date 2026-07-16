# A. Proposal — UX adaptable de la TUI

## Decisión de idioma

La UI v1 permanece en español, coherente con la interfaz existente. README y
superficies de distribución pueden seguir en inglés; i18n queda fuera de v1.

## Intent

Hacer que la TUI funcione correctamente en cualquier terminal, incluyendo
pequeños (80×24), con `NO_COLOR=1`, sin UTF-8, y que sea usable para
daltonismo. Añadir: modelo de estados uniforme, scroll, guard RAII para
loading, indicador de loading por fuente y **decisión SPIKE de una lengua para
la UI v1** (español o inglés). La infraestructura bilingüe queda fuera de v1.

## Spec

### R1. Modelo de estados uniforme

Cada panel de la TUI **MUST** implementar el mismo modelo de estados:
`Loading`, `Ready`, `Empty`, `Partial`, `Unavailable`, `Stale`, `NotConfigured`.

Cada estado **MUST** tener:
- Un color/estilo visual coherente.
- Un mensaje de ayuda si es temporal.
- Una acción sugerido si es un error.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Loading | El worker de un panel está activo | Se renderiza | Se muestra Loading + nombre de la fuente |
| Ready | Datos válidos disponibles | Se renderiza | Panel normal con datos |
| Empty | No hay datos para hoy | Se renderiza | "Sin uso hoy" |
| Partial | Hay algunos datos pero incompletos | Se renderiza | Datos disponibles + mensaje de parcial |
| Unavailable | Fuente no configurada o no disponible | Se renderiza | Mensaje de no disponible |
| Stale | Datos antiguos (fuera de ttl) | Se renderiza | Datos + indicador de stale |
| NotConfigured | Provider apagado en config | Se renderiza | Mensaje de no configurado |

### R2. Scroll y terminal pequeño

La TUI **MUST** manejar terminales de 80×24 sin panics ni contenido cortado.
Si el contenido no cabe en el espacio disponible, **MUST** hacer scroll
vertical dentro del panel o del layout completo.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Terminal pequeño | 80×24 | Se abre TUI | Contenido visible sin panic; scroll disponible |
| Panel más alto que terminal | Un panel tiene 30 filas de contenido | Se renderiza | El panel se corta con scroll; otros paneles visibles |
| Resize | Terminal cambia de tamaño durante render | Se detecta | Layout se recalcula; no hay panic |

### R3. Guard RAII para loading

El estado `Loading` **MUST** manejarse con un guard RAII: cuando se inicia
un scan, se adquiere un guard que se libera automáticamente cuando el worker
termina (éxito, error, o timeout). Si el worker no responde en X segundos,
el guard expira y el estado vuelve al anterior con un mensaje de timeout.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Worker lento | Scan activo pero tarda más de 30s | Se muestra Loading por 30s | Tras 30s, se muestra timeout con opción de retry |
| Worker crash | El thread de scan muere sin enviar resultado | El guard detecta | Se libera; se muestra Unavailable |
| Timeout + retry | Se muestra timeout | Usuario presiona `r` | Se reinicia el scan con nuevo guard |

### R4. Loading por fuente

Cuando un panel está en Loading, **MUST** mostrarse qué fuente se está
consultando: "Cargando tokens Pi..." en lugar de solo "Cargando...".

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Pi scan activo | Pi tokens está cargando | Se renderiza | "Cargando tokens Pi..." |
| OpenCode scan activo | OpenCode tokens está cargando | Se renderiza | "Cargando tokens OpenCode..." |
| Todos loading | Varios scans activos | Se renderiza | "Cargando..." + lista de fuentes |

### R5. NO_COLOR respetado

Si `NO_COLOR=1` o `TERM=dumb`, la TUI **MUST** renders en modo monocromático
sin colores ANSI. Los paneles usan texto y atributos de terminal básicos
(underlined, bold para headers) que funcionan en cualquier terminal.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| NO_COLOR=1 | Variable de entorno presente | Se abre TUI | Sin colores ANSI; texto legible |
| TERM=dumb | Terminal no soporta ANSI | Se abre TUI | Modo monocromático |
| Forzar color | FORCE_COLOR=1 | Se abre TUI | Colores ANSI activos incluso con NO_COLOR |

### R6. Fallback ASCII

Si el terminal no soporta UTF-8, la TUI **MUST** usar caracteres ASCII
para los elementos decorativos (bordes, separadores). Los íconos (✳, ⚠️)
se reemplazan por sus equivalentes ASCII (*, !, etc.).

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| No UTF-8 | LANG=C; terminal no soporta UTF-8 | Se abre TUI | Caracteres ASCII en lugar de UTF-8 icons |
| UTF-8 | Terminal con soporte UTF-8 | Se abre TUI | UTF-8 icons |

### R7. Selección no basada solo en color

Los estados de los providers (Ready/Warning/Critical) **MUST** usar al menos
dos canales: color + carácter o posición. Un usuario daltónico **MUST** poder
distinguir los estados sin color.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Daltónico | Terminal en modo daltónico | Se renderiza | Ready = [✓], Warning = [!], Critical = [✗] con colores pero también caracteres |
| Color ciego | Usuario sin distinción rojo/verde | Se renderiza | Caracteres suficientes para distinguir |

### R8. Ayuda para abreviaturas

La TUI **MUST** mostrar ayuda cuando el usuario presiona `?` con:
- Lista de teclas y sus acciones.
- Significado de cada abreaviatura.
- Estados posibles de cada panel.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Presionar `?` | En cualquier vista | Se presiona `?` | Overlay con ayuda completa |
| Tecla desconocida | Se presiona una tecla sin binding | Se muestra | Mensaje: "Tecla no asignada" |

## Decisions

1. **Modelo de estados unificado**: un `PanelState` compartido conserva un
   payload específico por panel sin duplicar la semántica de Loading, Empty,
   Partial, Unavailable, Stale y NotConfigured.
2. **RAII guard para loading**: Rust ownership hace esto natural; el guard
   libera en Drop.
3. **UTF-8 fallback a ASCII**: detectar con `LANG` y `TERM`; no con heuristics
   de capacidad del terminal.
4. **Idioma de la UI v1 (SPIKE pendiente)**: task 0.1 elige español o inglés.
   No se implementa infraestructura bilingüe ni otros idiomas antes de v1.

## Success Criteria

- Terminal 80×24 sin panic ni contenido cortado.
- NO_COLOR desactiva colores.
- Loading muestra fuente específica.
- Daltonismo: estados distinguibles sin color.
- Help accesible con `?`.
