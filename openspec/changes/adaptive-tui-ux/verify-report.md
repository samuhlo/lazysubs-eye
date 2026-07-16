# Verify report — adaptive-tui-ux

- `cargo test --locked`: 160 unit tests + 3 integration tests, OK.
- `cargo clippy --all-targets -- -D warnings`: OK.
- `cargo fmt --check`: OK.
- Evidencia enfocada: `scroll_recorta_indica_y_respeta_offset_y_resize`,
  `loading_guard_libera_en_drop_y_el_timeout_recupera_el_estado`,
  `color_utf8_y_estados_tienen_fallbacks_deterministas`,
  `ctrl_c_es_interrupcion_incluso_con_help_abierto`.
- `cargo build --release --locked` y smoke del binario final: OK.
