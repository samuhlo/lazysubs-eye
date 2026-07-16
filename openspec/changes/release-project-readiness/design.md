# A. Proposal — preparación del proyecto para release

## Intent

Establecer los gates obligatorios de calidad para cada fase de release
(Beta, RC, Stable), asegurar la consistencia entre tag/Cargo/PKGBUILD,
documentar la estrategia de release, y crear/mantener la documentación
necesaria para usuarios y contribuyentes.

## Spec

### R1. Gates de calidad en CI

El workflow de CI **MUST** ejecutar y pasar antes de cualquier release:

1. `cargo fmt --check`: sin cambios de formatting pendientes.
2. `cargo clippy -- -D warnings`: sin warnings de linter.
3. `cargo test`: todos los tests en verde.
4. Gate de vulnerabilidades elegido y documentado (para RC y Stable).
5. Smoke tests del artefacto: `--version`, `--help`, modo JSON.

Si cualquier gate falla, el release **MUST NOT** proceeder.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Gate falla | clippy reporta warning | Se intenta release | CI falla; no se publica |
| Gate pasa | Todos los gates pasan | Se taggea | CI aprueba; se puede release |

### R2. Consistencia de versionado

El versionado **MUST** seguir Semantic Versioning (SemVer):
`MAJOR.MINOR.PATCH`.

- `MAJOR`: cambios incompatibles en la API o en el comportamiento.
- `MINOR`: funcionalidades nuevas compatibles hacia atrás.
- `PATCH`: correcciones de bugs compatibles.

La versión en `Cargo.toml`, el tag Git y el PKGBUILD **MUST** ser
consistentes. El release automation **MUST** verificar que el tag coincide
con la versión en Cargo.toml antes de publicar.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Tag no coincide | Tag v1.2.3 pero Cargo.toml dice 1.2.2 | Se ejecuta release | Error: "Tag v1.2.3 no coincide con Cargo.toml 1.2.2" |
| PKGBUILD desincronizado | Cargo.toml 1.2.3 pero PKGBUILD dice 1.2.2 | Se ejecuta release | Error: "PKGBUILD desincronizado" |

### R3. Smoke tests del artefacto

Antes de publicar, **MUST** ejecutarse smoke tests contra el binario real:

1. `--version` retorna 0 y muestra versión.
2. `--help` muestra ayuda.
3. Modo JSON sin providers produce JSON válido.
4. `--check` retorna código según estado real.
5. TUI arranca sin panic.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Smoke falla | `--version` retorna 127 (binario no ejecutable) | Se ejecuta smoke | CI falla; release se detiene |

### R4. Matriz de compatibilidad

**MUST** documentarse explícitamente qué arquitecturas y entornos se
soportan. La decisión inicial:

- **x86_64-unknown-linux-musl**: binario estático, probado.
- **x86_64-unknown-linux-gnu**: se documentará como soportado solo si se prueba.
- **aarch64**: decisión pendiente; no se anuncia ni genera hasta verificarlo.

Cambiar la matriz requiere una decisión explícita documentada.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Usuario en ARM | Solo existe artefacto x86_64 | Consulta compatibilidad | La matriz indica claramente que ARM no está soportado todavía |

### R5. Estrategia de supply-chain

**Decisiones graduables** — no todas son MUST universales:

1. **Gate de vulnerabilidades**: antes de RC se elige una herramienta y una
   política de advisories. No se presupone `cargo audit` ni `cargo deny`.
2. **SBOM**: se implementa si resulta viable y se adopta; si no, se registra
   procedencia y checksum del artefacto. No bloquea Beta.
3. **Reproducibilidad exacta**: solo es obligatoria si se adopta y define un entorno
   reproducible. Si no se define, registrar procedencia, checksum y smoke tests como
   evidencia alternativa.
4. **PKGBUILD linting con `namcap`**: práctica recomendada si se publica en AUR;
   no bloquea Beta en otros canales.
5. **`cargo deny`**: se evalúa durante RC con decisión explícita; no se asume adopción.
6. **Attestation (firma de binarios)**: decisión pendiente para Stable, no para RC.

