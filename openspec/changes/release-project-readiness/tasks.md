# Tasks — release-project-readiness

status: complete
blocked_by: phase-dependent; see map.md

> Cada ciclo TDD cubre una capacidad funcional completa: RED (test que falla antes
> de implementar), GREEN (implementación mínima para pasar), TRIANGULATE (casos
> borde/adicionales), REFACTOR (limpieza, integración, revisión de comentarios).
> Al final se añaden fases separadas para documentación, suite completo, y
> preparación de apply-progress.md/verify-report.md durante ejecución.

**Decisiones graduables (no asumir adopción total):**
- SBOM: decisión para Stable, no bloquea Beta ni RC mientras no se adopte
- Attestation: pendiente para Stable, no para RC
- cargo-deny: se evalúa durante RC, decision explícita requerida
- aarch64 CI: decisión pendiente; no anunciar soporte antes de verificarlo

**Reproducibilidad:** Solo se exige checksum consistente si se define primero
un entorno reproducible. Si no, registrar procedencia, checksum y smoke tests.

## // 001. Gates de CI y calidad

- [x] 1.1 (RED) Test que verifica `.github/workflows/ci.yml` ejecuta
  cargo fmt --check, cargo clippy -- -D warnings, cargo test como mandatories
  - skills: `ein-discipline`, `github-workflow`
  - verify: `gh run list --workflow=ci.yml` muestra jobs pasando

- [x] 1.2 (GREEN) Asegurar que ci.yml tiene gates mandatories (no allow_failure);
  ampliar si falta alguno
  - skills: `ein-discipline`, `github-workflow`
  - verify: revisión de ci.yml; jobs son mandatories

- [x] 1.3 (TRIANGULATE) Elegir y probar el gate de vulnerabilidades para RC+;
  verificar también los smoke tests del artefacto
  - skills: `ein-discipline`, `github-workflow`
  - verify: ci.yml tiene condición para tags; smoke tests existen

- [x] 1.4 (REFACTOR) Documentar qué gates son obligatorios para cada fase
  (Beta: fmt/clippy/test; RC+: +audit)
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de comentarios en ci.yml; grep por allow_failure

## // 002. Consistencia de versionado

- [x] 2.1 (RED) Test que verifica script `scripts/verify-version.sh` falla
  si tag no coincide con Cargo.toml; falla si PKGBUILD desincronizado
  - skills: `ein-discipline`, `architecture`
  - verify: `./scripts/verify-version.sh` con tag y Cargo.toml sincronizados → 0;
    desincronizados → 1

- [x] 2.2 (GREEN) Crear `scripts/verify-version.sh` que extraiga versión de
  tag, Cargo.toml y PKGBUILD y las compare; integrar en release workflow
  - skills: `ein-discipline`, `github-workflow`
  - verify: script existe y funciona; release workflow lo ejecuta

- [x] 2.3 (TRIANGULATE) Tests: script no modifica nada (solo lectura);
  script maneja ausencia de PKGBUILD; script maneja tags sin v prefix
  - skills: `ein-discipline`, `architecture`
  - verify: `./scripts/verify-version.sh` sin PKGBUILD → 0 (PKGBUILD opcional)

- [x] 2.4 (REFACTOR) Documentar SemVer como source of truth; verificar que
  script no hardcodea versión
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de script; grep por version hardcoded

## // 003. Matriz de compatibilidad y aarch64

- [x] 3.1 (RED) Test que verifica README.md tiene sección Compatibility con
  x86_64 como soporte inicial y sin promesas de arquitecturas no probadas
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: README.md tiene sección; matriz documenta arquitectura y test status

- [x] 3.2 (GREEN) Documentar matriz en README.md: x86_64 probado; aarch64
  no soportado hasta que una decisión y un build verificable lo habiliten
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: README.md tiene sección Compatibility; matriz es clara

- [x] 3.3 (SPIKE) Evaluar aarch64 CI: si es factible, proponer job
  `cargo build --target aarch64-unknown-linux-gnu` (build only, no tests)
  - skills: `ein-discipline`, `github-workflow`
  - verify: si se adopta, CI genera el artefacto; si no, la matriz indica no soportado

- [x] 3.4 (DOCS) Documentar decisión de aarch64 en scope.md y map.md;
  verificar que matriz no promete más de lo que se prueba
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de scope.md y map.md; grep por aarch64 en decisiones

## // 004. Smoke tests del artefacto

- [x] 4.1 (RED) Test de integración que verifica smoke tests del binario:
  --version retorna 0, --help muestra ayuda, modo JSON produce JSON válido,
  --check retorna código según estado
  - skills: `ein-discipline`, `github-workflow`
  - verify: `cargo test --test cli_smoke` — rojos

- [x] 4.2 (GREEN) Añadir job en release workflow que: (a) primero build del
  binario con `cargo build --release`, (b) luego smoke tests contra el binario
  real con `./target/release/lazysubs-eye`
  - skills: `ein-discipline`, `github-workflow`
  - verify: smoke tests pasan en CI; workflow tiene build antes de smoke

- [x] 4.3 (TRIANGULATE) Smoke con binario de release descargado (si se usa
  release asset); smoke en CI antes de publish
  - skills: `ein-discipline`, `github-workflow`
  - verify: smoke pasa con binario descargado de release

- [x] 4.4 (REFACTOR) Documentar qué smoke tests se ejecutan; verificar que
  no modifican el sistema (solo read-only flags)
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de smoke tests; grep por write/modify en smoke

## // 005. Decisiones de supply chain

