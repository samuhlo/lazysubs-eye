# Apply progress — release-project-readiness

Estado: implementado para Beta/RC. CI exige fmt/Clippy/tests; RC+ añade RustSec.
Tag, Cargo y PKGBUILD se verifican; release x86_64-musl ejecuta smoke antes de
publicar, genera tarball y SHA-256. README, SECURITY, CONTRIBUTING, CHANGELOG,
feedback Beta y checklist están presentes.

Decisiones: aarch64 no soportado; no se afirma reproducibilidad bit a bit;
SBOM/attestation se deciden para Stable y cargo-deny durante RC. Promoción
Beta→RC es humana.
Siguiente paso: ejecutar el workflow sobre el próximo tag Beta/RC y archivar.
