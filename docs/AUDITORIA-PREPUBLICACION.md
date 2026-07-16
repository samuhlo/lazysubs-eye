# Auditoría de prepublicación — 2026-07-16

> **Documentos relacionados:**
> - [PLAN-PUBLICACION.md](PLAN-PUBLICACION.md) — plan de remediación con estrategia
>   de lanzamiento, workstreams, gates y matriz de dependencias.
> - [CHECKLIST-PUBLICACION.md](CHECKLIST-PUBLICACION.md) — gate operativo reutilizable
>   con evidencia concreta para cada fase (Beta, RC, Stable).

## Fecha y alcance

Esta auditoría corresponde a la revisión estática del código base de lazysubs-eye
realizada el **2026-07-16**. El análisis se limitó a lectura de código fuente,
configuración y estructura del repositorio. **No se ejecutaron tests,
benchmarks, ni herramientas de profiling durante esta auditoría.** Cualquier
medición de rendimiento presente en este documento es un objetivo de diseño,
no un resultado verificado.

---

## Veredicto

**Apto para beta técnica Linux x86_64 con entorno controlado (Waybar/Omarchy),
tras resolver los hallazgos P0. No está listo para distribución estable
generalista.**

Razones:
- La capa de instalación/desinstalación modifica reglas del sistema sin
  validación transaccional ni rollback completo.
- El backfill de historial es bloqueante y destructivo; puede sobrescribir
  datos existentes.
- La caché y el estado tienen condiciones de carrera documentadas; la
  concurrencia no ha sido probada.
- No existe un sistema de diagnóstico que permita distinguir un estado real
  (vacío, no configurado, stale) de un fallo silencioso.
- Los permisos de archivos en `~/.config/lazysubs-eye/`, `~/.cache/lazysubs-eye/` y
  `~/.local/state/lazysubs-eye/` no están protegidos explícitamente; archivos de
  historial pueden contener datos sensibles sin restricciones de acceso.

---

## Fortalezas

1. **Arquitectura modular**: providers, TUI, caché e ingesta local están
   separados por módulos. Claude, Pi y OpenCode ya mantienen flujos de uso
   local diferenciados.
2. **TDD estricto**: `openspec/config.yaml` declara `strict_tdd: true` y
   `cargo test` como runner; las pruebas de pi_tokens y opencode_tokens son
   deterministas y cubren bordes.
3. **Base de estados explícitos**: OpenCode ya distingue estados como `Ready`,
   `Empty`, `Unavailable`, `Stale` y `Loading`. El patrón puede unificarse en
   los demás paneles.
4. **Caché atómica**: `cache::atomic_save` usa write-to-temp + rename, lo
   que evita corrupciones ante fallos de escritura; el patrón ya está
   testado.
5. **Privacidad local por defecto**: no se llama a APIs remotas sin
   credenciales del usuario; todo se ejecuta en el entorno local.
6. **Base de distribución existente**: PKGBUILD con sha256, binario estático
   musl y tags que disparan releases automáticas. La reproducibilidad exacta
   todavía no está demostrada.

---

## Hallazgos priorizados

### P0 — debe resolverse antes de beta técnica

| ID | Evidencia | Descripción | Consecuencia |
|----|-----------|-------------|--------------|
| P0-1 | `src/cache.rs` + `src/install.rs` | **Permisos 0600/0700 ausentes.** `atomic_save` y `install.rs` escriben archivos de configuración, caché, historial y estado de notificaciones sin fijar permisos restrictivos. En sistemas compartidos, cualquier usuario puede leer tokens, API keys (en config) y el historial de gasto. | Exposición de credenciales y datos de uso. |
| P0-2 | `src/install.rs` `uninstall()` | **Uninstall destructivo.** La función elimina el módulo waybar, el CSS, la windowrule y el symlink sin pedir confirmación ni verificar ownership. Si otro proceso instaló reglas manuales en los mismos archivos, las borra sin mención. | Pérdida de configuración del usuario; conflictividad con instalaciones paralelas. |
| P0-3 | `src/install.rs` `install()` | **Install no transaccional.** Si el proceso falla tras modificar waybar config y antes de escribir el windowrule, el sistema queda en estado inconsistente: módulo activo sin windowrule, o viceversa. No hay rollback. | Inconsistencia del estado del sistema; la desinstalación manual deja residuos. |
| P0-4 | `src/cache.rs` `atomic_save()` + symlinks | **Escritura atómica sin política de symlinks.** El rename puede sustituir un symlink por un archivo regular y romper una configuración gestionada externamente. La aplicación no detecta ni explica ese cambio. | Pérdida de la relación con el destino y confusión de rutas. |
| P0-5 | `src/install.rs` | **Ruta del ejecutable inconsistente.** El polling usa una ruta absoluta, pero el click puede ejecutar el nombre `lazysubs-eye` y depender del PATH del servicio de Waybar. | El módulo puede mostrar datos y fallar al abrir la TUI. |
| P0-6 | `src/install.rs` | **Ownership no verificable.** `install` y `uninstall` usan marcadores `lazysubs-eye-begin/end` para delimitar su configuración, pero no verifican que sean las únicas modificaciones de lazysubs-eye. Un archivo sin marcador pero modificado por una versión anterior se pierde en uninstall. | Erosión silenciosa de configuración. |

