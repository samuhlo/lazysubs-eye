# Apply progress — safe-system-integration

Estado: implementado. Preflight acumula errores antes de escribir; plan JSON y
dry-run no mutan; `--sandbox DIR` modifica sólo el árbol aislado y no recarga
servicios; el ejecutable se resuelve/canoniza; ownership manual aborta; install
y uninstall hacen backups únicos y rollback inverso automático.

Archivos: `src/install.rs`, `src/main.rs`, README. No se añadió dependencia.
Riesgo residual: los reload externos son best-effort y se informan al usuario;
la integridad de archivos ya está confirmada antes de ejecutarlos.
Siguiente paso: archivar el change después de integrar el conjunto.
