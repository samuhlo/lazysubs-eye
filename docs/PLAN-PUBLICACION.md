# Plan de publicación — lazysubs-eye

## Objetivo

Convertir lazysubs-eye en un producto distribuible de forma reversible y verificable,
orientado inicialmente a usuarios técnicos en Linux con waybar/Omarchy, y
posiblemente a usuarios de otras distribuciones con waybar sin Omarchy.

## Definición de "publicable"

Un paquete o release de lazysubs-eye se considera **publicable** cuando cumple
simultáneamente:

1. **Scope aceptado**: todas las tareas del paquete OpenSpec correspondiente
   están marcadas `[x]` tras `sdd-verify`.
2. **Tests en verde**: `cargo test --locked` pasa sin errores.
3. **Errores manejados**: no hay `unwrap()`, `expect()` con mensajes genéricos,
   ni `panic!()` ante inputs válidos.
4. **Métricas observables**: cada feature tiene al menos una métrica
   observable (tiempo, tamaño de índice o número de lecturas) registrada.
5. **Documentación completa**: cada archivo tocado tiene comentarios que explican
   el _por qué_, no el _qué_.
6. **Verify report existe**: `sdd-verify` genera un informe con los comandos
   ejecutados y los resultados.
7. **Sin warnings**: `cargo clippy` y `cargo fmt --check` pasan.

---

## Estrategia de lanzamiento

```
Oleada 0 (baseline y decisiones)
     ↓
Oleada 1A (P0 en paralelo — sin dependencias entre sí)
    ├── secure-local-persistence
    └── safe-system-integration
     ↓
Oleada 1B (P0 — depende de secure-local-persistence)
    └── reliable-history-ingestion
     ↓
Oleada 2 (P0/P1 — diseño puede empezar antes; implementación tras contratos base)
    ├── observable-health-diagnostics
    └── runtime-performance
     ↓
Oleada 3 (P1 — depende de modelo uniforme de reliable/observable)
    └── adaptive-tui-ux
     ↓
Oleada 4 (release readiness en cada fase; cierra último)
    └── release-project-readiness (P0 → P1 → P2)
```

### Fases

1. **Beta técnica Linux x86_64**: solo waybar + Omarchy. Solo para usuarios
   que puedan reportar bugs en español. Sin anuncios públicos.
2. **RC (Release Candidate)**: amplia a otras configuraciones Linux con waybar
   sin Omarchy. Anuncio limitado. Bugs bloqueantes resueltos antes de stable.
3. **Stable**: público general. Anuncio en Reddit/GitHub. Paquetes AUR
   verificados.

---

## Principios

1. **Cambios pequeños**: cada paquete OpenSpec debe caber en una PR manejable.
   Si un paquete supera ~400 LOC de producción, dividirlo.
2. **Strict TDD**: RED antes de GREEN. Ningún código de producción sin test
   que lo justifique.
3. **No afirmar verificaciones no ejecutadas**: no decir "tests pasan" si no
   se ejecutaron en la sesión actual.
4. **Seguridad local por defecto**: archivos de config, caché e historial con
   permisos 0600/0700. Ningún secreto en variables de entorno ni logs.
5. **Estados explícitos**: cada panel de la TUI y cada salida de `--check`
   debe distinguir unambiguously entre empty, unavailable, stale y error.
6. **Rendimiento medido**: los objetivos de rendimiento son ceiling budgets,
   no valores aspiracionales. Se miden; si no se cumplen, se abre un issue.
7. **Instalación reversible**: `install` y `uninstall` deben ser idempotentes
   y seguras. Nunca perder configuración del usuario.

---

## Workstreams y sus paquetes OpenSpec

