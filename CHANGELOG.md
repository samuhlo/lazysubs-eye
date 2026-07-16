# Changelog

All notable changes are documented here.

## Unreleased

## 0.14.0 - 2026-07-16

### Added

- Adaptive TUI states, scrolling, help overlay and terminal-safe cleanup.
- `--check`, `doctor --json` and sanitized verbose diagnostics.
- Reliable daily history ingestion with observable, resumable backfill.
- Transactional sandboxed install plans and automatic rollback.
- Release smoke tests, performance budgets and Beta feedback templates.

### Changed

- Provider collection runs concurrently while preserving display order.
- Pi and SQLite history processing use bounded incremental reads and batches.
- Release artifacts are built as locked x86_64 MUSL tarballs with SHA-256.

### Fixed

- Interrupted persistence and failed ingestion no longer replace good state.
- Backfill cancellation preserves committed days and resumes contiguously.
- TUI loading states recover after timeout, panic and terminal interruption.

### Security

- Atomic private persistence, restrictive permissions and advisory locks.
- Symlink traversal is rejected and lock loss aborts before replacement.

[Unreleased]: https://github.com/samuhlo/lazysubs-eye/compare/v0.14.0...HEAD
[0.14.0]: https://github.com/samuhlo/lazysubs-eye/compare/v0.13.0...v0.14.0
