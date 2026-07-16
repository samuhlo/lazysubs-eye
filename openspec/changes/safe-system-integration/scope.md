# Alcance: integración segura con el sistema

## SCOPE PACKET

```yaml
scope: Hacer que install y uninstall sean transaccionales, verificables con dry-run,
  con preflight checks, detección de ownership real, no interfieran con reglas
  manuales, y con rollback automático ante fallos.
change_name: safe-system-integration
budget_allocated:
  max_tokens: 18000
  max_reads: 25
  max_runtime_ms: 900000
webfetch: false
strict_tdd: true
artifact_language: es
```

## Resultado esperado

`lazysubs-eye install` modifica el sistema de forma verificable y reversible:
muestra qué va a hacer antes de hacerlo, no toca reglas manuales fuera de los
marcadores, hace rollback automático si algo falla, y permite probar en un
sandbox antes de tocar el sistema real.

## Hechos de partida

Estos hechos están verificados y no necesitan redescubrirse:

- lazysubs-eye es un binario Rust; usa `cargo test` y `strict_tdd: true`.
- `src/install.rs` ya existe con funciones `install()` y `uninstall()`.
- Install añade: módulo waybar, CSS, windowrule Hyprland, symlink ~/.local/bin.
- Uninstall elimina: módulo waybar, CSS, windowrule Hyprland, symlink.
- Ya existen marcadores `lazysubs-eye-begin` y `lazysubs-eye-end`.
- Los backups se crean con sufijo `.bak.<epoch>`.
- No hay preflight checks antes de modificar.
- No hay rollback automático.
- El windowrule usa ruta hardcodeada `~/.local/bin/lazysubs-eye`.
- No hay modo sandbox para probar sin tocar el sistema.

## Alcance funcional

### Preflight checks

- Verificar binario ejecutable existe.
- Verificar directorios de config escreibles.
- Verificar que waybar config existe.
- Reportar exactamente qué falló.

### Plan y dry-run

- `--dry-run` muestra plan completo de archivos, backups, comandos.
- Sin modificar nada.
- Compatible con sandbox.

### Resolución del binario durante install

- Usar `std::env::current_exe()` para resolver la ruta real.
- Los comandos de polling y click reciben la ruta absoluta resuelta.
- Si el binario se mueve, `doctor` lo detecta y `install` repara la integración.

### Ownership y marcadores

- Verificar que las reglas dentro de marcadores no fueron editadas manualmente.
- Warn sobre reglas manuales entre marcadores.
- No eliminar reglas fuera de marcadores.

### Uninstall seguro

- Solo elimina lo que está entre marcadores.
- Si faltan marcadores, error actionable.
- Backup de cada archivo antes de modificar.

### Transacción y rollback

- Crear backup antes de cada modificación.
- Si cualquier paso falla, rollback de todos los pasos anteriores.
- rollback usa los backups `.bak.<epoch>`.

### Sandbox mode

- Directorio temporal para probar sin tocar el sistema real.
- Compatibles con --dry-run.

## Criterios de aceptación

1. `install --dry-run` no modifica ningún archivo.
2. install que falla en paso 3 revierte los pasos 1-2.
3. uninstall no elimina reglas manuales fuera de marcadores.
4. La ruta del binario se resuelve durante `install` y se incrusta en los comandos.
5. Sandbox y dry-run tienen semánticas separadas y combinables.
6. Los tests usan temp dirs y mocks.

## Fuera de alcance

- Cambiar el formato de los módulos de waybar (el formato de salida es estable).
- Implementar install en distribuciones que no usan waybar (solo Linux con waybar).
- Hacer rollback de archivos que el usuario modificó manualmente entre install y rollback.
- Soporte para sway/wlroots u otros compositors.

## Investigación acotada para sdd-map

1. ¿Cuáles son exactamente las funciones de install y uninstall hoy?
2. ¿Cómo se insertan las reglas en waybar config y hyprland.conf?
3. ¿Hay tests existentes de install/uninstall?
4. ¿Qué tan complejo es el rollback?
5. ¿Hay forma de detectar si una línea fue editada vs añadida por install?

## Riesgos y controles

| Riesgo | Control requerido |
|--------|-------------------|
| Install con rollback incompleto | Tests de rollback; backup explícito |
| Uninstall que borra reglas manuales | Validación de marcadores; warn antes de borrar |
| Binario no encontrado o movido | Preflight durante install y diagnóstico posterior con `doctor` |
| Sandbox no refleja el comportamiento real | Tests paralelos sandbox + real |

## Condiciones para pasar a diseño

El mapa debe identificar: las funciones exactas de install y uninstall, el
formato de los marcadores, y el mecanismo de rollback actual. Si no hay
evidencia de rollback, el diseño debe proponer uno.
