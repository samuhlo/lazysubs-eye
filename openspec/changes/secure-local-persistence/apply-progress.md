# Apply progress — secure-local-persistence

Estado: implementado. `atomic_save` crea temp 0600, sincroniza fichero y
directorio, publica por rename y rechaza cualquier symlink en la cadena. Los
directorios privados son 0700. `flock` coordina status.json con timeout y
verificación de inode antes del commit para detectar `LockLost`.

Archivos: `src/cache.rs`, `src/file_lock.rs`, `src/history.rs`, `Cargo.toml`.
Decisiones: flock Unix advisory por destino; status usa timeout 100 ms; datos
no compartidos conservan atomicidad sin lock. Riesgo documentado: no Windows.
Siguiente paso: archivar el change después de integrar el conjunto.
