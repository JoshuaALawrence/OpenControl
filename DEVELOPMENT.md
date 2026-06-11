# Development Guide

This guide covers local development, testing, and release workflows for OpenControl.

## Prerequisites

- **Rust**: 1.70+ ([install](https://rustup.rs/))
- **Windows**: 10/11 (required for build)
- **Git**: For version control and tagging
- **Python**: 3.9+ (optional, for MCP test client)

## Quick Start

### Build

```powershell
# Release build (optimized)
.\Build.cmd release

# Debug build (faster compile)
.\Build.cmd debug

# Or use cargo directly
cargo build --release
```

Output: `target/release/OpenControl.exe` (~2 MB)

### Test

```powershell
# Run all tests
.\Build.cmd test

# Run specific test
cargo test --release keysym::tests::

# Run integration tests only
cargo test --release --test integration_test

# Run Python MCP test (requires binary built first)
cd tests
python mcp_rust_test.py
```

### Development Build & Test Loop

```powershell
# Watch for changes and rebuild (requires cargo-watch)
cargo watch -x "build --release" -x "test --release"

# Or manually:
cargo build --release
cargo test --release
```

## Project Structure

```
opencontrol/
├── src/
│   ├── lib.rs              # Library entry, module exports
│   ├── main.rs             # MCP server binary
│   ├── protocol.rs         # Data structures (tests included)
│   ├── worker.rs           # STA COM thread manager
│   ├── blocklist.rs        # User privacy rules: parsing + matching (tests included)
│   ├── interrupt.rs        # Escape key interrupt handling
│   ├── uia.rs              # UI Automation accessibility
│   ├── capture/            # Screen capture
│   │   ├── mod.rs
│   │   ├── desktop.rs      # Desktop info, annotation
│   │   ├── redact.rs       # Blocklist redaction: geometry + pixels (tests included)
│   │   └── wgc.rs          # Windows.Graphics.Capture
│   ├── input/              # Keyboard/mouse input
│   │   ├── mod.rs
│   │   └── keysym.rs       # X11 keysym names (tests included)
│   ├── vision/             # OCR
│   │   ├── mod.rs
│   │   └── ocr.rs
│   └── system/             # System info, files, processes
│       ├── mod.rs
│       ├── sys.rs
│       ├── installed.rs
│       └── winutil.rs
├── tests/
│   ├── integration_test.rs # Integration tests (incl. blocklist redaction)
│   ├── mcp_rust_test.py    # Python MCP client smoke (incl. blocklist)
│   ├── redaction-smoke.ps1 # Privacy/redaction end-to-end smoke
│   └── smoke-test.ps1      # fmt + clippy + unit-test pre-commit check
├── .github/
│   └── workflows/
│       ├── ci.yml          # Tests, build, clippy on every push
│       ├── release.yml     # Build and release on tag push
│       └── python-test.yml # Python client test
├── scripts/
│   └── release.ps1         # Release helper script
├── Cargo.toml
├── build.rs                # Icon and version embedding
├── Build.cmd               # Windows build helper
└── release.toml            # cargo-release configuration
```

## Making Changes

### Adding a New Tool

1. Add the tool function in appropriate module (`capture/`, `input/`, `vision/`, `system/`)
2. Add `#[tool(...)]` macro in `main.rs`
3. Add integration test in `tests/integration_test.rs`
4. Update [README.md](./README.md) tools list
5. Update [CHANGELOG.md](./CHANGELOG.md)

### Adding Tests

**Unit tests** — Add inline `#[cfg(test)]` modules:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        // test body
    }
}
```

**Integration tests** — Add to `tests/integration_test.rs`

```rust
#[test]
fn test_new_feature() {
    // Integration test
}
```

### Debugging

- **Logs**: Tools print to stderr; capture in terminal
- **Breakpoints**: VS Code Debugger with Rust extension
- **Profiling**: Use Windows Performance Toolkit or `cargo flamegraph`

## CI/CD Pipeline

### On Every Push to main/develop

**GitHub Actions: CI Workflow** (`ci.yml`)
- ✓ `cargo test --release` (unit + integration tests)
- ✓ `cargo clippy` (lints)
- ✓ `cargo fmt --check` (formatting)
- ✓ `cargo build --release` (release binary)
- ✓ Security audit (RUSTSEC)

### On Tag Push (e.g., `git push origin v0.2.0`)

**GitHub Actions: Release Workflow** (`release.yml`)
- ✓ Build release binary
- ✓ Create GitHub Release
- ✓ Upload `OpenControl.exe` as asset
- ✓ Create release branch

## Releasing

### Automated Release (Recommended)

```powershell
# From project root
.\scripts\release.ps1 -Version "0.2.0"
```

This will:
1. Verify working directory is clean
2. Update `Cargo.toml` version
3. Run all tests
4. Create signed git commit
5. Create and push annotated tag
6. GitHub Actions automatically builds and releases

### Manual Release

If you prefer manual steps:

```powershell
# 1. Update version in Cargo.toml
# 2. Update CHANGELOG.md

