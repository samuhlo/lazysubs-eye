# Verify report — release-project-readiness

- `scripts/verify-version.sh v0.13.0`: OK; `v9.9.9`: falla esperado.
- `cargo build --release --locked`: OK; binario x86_64 de 6,25 MiB
  (6.551.608 bytes).
- `scripts/smoke-release.sh target/release/lazysubs-eye`: OK.
- Docs y `.github/ISSUE_TEMPLATE/beta-feedback.md`: presentes.
- Workflow: target MUSL instalado por `dtolnay/rust-toolchain`, build locked,
  checksum, smoke pre-publicación y RustSec RC+ con permiso de issues.
- El toolchain local no incluye el target MUSL; el intento local falla antes de
  compilar por ausencia de `core/std`. La instalación y build quedan como gate
  obligatorio del runner de release, no como afirmación de build local.
- Gates fmt/Clippy/tests (160 unit + 3 integración): OK.
