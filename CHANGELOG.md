# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- No unreleased entries yet.

## [0.1.0] - 22.03.2026

Acton 0.1.0 is the first semi-stable beta release with a complete installation and delivery story.
It makes the CLI easy to install from official artifacts while keeping the project on the beta release channel.

### Added

- Added an official shell installer (`acton-installer.sh`) for public beta releases.
- Added official release artifacts for four first-class platforms: macOS (ARM64, x86_64) and Linux GNU (ARM64, x86_64).
- Added a full CI and release pipeline around builds, release validation, artifact checks, public release mirroring, and dependency and security checks.
- Added broader developer tooling over the last few months, including `acton doctor`, `func2tolk`, `Acton.toml` schema generation, better starter templates, and improved TypeScript wrapper generation.

### Changed

- Promoted Acton to the `0.1.x` beta release line.
- Changed the recommended installation flow to use the public installer and official release artifacts.
- Improved `acton up`, templates, wrappers, localnet and network workflows, test reporting, and documentation across the project.

### Fixed

- Fixed numerous issues across CI, release automation, tests, documentation, wrappers, wallets, localnet and litenode integration, formatter output, and diagnostics.
- Fixed multiple flaky tests and platform-specific issues, especially around macOS and release workflows.
- Fixed many smaller bugs and polish issues accumulated over the last few months across the CLI, compiler-facing tooling, and project templates.

### Upgrade Notes

- Prefer installing or updating via `acton-installer.sh` or the official release archives.
- First-class public artifacts are available for macOS (ARM64, x86_64) and Linux GNU (ARM64, x86_64).
- If you use generated TypeScript wrappers, note that recent releases now emit them into `wrappers/` by default.

## [0.0.21] - 21.03.2026

### Added

- Added a `counter` starter template with a React + Vite app for `acton new`.
- Added `func2tolk --version`.

### Changed

- Changed generated TypeScript wrappers to go to `wrappers/` by default.

### Upgrade Notes

- If you rely on generated TypeScript wrappers, update any tooling that expected the previous default output location.
- Project references now use the `ton-blockchain/acton` repository path.

### Internal

- Added `cargo xtask schema` to generate the `Acton.toml` JSON schema.
- Added baseline maintainer and project docs, including release, support, security, and conduct policies.
- Improved CI and release automation reliability across release checks and macOS workflows.

## [0.0.20] - 18.03.2026

First version with completed CI.
