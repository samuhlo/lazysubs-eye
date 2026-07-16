# Verify report — observable-health-diagnostics

- Caja negra `tests/cli_check.rs`: ready=0, warning/stale=1, critical=2,
  unavailable/config inválida=3; nunca 4.
- `tests/cli_smoke.rs`: version, help y doctor JSON válidos.
- Unit tests de códigos, saneado, config y reporte doctor: OK.
- Suite global: 160 unit tests + 3 integration tests, OK.
- Gates fmt/Clippy, build release y smoke del binario: OK.