| Paquete | Prioridad | Dependencias | Gate de entrada | Resultado esperado |
|---------|-----------|--------------|-----------------|-------------------|
| `secure-local-persistence` | P0 | Ninguna | Oleada 1A | Archivos con 0600/0700; atomic_save seguro ante symlinks; locks multiproceso |
| `safe-system-integration` | P0 | Ninguna | Oleada 1A | Install/uninstall transaccional; dry-run; preflight checks; ownership verificable |
| `reliable-history-ingestion` | P0/P1 | secure-local-persistence | Oleadas 1A (P0), 1B (P1) | Backfill no bloqueante; estados explícitos; idempotencia; cutoff; transacciones |
| `observable-health-diagnostics` | P0/P1 | reliable-history-ingestion + secure-local-persistence | Oleada 2 | `--check` semántico; `doctor`; errores accionables; validación de config; exit codes documentados |
| `runtime-performance` | P1 | reliable-history-ingestion | Oleada 2 | Eliminación de O(E²); providers paralelos; budgets de rendimiento; streaming |
| `adaptive-tui-ux` | P1 | reliable-history-ingestion + observable-health-diagnostics | Oleada 3 | NO_COLOR; terminal pequeño; scroll; loading guards; ayuda para abreviaturas; decisión de idioma |
| `release-project-readiness` | P0/P1/P2 | Ninguna (P0); secure + observable (P1); todo P0+P1 (P2) | Oleadas 1A/2/3/4 | Quality gates en CI; tags/Cargo/PKGBUILD consistentes; README/screenshots; SECURITY.md; CHANGELOG |

---

## Diagrama de dependencias entre paquetes

```text
secure-local-persistence ──→ reliable-history-ingestion ──┬─→ runtime-performance
                                                          └─→ observable-health-diagnostics
                                                                      │
                                                                      ▼
                                                               adaptive-tui-ux

safe-system-integration se ejecuta en paralelo con secure-local-persistence.
release-project-readiness diseña los gates desde el inicio, pero cada gate se
cierra únicamente cuando sus paquetes requeridos tienen verify report aprobado.
```

---

## Gates exactos por fase

### Gate Beta técnica

- [ ] `cargo test --locked` pasa completo
- [ ] `cargo fmt --check` sin cambios pendientes
- [ ] `cargo clippy --all-targets -- -D warnings` sin warnings
- [ ] Todos los P0 de `secure-local-persistence` resueltos
- [ ] Todos los P0 de `safe-system-integration` resueltos
- [ ] `reliable-history-ingestion` secciones 001-003 verificadas: estados, transacciones y backfill fuera del camino crítico
- [ ] `observable-health-diagnostics` secciones 001-002 verificadas: config y contrato `--check`
- [ ] `release-project-readiness` tiene gates base, versión coherente y smoke tests del artefacto
- [ ] PKGBUILD actualizado y probado en sandbox
- [ ] README refleja el estado actual (E1-E3)
- [ ] El binario pasa `file` y `ldd` correctamente

### Gate RC

- [ ] Todos los gate de Beta completados
- [ ] Todas las tareas restantes de `reliable-history-ingestion` verificadas
- [ ] Todas las tareas restantes de `observable-health-diagnostics` verificadas
- [ ] Todos los P1 de `runtime-performance` resueltos
- [ ] `release-project-readiness` P1: tags/Cargo/PKGBUILD consistentes; smoke tests en CI
- [ ] Se ha elegido un gate de vulnerabilidades para dependencias y el gate adoptado pasa
- [ ] Security.md publicado en el repo
- [ ] CHANGELOG strategy documentada

### Gate Stable

- [ ] Todos los gate de RC completados
- [ ] Todos los P1 de `adaptive-tui-ux` resueltos
- [ ] `release-project-readiness` P2: todos los gates cerrados; soak test completado
- [ ] Documentación completa en docs/: [AUDITORIA-PREPUBLICACION.md](AUDITORIA-PREPUBLICACION.md) hace referencia a este plan
- [ ] Matriz de compatibilidad publicada (x86_64/aarch64 como decisión explícita)
- [ ] Decisiones de supply-chain documentadas (SBOM, attestation si aplica)

---

## Matriz de pruebas

| Tipo | Unit | Integration | Manual sandbox | Performance | Packaging/Release | Security/Supply chain |
|------|------|-------------|----------------|-------------|-------------------|-----------------------|
| secure-local-persistence | `cargo test` (archivos 0600/0700) | — | chmod 0777 en temp; verificar denied | — | Verificar permisos en binario final | — |
| safe-system-integration | — | install/uninstall en temp con marcadores | Sandbox XDG/HOME | — | PKGBUILD lint si se publica en AUR | — |
| reliable-history-ingestion | `cargo test` (backfill, cutoff, idempotencia) | — | 30 días en temp; verificar no bloquea | Timing de backfill con 30 días | — | — |
| observable-health-diagnostics | `cargo test` (estados, exit codes) | — | `--check` con config rota; `doctor` | — | — | — |
| runtime-performance | `cargo test` (O(E²) eliminado) | — | — | Benchmarks con fixture grande; budgets | — | — |
| adaptive-tui-ux | `cargo test` (render, estados, NO_COLOR) | — | Terminal 80×24; NO_COLOR=1 | — | — | — |
| release-project-readiness | — | Smoke tests CLI | — | — | CI gates completos; artefacto real | Gate de vulnerabilidades elegido; revisión PKGBUILD si aplica |

