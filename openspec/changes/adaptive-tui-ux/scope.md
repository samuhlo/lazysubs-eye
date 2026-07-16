# Alcance: UX adaptable de la TUI

## SCOPE PACKET

```yaml
scope: Hacer que la TUI funcione en cualquier terminal (80x24, NO_COLOR,
  dumb, daltónico). Modelo de estados uniforme, scroll, guard RAII para
  loading, indicador de fuente, y decisión de idioma.
change_name: adaptive-tui-ux
budget_allocated:
  max_tokens: 15000
  max_reads: 20
  max_runtime_ms: 750000
webfetch: false
strict_tdd: true
artifact_language: es
```

## Resultado esperado

TUI usable en cualquier terminal: pequeños, sin color, sin UTF-8, daltónico.
Ayuda accesible.

## Hechos de partida

- TUI con ratatui.
- Estados existentes pero no uniformes: algunos paneles no tienen todos.
- No hay scroll; contenido se corta en terminals pequeños.
- Loading no indica qué fuente está cargando.
- NO_COLOR no respetado.
- No hay fallback ASCII.
- Estados basados en color.

## Criterios de aceptación

1. Terminal 80×24 funciona sin panic.
2. NO_COLOR=1 desactiva colores.
3. Loading muestra fuente.
4. Daltonismo: estados distinguibles sin color.
5. Help con `?`.

## Fuera de alcance

- Cambiar el layout de los paneles.
- Añadir nuevos paneles.
- Infraestructura bilingüe o i18n general. El SPIKE 0.1 elige una sola lengua
  para la UI v1: español o inglés.

## Decisión de idioma

La UI v1 usa español. No se introduce infraestructura i18n en este cambio.

Los estados usan dos canales: `[✓]` listo, `[!]` aviso/stale y `[✗]` fallo;
el fallback ASCII usa `[v]`, `[!]` y `[x]`. `Ctrl+C` se trata como salida aun
en raw mode y el guard RAII restaura el terminal en retorno, error o panic.
SIGTERM no se intercepta en v1: el alcance verificable es teclado/Ctrl+C y
unwind; añadir handlers asíncronos queda fuera para evitar lógica no signal-safe.
