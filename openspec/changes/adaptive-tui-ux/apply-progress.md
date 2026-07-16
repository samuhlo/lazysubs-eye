# Apply progress — adaptive-tui-ux

Estado: implementado. La UI v1 queda en español. `PanelState` cubre tokens e
historial; el layout recorta con indicador y offset; `LoadingGuard` y deadlines
liberan scans; NO_COLOR/FORCE_COLOR/TERM y UTF-8 tienen fallbacks; los estados
usan texto además de color; ayuda, Ctrl+C y unwind restauran el terminal.

Archivos principales: `src/tui.rs`, `scope.md`, `design.md`, `map.md`.
Decisiones: fallback `[v]/[!]/[x]`; Ctrl+C sí, handler SIGTERM fuera de v1.
Riesgo residual: la apariencia exacta depende del emulador, pero 80×24 y modo
monocromo están cubiertos por seams deterministas.
Siguiente paso: archivar el change después de integrar el conjunto.