---

## Definition of Done común

Para cualquier paquete antes de marcar sus tareas como completadas:

1. **Scope aceptado**: todas las tareas tienen evidencia de que fueron
   implementadas según `scope.md`.
2. **Tests RED/GREEN**: hay tests que fallan antes de la implementación y
   pasan después; los tests nuevos están marcados.
3. **Errores manejados**: no hay unwrap/expect/panic ante inputs válidos.
4. **Documentación**: cada archivo tocado tiene comentarios de por qué.
5. **Métricas**: los objetivos de rendimiento tienen al menos una medición
   en el apply progress.
6. **Verify report**: `sdd-verify` se ejecutó y generó evidencia.
7. **Sin warnings**: `cargo clippy` y `cargo fmt --check` pasan.
8. **Rollback documentado**: el map.md de cada paquete incluye el plan de
   rollback verificado.

---

## Registro de decisiones pendientes

Las siguientes decisiones requieren una resolución explícita antes del
gate RC. Se incluye una recomendación inicial, pero deben documentarse
como decisiones explícitas en el changelog de cada fase.

### D1 — Idioma de la UI

- **Decisión tomada**: README público en inglés; documentación interna en español.
  El idioma de la UI (TUI, notificaciones) **sigue abierto** hasta la fase de
  UX. La decisión debe elegir una plataforma: español o inglés, no i18n general.
  Otros idiomas quedan fuera de v1.
- **Riesgo si no se decide**: inconsistencia entre mensajes de UI y logs.

### D2 — Soporte de arquitecturas

- **Recomendación**: x86_64 como primaria; aarch64 como secundaria con
  build explícito en CI. Documentar como matriz, no como "soporta todo".
- **Riesgo si no se decide**: expectativas de usuario rotas en ARM.

### D3 — Notificaciones: default on/off

- **Recomendación**: default `on` con cooldown de 30 min. El usuario ya
  configuró notify-send; el valor por defecto debe ser útil.
- **Riesgo si no se decide**: usuarios nuevos no reciben alertas; usuarios
  avanzados deshabilitan manualmente.

### D4 — Política de symlinks

- **Decisión pendiente de diseño**: spike requerido para evaluar opciones:
  (a) rechazar symlinks como destino, (b) seguir symlinks solo dentro de rutas
  internas ($XDG_CACHE_HOME/lazysubs-eye, $XDG_CONFIG_HOME/lazysubs-eye), (c) otra
  restricción. El diseño debe especificar opciones seguras y criterios de
  aceptación antes de implementar.
- **Riesgo si no se decide**: P0-4 puede repetirse con otra versión.

### D5 — Source of truth: ruta de OpenCode en config

- **Recomendación**: permitir `OPENCODE_DB` como variable de entorno, no
  como clave de config. Mantener la separación entre config de producto
  y variables de entorno del provider.
- **Riesgo si no se decide**: confusión entre credenciales y configuración.

### D6 — Política de datos stale

- **Recomendación**: documentar que "stale" significa "datos de hace más
  de `ttl`" y que un stale state no implica error. El usuario debe poder
  distinguir stale de error sin leer código.
- **Riesgo si no se decide**: P1-2 permanece sin resolver en UX.

### D7 — Presupuesto de refresh global

- **Recomendación inicial**: budget máximo de 5s para el refresh completo
  con cancelabilidad. Si un provider supera su timeout individual, se cancela
  y se degrada a stale. **Tratar como objetivo a validar con baseline**, no
  como compromiso definitivo. Medir baseline real antes de afianzar el budget.
- **Riesgo si no se decide**: P1-7 no tiene métrica objetivo.

### D8 — Formato de logs

- **Recomendación**: mantener logs mínimos en stdout/stderr solo con
  `--verbose`. No escribir a archivos de log. Documentar qué se emite
  con cada flag.
- **Riesgo si no se decide**: usuarios confunden output de debug con error.

### D9 — Nivel de verificación de la cadena de suministro

- **Decisiones como gates graduables**: antes de RC se debe elegir una política
  de vulnerabilidades y su herramienta, por ejemplo `cargo audit` o
  `cargo deny`. SBOM, attestation y reproducibilidad exacta son decisiones
  independientes para Stable. Ninguna herramienta queda aprobada por este plan.