# 3. Test
cargo test --release

# 4. Build
cargo build --release

# 5. Commit
git add Cargo.toml CHANGELOG.md
git commit -m "chore: release v0.2.0"

# 6. Tag and push (triggers GitHub Actions)
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin main
git push origin v0.2.0
```

### Verify Release

1. Monitor: https://github.com/yourusername/computer-use/actions
2. Once complete, check: https://github.com/yourusername/computer-use/releases/tag/v0.2.0
3. Download and verify `OpenControl.exe`

## Version Management

Versions follow [Semantic Versioning](https://semver.org/):
- **MAJOR**: Breaking changes to MCP protocol or tool API
- **MINOR**: New features (backward compatible)
- **PATCH**: Bug fixes (backward compatible)

Examples:
- `0.1.0` → `0.2.0` (new tools)
- `0.2.0` → `0.2.1` (bug fix)
- `0.2.1` → `1.0.0` (stable release)

## Dependency Management

### Adding Dependencies

```powershell
# Add with cargo
cargo add <crate>

# Or edit Cargo.toml manually
```

### Checking for Security Issues

```powershell
# Local audit
cargo audit

# Or relies on CI (GitHub Actions automated)
```

### Updating Dependencies

```powershell
# Check for updates
cargo outdated

# Update all
cargo update

# Test after update
cargo test --release
```

## Performance Optimization

### Profiling

```powershell
# Build release with debug symbols
cargo build --release --profile=release-with-symbols

# Use Windows Performance Toolkit
wpr -start CPU
# ... run operations ...
wpr -stop trace.etl
```

### Binary Size

The release build uses aggressive optimization:

```toml
[profile.release]
opt-level = "z"  # Optimize for size
lto = true       # Link-time optimization
codegen-units = 1  # Single codegen unit for better optimization
strip = true     # Strip debug symbols
```

Current size: ~2 MB

## Troubleshooting

### Build Failures

```powershell
# Clean rebuild
cargo clean
cargo build --release

# Check Rust version
rustc --version
rustup update

# Clear cargo cache
cargo clean
rm -r target/
```

### Test Failures

```powershell
# Run tests with full output
cargo test --release -- --nocapture

# Run single test
cargo test --release test_name -- --nocapture

# Python test requires built binary
cargo build --release
cd tests
python mcp_rust_test.py
```

### Windows-Specific Issues

- **COM Threading**: Worker handles STA/MTA via tokio spawn_blocking
- **DPI**: Per-monitor DPI applied correctly for multi-monitor
- **UAC**: Run as admin for full window access
- **Antivirus**: May slow down file operations

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Windows Dev Docs](https://learn.microsoft.com/en-us/windows/win32/api/)
- [MCP Spec](https://spec.modelcontextprotocol.io/)
- [GitHub Actions Docs](https://docs.github.com/en/actions)

## Getting Help

- **Issues**: File on GitHub with reproduction steps
- **Discussions**: Use GitHub Discussions for questions
- **Security**: Email security@ instead of filing public issue

## Code Style

Follow Rust conventions and the existing codebase:
- `cargo fmt` for formatting
- `cargo clippy` before commit
- Minimal comments (document intent, not obvious behavior)
- No module-header doc comments or verbose banners
- Tests alongside implementation (inline `#[cfg(test)]`)