### P1 — debe resolverse antes de RC

| ID | Evidencia | Descripción | Consecuencia |
|----|-----------|-------------|--------------|
| P1-1 | `src/main.rs` `check()` | **`--check` sin providers devuelve exit 0.** Si no hay ningún provider configurado, no se produce error y `--check` puede indicar "todo bien" cuando no monitoriza nada. | Scripts de monitorización dan falsos positivos. |
| P1-2 | `src/cache.rs` + estado | **Stale no diferenciado.** El estado `Stale` se usa tanto para "los datos son de hace más de `ttl`" como para "el último fetch falló pero hay datos previos". No hay forma de distinguirlos sin leer el código. | El usuario no sabe si sus datos están frescos o llevan horas fallando. |
| P1-3 | `.github/workflows/release.yml` | **Release sin quality gates.** El workflow compila, empaqueta y publica al taggear, pero no repite tests, formato, Clippy ni valida la coherencia tag-versión. | Se puede publicar un binario que no pasó los gates del commit etiquetado. |
| P1-4 | `src/main.rs` + `src/history.rs` | **Backfill parcial y en camino crítico.** En un cache miss, la ingesta puede ejecutar el backfill histórico antes de producir la salida CLI/Waybar. La TUI también puede iniciar trabajo histórico junto a los scans normales. | Primera salida lenta y lecturas duplicadas de gran volumen. |
| P1-5 | `src/tokens.rs` + `src/pi_tokens.rs` | **Fallos de scanners confundidos con vacío.** Claude y Pi pueden devolver una colección vacía ante errores de lectura. El usuario no distingue "sin uso" de "no se pudo leer". | Datos incompletos presentados como ausencia real. |
| P1-6 | `src/pi_tokens.rs` `add_entry()` | **Bootstrap Pi O(E²).** Cada entrada nueva vuelve a agregar las entradas vistas anteriormente, por lo que el coste crece cuadráticamente con el historial inicial. | Degradación severa con sesiones históricas grandes. |
| P1-7 | `src/providers/mod.rs` | **Providers secuenciales con timeouts acumulativos.** `collect_all` consulta providers y cuentas en serie. Un provider lento retrasa toda la respuesta. | La latencia total es la suma de las esperas. |
| P1-8 | `src/cache.rs` + `src/notify.rs` + `src/tui.rs` | **Concurrencia sin coordinación suficiente.** Procesos simultáneos pueden truncar/sobrescribir `status.json` o duplicar notificaciones; la TUI también puede lanzar scans históricos repetidos sin cancelación. | Estado corrupto, avisos duplicados y trabajo de I/O desperdiciado. |
| P1-9 | `src/config.rs` | **Validación semántica insuficiente.** La config se deserializa sin validar relaciones y rangos: thresholds invertidos, TTL/cooldown negativos, alias duplicados o `base_url` insegura. | Comportamiento inesperado y posible envío de credenciales por HTTP. |
| P1-10 | `src/tui.rs` | **TUI loading sin guard RAII.** El estado `Loading` se activa al iniciar un scan y se desactiva al recibir el resultado. Si el worker falla sin enviar update, el estado queda permanentemente en Loading hasta el siguiente scan. No hay guard RAII ni timeout. | Pantalla pillada en Loading para siempre. |
| P1-11 | `src/tui.rs` | **Scroll y terminal pequeño.** La TUI asume que hay espacio para todos los paneles. En terminales de 80×24 o con font grande, los paneles inferiores se salen. No hay scroll ni layout adaptativo. | Ilegibilidad en configuraciones comunes. |
| P1-12 | `src/tui.rs` | **NO_COLOR no respetado.** La TUI siempre pinta con colores ANSI. Si `NO_COLOR=1` o `TERM=dumb`, los paneles siguen usando estilos ANSI que no se muestran correctamente. | ilegibilidad en terminales sin color; violación de convención. |
| P1-13 | `src/tui.rs` | **Terminal restore fallido.** Si lazysubs-eye recibe una señal (SIGINT, SIGTERM) mientras el terminal está en modo alternativo, no se restaura correctamente. El usuario ve la pantalla corrupta. | Experiencia degradada tras interrupciones. |
| P1-14 | `README.md` + `docs/` | **Documentación obsoleta y onboarding incompleto.** Roadmap, ruta de OpenCode e instrucciones de instalación contradicen partes del código; falta troubleshooting para fallos comunes. | El usuario no puede instalar ni diagnosticar con confianza. |
| P1-15 | `src/install.rs` + packaging | **Compatibilidad no gobernada.** El producto se presenta como válido para cualquier Linux con Waybar, pero la distribución y pruebas actuales se concentran en Linux x86_64/Omarchy. | Expectativas incorrectas fuera del entorno principal. |
| P1-16 | `src/history.rs` + `src/cache.rs` | **Gobierno de datos stale insuficiente.** El historial guarda datos de días anteriores sin política de retención documentada más allá de `history_days`. No hay forma de saber qué días tienen backfill completo vs. parcial. | Datos potencialmente engañosos en vistas históricas. |