- **Riesgo si no se decide**: usuarios no pueden auditar dependencias.

---

## Riesgos del programa y mitigaciones

| Riesgo | Probabilidad | Impacto | Mitigación |
|--------|--------------|---------|------------|
| Scope creep: se agregan features antes de v1 | Alta | Alta | Definir explícitamente qué NO está en cada paquete; sección "no objetivos" en cada scope.md |
| Tests insuficientes para condiciones de carrera | Media | Alta | Añadir tasks de concurrency testing en runtime-performance; stress tests en sandbox |
| Dependencias inseguras en Rust | Baja | Alta | Elegir antes de RC una política de advisories y bloquear los hallazgos según esa política |
| Backward compatibility rota por cambios en config | Media | Alta | Versionar config con `config_version`; nunca romper esquemas sin migración |
| Usuario no puede roll back tras install | Media | Alta | `uninstall` debe restaurar exactamente el estado previo; verificar con marcadores |
| Binario no portable entre distribuciones | Media | Media | CI con binario estático musl; PKGBUILD con dependencies mínimas; probar en Arch y Debian |
| Documentación desactualizada tras cada release | Alta | Media | CHANGELOG strategy definida en release-project-readiness; docs en el mismo PR que el feature |

---

## Cómo usar y completar los paquetes OpenSpec

Cada paquete OpenSpec contiene cuatro archivos:

1. **`design.md`**: Proposal → Spec (requisitos R1...) → Escenarios
   Dado/Cuando/Entonces → Decisions → Success Criteria. Se genera en la
   fase `sdd-design` y no se modifica durante apply.
2. **`scope.md`**: incluye/no-incluye, budget si aplica, acceptance criteria
   con checkboxes sin marcar. Se genera en `sdd-design` y se refina en
   `sdd-map`.
3. **`tasks.md`**: tareas numeradas con dependencias internas, ciclos
   RED/GREEN/TRIANGULATE/REFACTOR, y tasks de verificación/documentación.
   Se genera en `sdd-design` y se marca `[x]` durante `sdd-apply`.
4. **`map.md`**: flujo afectado, archivos concretos, puntos de riesgo,
   estrategia de pruebas, rollback y dependencias con otros paquetes. Se
   genera en `sdd-map`.

**Flujo típico**:
```
sdd-explore → sdd-design → sdd-map → sdd-apply (tareas) → sdd-verify
```

Durante `sdd-apply`, se ejecutan las tareas de `tasks.md` en orden, se marca
cada una como `[x]` al completar, y se genera `apply-progress.md` con el
progreso. Durante `sdd-verify`, se genera `verify-report.md`. Al terminar,
el paquete se mueve a `openspec/changes/archive/`.

**Convenciones de etiquetas de tareas:** las capacidades funcionales requieren
el ciclo completo RED/GREEN/TRIANGULATE/REFACTOR. Las tareas no funcionales
(investigación, documentación, verificación) usan etiquetas SPIKE, DOCS o
VERIFY según corresponda — es correcto que un spike o la documentación no
finjan ser un ciclo TDD.

**Nota sobre convenciones de archivos SDD**: este proyecto usa la convención
donde `apply-progress.md` se genera durante apply (no se crea manualmente).
Los archivos `summary.md` y `verify-report.md` también se generan durante
las fases de verify, no antes. No se debe crear `apply.md` manualmente.

---

## Qué NO agregar antes de v1

Estos elementos están fuera del scope de todas las oleadas hasta que lazysubs-eye
alcance stable v1:

1. **GUI gráfica** (Electron, GTK, Qt): la TUI es el producto. Cualquier GUI
   es un proyecto separado.
2. **Integración con la nube** (sync de config, historial remoto,
   dashboards web): contradice la privacidad local por defecto.
3. **Telemetría o analytics**: ningún dato sale del equipo del usuario sin
   consentimiento explícito.
4. **Providers adicionales** (Gemini, Groq, etc.) salvo contribuciones
   aisladas de terceros que mantengan el mismo nivel de calidad.
5. **Rewriting async completo** (Tokio, async-std): el modelo actual con
   threads y channels es suficiente para el caso de uso; async añade
   complejidad sin beneficio presente.

Si un item de esta lista parece necesario, abrir un issue separado
y esperar a v2.