**Requisitos obligatorios base** (toda fase): `cargo fmt`, `cargo clippy`,
`cargo test --locked`, consistencia tag/Cargo/PKGBUILD cuando aplique,
build explícito, smoke `--version`/`--help`, checksum del artefacto y evidencia
de release.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Vulnerabilidad conocida | El gate adoptado detecta un advisory bloqueante | Se ejecuta CI | CI falla según la política documentada |
| SBOM adoptado | La decisión de Stable exige SBOM | Se publica | El asset se genera y adjunta según el procedimiento verificado |

### R6. Documentación de release

Para cada release, **MUST** existir:

1. **README.md** actualizado: quickstart, compatibilidad, privacidad y
   troubleshooting; muestra textual en Beta y capturas reales antes de Stable.
2. **SECURITY.md**: cómo reportar vulnerabilidades, política de disclosure.
3. **CONTRIBUTING.md**: cómo contribuir, estándares de código, proceso de PR.
4. **CHANGELOG.md**: lista de cambios por versión (formato Keep a Changelog).

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| README desactualizado | Se libera con features nuevos no documentados | Se ejecuta CI | Gate de docs falla |
| SECURITY.md falta | No existe | Se intenta release stable | Gate de docs falla |

### R7. Beta feedback y gate a RC

Antes de avanzar de Beta a RC, **MUST** obtenerse feedback de beta testers
y resolverse los bugs bloqueantes. El período de beta **MUST** ser de al
menos 7 días.

El gate a RC incluye:
- [ ] Bugs bloqueantes de beta resueltos.
- [ ] Documentación actualizada con los cambios de beta.
- [ ] Los testers confirman que los problemas bloqueantes están resueltos.

| Escenario | Given | When | Then |
|-----------|-------|------|------|
| Bug bloqueante abierto | Se intenta promover a RC | Hay bugs con la etiqueta acordada para bloqueantes | El gate RC falla |

## Decisions

1. **SemVer como esquema de versiones**: `Cargo.toml` es la fuente de verdad y
   el workflow valida tag y PKGBUILD sin introducir otra herramienta obligatoria.
2. **Gate de vulnerabilidades para RC+**: la herramienta y severidad bloqueante
   se deciden antes de RC.
3. **SBOM como decisión graduada**: la decisión de incluirlo se toma antes de
   Stable; si no se adopta, se registra procedencia + checksum
   como evidencia alternativa.
4. **x86_64 como arquitectura inicial**: es el único artefacto obligatorio de
   Beta. La decisión de CI y distribución para aarch64 se toma durante
   RC (task 3.3). No se asume adopción automática.
5. **Attestation (SPIKE pendiente para Stable)**: firma de binarios es
   decisión pendiente para Stable — no se implementa en este change ni
   se asume adopción.
6. **cargo-deny (SPIKE pendiente para RC)**: se evalúa durante RC con
   decisión explícita — no se asume adopción automática en Beta.
7. **Reproducibilidad exacta (SPIKE-graduado)**: solo es obligatoria si se
   adopta y define un entorno reproducible. Si no se define, se registra
   procedencia, checksum y smoke tests como evidencia alternativa.

## Success Criteria

- CI gates pasan antes de cada release.
- Tag/Cargo/PKGBUILD son consistentes.
- Smoke tests verifican el artefacto.
- README, SECURITY.md, CONTRIBUTING.md, CHANGELOG.md existen y están actualizados.
- Beta feedback se Documenta y gating funciona.

## Decisiones de ejecución (2026-07-16)

- **Advisories RC+**: se adopta RustSec mediante `rustsec/audit-check@v2` en
  releases. Cualquier advisory no ignorado bloquea RC/Stable; una excepción
  exige issue público, alcance y fecha de retirada. Beta conserva fmt, Clippy y
  tests como gates obligatorios.
- **Procedencia**: GitHub Actions compila desde el commit etiquetado, publica el
  tarball y su SHA-256 y ejecuta smoke tests sobre ese mismo binario antes de
  publicar. No se declara reproducibilidad bit a bit porque no se ha congelado
  una imagen/toolchain completa.
- **SBOM**: diferido a la decisión de Stable. Para Beta/RC la evidencia aprobada
  es commit/tag + `Cargo.lock` + checksum + smoke.
- **aarch64**: no adoptado en este cambio; no hay runner/build verificable y no
  se anuncia soporte. La única plataforma publicada es x86_64 Linux musl.
- **Attestation**: pendiente para Stable; no bloquea RC.
- **cargo-deny**: evaluación separada durante RC; no se adopta implícitamente.
