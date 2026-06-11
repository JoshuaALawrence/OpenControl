# Contributing to OpenControl

Thanks for helping improve OpenControl. This project is a Windows-native Rust MCP server, so changes should stay focused on reliability, safety, privacy, and a small self-contained binary.

## Before You Start

- Read the [README](README.md), [Development Guide](DEVELOPMENT.md), and [Testing Guide](TESTING.md).
- Search existing issues and pull requests to avoid duplicate work.
- Open an issue first for large changes, new tools, protocol changes, security-sensitive behavior, or changes that affect privacy boundaries.
- Do not report vulnerabilities in public issues. Follow the [Security Policy](SECURITY.md).

## Development Setup

Requirements:

- Windows 10 or 11, 64-bit
- Rust stable 1.70 or newer
- Git
- Python 3.9 or newer for optional MCP client tests

Build from the repository root:

```powershell
cargo build --release
```

Or use the helper script:

```powershell
.\Build.cmd release
```

The release binary is written to `target\release\OpenControl.exe`.

## Testing

Run the checks that match the change you made. For most pull requests, run:

```powershell
cargo fmt -- --check
cargo clippy --release --all-features -- -D warnings
cargo test --release --all-features
```

Useful focused checks:

```powershell
cargo test --release --lib
cargo test --release --test integration_test
.\tests\smoke-test.ps1
.\tests\smoke-test.ps1 -Full
```

If the MCP host is holding `OpenControl.exe`, stop the running process before rebuilding.

## Code Guidelines

- Follow the existing Rust style and run `cargo fmt`.
- Keep the binary small and dependency footprint conservative.
- Prefer standard library, Windows APIs, and existing project helpers before adding a crate.
- Keep comments minimal and focused on non-obvious intent or constraints.
- Preserve the user's privacy boundaries. Changes touching screenshots, OCR, UI Automation, window control, process launching, file access, command execution, clipboard, or blocklist behavior need extra care and tests.
- Validate input at tool boundaries and return actionable errors.
- Avoid broad refactors in feature or bug-fix pull requests unless they are required for the change.

## Adding or Changing Tools

When adding a tool or changing tool behavior:

1. Add or update the implementation in the appropriate module under `src/`.
2. Add or update the MCP tool handler in `src/main.rs`.
3. Add unit or integration coverage for the behavior.
4. Update the tool catalog and relevant examples or docs.
5. Mention user-visible behavior changes in [CHANGELOG.md](CHANGELOG.md).

## Documentation

Update documentation when behavior, setup, supported hosts, security expectations, or user-facing tools change. Keep examples runnable on Windows PowerShell unless a different shell is explicitly required.

## Pull Requests

Use clear titles. Conventional Commit style is preferred, for example:

- `fix: redact blocked windows from OCR output`
- `feat: add window topmost control`
- `docs: clarify VS Code setup`

A good pull request includes:

- What changed and why
- How it was tested
- Any compatibility, privacy, or security impact
- Screenshots or logs when they help explain UI or MCP behavior

By contributing, you agree that your contribution will be licensed under this project's AGPL-3.0-or-later license.