- [x] 5.1 (SPIKE) Definir threat model, política de advisories y controles
  graduables para Beta, RC y Stable; decidir SBOM y reproducibilidad por separado
  - skills: `ein-discipline`, `architecture`
  - verify: decisión registrada con alcance, severidades y evidencia requerida

- [x] 5.2 (RED) Crear tests que fallen al faltar los gates aprobados; si se
  adopta SBOM, verificar dependencias y versiones; si no, procedencia y checksum
  - skills: `ein-discipline`, `architecture`
  - verify: tests del gate elegido fallan antes de implementarlo

- [x] 5.3 (GREEN) Implementar únicamente los gates aprobados; si se adopta SBOM,
  crear su generador y adjuntarlo; si no, registrar procedencia y checksum
  - skills: `ein-discipline`, `architecture`
  - verify: si se adopta, el control genera evidencia válida; si no se adopta,
    la decisión y su alternativa quedan documentadas

- [x] 5.4 (TRIANGULATE) Si se adopta reproducibilidad, probar dos builds dentro
  del entorno definido; si no, triangular procedencia, checksum y smoke tests
  - skills: `ein-discipline`, `architecture`
  - verify: SI reproducible → dos builds producen igual sha256;
    SI no reproducible → verify-report documenta procedencia y smoke tests

- [x] 5.5 (REFACTOR) Documentar decisión de SBOM y reproducibilidad;
  cargo-deny como decisión separada para RC (no se asume su adopción)
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de design.md y scope.md; grep por SBOM en decisiones

## // 006. Documentación de release

- [x] 6.1 (RED) Test que verifica README.md tiene quickstart, compatibilidad,
  privacidad y troubleshooting; SECURITY.md, CONTRIBUTING.md,
  CHANGELOG.md existen
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: `ls SECURITY.md CONTRIBUTING.md CHANGELOG.md README.md`

- [x] 6.2 (GREEN) Crear/actualizar README.md, SECURITY.md, CONTRIBUTING.md,
  CHANGELOG.md según spec R6
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: todos los archivos existen con contenido no obsoleto

- [x] 6.3 (TRIANGULATE) CHANGELOG con formato Keep a Changelog; SECURITY.md
  con política de disclosure; CONTRIBUTING.md con setup y PR process
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: CHANGELOG tiene secciones Added/Changed/Fixed/Security;
    SECURITY.md tiene timeline de respuesta; CONTRIBUTING.md tiene code style

- [x] 6.4 (REFACTOR) Mantener la muestra textual para Beta y añadir capturas
  reales antes de Stable; evitar duplicación entre documentos
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de docs; grep por duplicación

## // 007. Beta feedback gating

- [x] 7.1 (RED) Test que verifica issue template de beta-feedback existe;
  label "beta-feedback" existe; proceso documentado en CONTRIBUTING.md
  - skills: `ein-discipline`, `github-workflow`
  - verify: `.github/ISSUE_TEMPLATE/beta-feedback.md` existe

- [x] 7.2 (GREEN) Crear issue template con campos para reportar bugs de beta;
  definir proceso en CONTRIBUTING.md; gate de promoción Beta → RC documentado
  - skills: `ein-discipline`, `github-workflow`
  - verify: template existe con campos útiles; proceso en CONTRIBUTING.md

- [x] 7.3 (TRIANGULATE) Gate de promoción: bugs bloqueantes resueltos,
  docs actualizadas, alpha/beta testers confirman; proceso manual (no automático)
  - skills: `ein-discipline`, `github-workflow`
  - verify: release checklist existe; alguien verifica antes de RC

- [x] 7.4 (REFACTOR) Documentar que promoción Beta → RC requiere decisión
  humana; verificar que no se automatiza sin verificación
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: revisión de CONTRIBUTING.md y release process

## // 008. Suite completo y preparación

- [x] 8.1 (VERIFY) Ejecutar suite completo: `cargo test --locked`
  - skills: `ein-discipline`
  - verify: todos los tests pasan

- [x] 8.2 (VERIFY) Quality gates finales: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
  - skills: `ein-discipline`
  - verify: sin errores

- [x] 8.3 (VERIFY) Preparar apply-progress.md con: tareas completadas, archivos tocados,
  decisiones técnicas (SBOM, reproducibilidad, aarch64, cargo-deny graduables),
  riesgos, siguiente paso
  - skills: `ein-discipline`
  - verify: apply-progress.md existe y está completo tras ejecución

- [x] 8.4 (VERIFY) Preparar verify-report.md con: comandos ejecutados, output relevante,
  evidencia de que cada gate pasó, verificación de docs
  - skills: `ein-discipline`
  - verify: verify-report.md existe y contiene evidencia tras verificación

## // 009. Attestation y cargo-deny (decisiones pendientes)

- [x] 9.1 (DOCS) Documentar en design.md y scope.md: attestation (firma de binarios)
  es decisión pendiente para Stable, no se implementa en este change
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: grep por attestation en design.md y scope.md → existe como pending

- [x] 9.2 (DOCS) Documentar en design.md y scope.md: cargo-deny se evaluará durante
  RC con decisión explícita; no se asume adopción automática
  - skills: `ein-discipline`, `cognitive-doc-design`
  - verify: grep por cargo-deny en design.md y scope.md → existe como deferred

- [x] 9.3 (VERIFY) Verificar que ninguna tarea asume adopción de attestation o cargo-deny
  - skills: `ein-discipline`
  - verify: grep por attestation o cargo-deny en tasks.md → son decisiones documentadas,
    no implementación