### P2 — debe resolverse antes de estable generalista

| ID | Evidencia | Descripción | Consecuencia |
|----|-----------|-------------|--------------|
| P2-1 | `src/tui.rs` | **Selección no basada solo en color.** Los estados de los providers usan color (verde/amarillo/rojo) sin otros indicadores. Usuarios daltónicos no distinguen el estado. | Accesibilidad rota. |
| P2-2 | `src/tui.rs` | **Carga de Claude/Pi/OpenCode no visible.** Cuando un panel está en Loading, no hay indicador de qué fuente se está consultando ni cuánto lleva. | El usuario no sabe si está colgado o funcionando. |
| P2-3 | `src/notify.rs` | **Notificaciones sin fallback visible.** Si `notify-send` no está disponible o falla, el resultado se ignora. | El usuario cree que las alertas funcionan cuando no es así. |
| P2-4 | `docs/**/*.md` | **Documentación desactualizada.** ARQUITECTURA.md y PLAN-PRODUCTO.md no reflejan los cambios de E1-E3. No hay CHANGELOG. | Incapacidad de auditar qué cambió entre versiones. |

---

## Presupuesto inicial de rendimiento

Estos son **objetivos a medir**, no resultados verificados:

| Métrica | Objetivo |
|---------|----------|
| Waybar cacheado (datos frescos) | < 10 ms desde que el módulo recibe la señal hasta que waybar recibe el json |
| Primer render TUI | < 150 ms desde que el usuario ejecuta `lazysubs-eye` hasta que ve la primera pantalla |
| Escaneo incremental local (Pi o OpenCode, sin cambios) | < 500 ms para un día con 100 sesiones y 0 bytes nuevos |
| Refresh completo (todos los providers + tokens) | Presupuesto global elegido después de medir el baseline; 5 s es solo el candidato inicial |
| Backfill de 30 días | Ejecutado fuera del camino crítico; no bloquea el primer render |
| Scroll en terminal 80×24 | Sin errores ni contenido cortado |

---

## Riesgos y limitaciones de esta auditoría

1. **Sesgo de confirmación**: el auditor conoce la arquitectura y puede haber
   inconscientemente ignorado hallazgos que contradigan su modelo mental.
2. **Alcance estático**: sin ejecución, no se pueden medir condiciones de
   carrera, tiempos de lectura reales, ni comportamiento bajo carga.
3. **Auditoría de caja negra parcial**: se leyó código pero no se verificó
   que la implementación corresponda al diseño documentado.
4. **Dependencias transitivas no auditadas**: `Cargo.lock` no fue inspeccionado
   para vulnerabilidades conocidas.
5. **Seguridad de credenciales no verificada**: no se auditó el manejo de
   `api_key` en config (en claro en TOML), ni el almacenamiento de tokens
   OAuth en `~/.config/claude/`.
6. **Concurrency model no probado**: los risks de condiciones de carrera
   identificados son teóricos; requieren tests de tensión para confirmarse.
7. **Compatibilidad con otras distribuciones no probada**: los fallbacks de
   install para non-Omarchy no se probaron en un sistema real sin Hyprland.

---

## Próximo paso

Esta auditoría alimenta `openspec/changes/release-project-readiness` y los
seis paquetes de estabilización P0/P1. El veredicto y los hallazgos deben
revisarse con datos ejecutados antes de cada gate de release.
