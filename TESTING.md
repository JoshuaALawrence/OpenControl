# Testing & CI/CD Setup Guide

This document summarizes the comprehensive testing and continuous integration/deployment infrastructure set up for OpenControl.

## Testing Overview

### Unit Tests

Unit tests are embedded in source files using `#[cfg(test)]` modules:

#### Existing Tests

1. **`src/input/keysym.rs`** — Key symbol parsing
   - `test_resolve_modifier_keys` — Ctrl, Alt, Shift, Win variants
   - `test_resolve_function_keys` — F1-F24 keys
   - `test_resolve_navigation_keys` — Arrow keys, Home, End, Page Up/Down
   - `test_resolve_ascii_letters` — A-Z
   - `test_resolve_ascii_digits` — 0-9
   - `test_resolve_numpad_keys` — Numpad keys with prefixes
   - `test_is_modifier` — Modifier key detection
   - `test_all_names_completeness` — All key names available
   - `test_case_insensitivity` — Case handling

2. **`src/protocol.rs`** — Data structures (Window, AppEntry)
   - `test_window_serialization_minimal` — Serialize window without title
   - `test_window_serialization_with_title` — Serialize window with title
   - `test_window_deserialization` — Deserialize window from JSON
   - `test_app_entry_serialization_minimal` — Serialize app without metadata
   - `test_app_entry_with_windows` — Serialize app with multiple windows
   - `test_negative_window_id` — Handle negative window IDs
   - `test_large_window_id` — Handle i64::MAX window IDs

3. **`src/blocklist.rs`** — Application blocklist parsing & matching
   - exe-name / exe-path / title (substring + `*` glob) / class matching
   - AND within a rule, OR across rules; empty rule/list match nothing
   - JSON parsing, `fail_closed` default, blur-sigma clamping, hex color
   - environment-variable rule parsing

4. **`src/capture/redact.rs`** — Redaction geometry & pixels
   - rect intersect, `subtract_hole` (4-quadrant), `subtract_occluders`
   - `union_area` (disjoint / overlap / contained)
   - screen→bitmap translation + clipping
   - solid fill blacks out the region; Gaussian blur blends and keeps alpha

Run unit tests:

```powershell
# All unit tests
cargo test --release --lib

# Specific test
cargo test --release keysym::tests::test_resolve_modifier_keys
```

### Integration Tests

Integration tests are in `tests/` directory and test MCP protocol end-to-end:

- **`tests/integration_test.rs`**
  - `test_mcp_server_initialize` — Server responds to MCP initialize
  - `test_mcp_tools_list` — Tools/list returns expected tools
  - `test_blocklist_redaction_and_refusal` — End-to-end blocklist check against a
    real Notepad window: starts a server with `OPENCONTROL_BLOCK_EXE=notepad.exe`
    and asserts the app is hidden from `list_windows`, `get_blocklist` reports it,
    `focus_window`/`observe` are refused, and `take_screenshot` still returns an
    image (Notepad redacted). Skips gracefully on a headless desktop.

Run integration tests:

```powershell
# All integration tests
cargo test --release --test integration_test

# Specific test
cargo test --release --test integration_test test_mcp_server_initialize
```

### Python MCP Tests

`tests/mcp_rust_test.py` — Protocol compliance test using Python stdlib JSON-RPC:

- Initialize handshake
- Tools/list verification (incl. `get_blocklist`)
- Tool invocation (screen_info, system_info, etc.)
- Image and text response parsing
- Blocklist section: launches Notepad, then verifies a blocked server hides it,
  refuses `focus_window`/`observe`, and still returns a (redacted) screenshot

Run manually:

```powershell
# Build first
cargo build --release

# Run test
cd tests
python mcp_rust_test.py
```

### Redaction Smoke (PowerShell)

`tests/redaction-smoke.ps1` — Focused end-to-end check of the privacy feature.
Runs the blocklist integration test, captures a baseline vs. blocked screenshot
of a real Notepad window (saved as PNGs for visual comparison), proves the
blocked window's pixels are near-black, and runs the Python smoke.

```powershell
.\tests\redaction-smoke.ps1                 # build + full check
.\tests\redaction-smoke.ps1 -SkipBuild      # reuse existing binary
.\tests\redaction-smoke.ps1 -KeepArtifacts  # keep the saved PNGs
```

## Running Tests Locally

### Quick Test (< 30 seconds)

```powershell
# Unit tests only
cargo test --release --lib --quiet
```

### Full Test Suite

```powershell
# All tests (unit + integration)
cargo test --release

# With verbose output
cargo test --release -- --nocapture --test-threads=1
```

### Smoke Test (Pre-Commit)

Quick checks before committing:

```powershell
.\tests\smoke-test.ps1          # Basic checks
.\tests\smoke-test.ps1 -Full    # Full suite + build
```

### Build Helper

Enhanced `Build.cmd` with test options:

```powershell
Build.cmd test      # Run all tests
Build.cmd release   # Build release binary
Build.cmd debug     # Build debug binary
Build.cmd clean     # Clean build artifacts
```

### Makefile (Linux/WSL/Git Bash)

```bash
make test           # Unit tests
make test-all       # All tests
make lint           # Format + clippy
make build          # Release build
make clean          # Clean artifacts
```

## CI/CD Pipeline

### GitHub Actions Workflows

#### `.github/workflows/ci.yml` — Main CI

Runs on every **push to main/develop** and **pull requests**:

- **Test Suite** — `cargo test --release`
  - Unit tests
  - Integration tests
  - Clippy warnings as errors
  - Code formatting check
  - Security audit

- **Clippy** — Lint checking
  - Warns on common mistakes
  - Must pass before merge

- **Rustfmt** — Code formatting
  - Enforces consistent style

- **Build** — Release binary
  - Optimized build
  - Binary size reported
  - Artifact uploaded (7 day retention)

- **Integration Tests** — End-to-end protocol tests
  - Runs after successful build
  - 5-minute timeout

- **Security Audit** — Dependency vulnerability scanning
  - Checks RUSTSEC database
  - Non-blocking for now

#### `.github/workflows/release.yml` — Release Pipeline

Runs on **tag push** (e.g., `git push origin v0.2.0`):

- **Build and Release**
  - Builds optimized release binary
  - Creates GitHub Release
  - Uploads `OpenControl.exe` as asset
  - Generates release notes

- **Publish to crates.io** (Optional)
  - Currently disabled (`publish = false` in Cargo.toml)
  - Can be enabled for library releases

- **Create Release Branch**
  - Creates `release/X.Y.Z` branch for tracking

#### `.github/workflows/python-test.yml` — Python Protocol Tests

Runs:

- On every push to main/develop
- On pull requests
- Scheduled daily at 2 AM UTC

Tests MCP protocol compliance with Python client.

### CI/CD Status & Artifacts

- **Actions Dashboard**: <https://github.com/yourusername/computer-use/actions>
- **Artifacts**: Uploaded for each successful build (7 day retention)
- **Releases**: <https://github.com/yourusername/computer-use/releases>

## Release Process

### Automated Release (Recommended)

```powershell
# 1. Review what will change
.\scripts\release.ps1 -Version "0.2.0" -DryRun

# 2. Execute release
.\scripts\release.ps1 -Version "0.2.0"

# 3. Monitor build
# GitHub Actions automatically:
# - Builds binary
# - Creates release
# - Uploads OpenControl.exe
```

### Manual Release Steps

```powershell
# 1. Update version in Cargo.toml
# 2. Update CHANGELOG.md
# 3. Test
cargo test --release
# 4. Build
cargo build --release
# 5. Commit & tag
git add Cargo.toml CHANGELOG.md
git commit -m "chore: release v0.2.0"
git tag -a v0.2.0 -m "Release 0.2.0"
# 6. Push (triggers CI/CD)
git push origin main
git push origin v0.2.0
```

### Verification

```powershell
# Check build status
# https://github.com/yourusername/computer-use/actions

# Verify release
# https://github.com/yourusername/computer-use/releases/tag/v0.2.0

# Download and test binary
# (Windows will warn it's unsigned; dismiss to run)
```

## Test Coverage

| Module | Tests | Coverage |
|--------|-------|----------|
| `input/keysym.rs` | 9 unit tests | All public functions + edge cases |
| `protocol.rs` | 8 unit tests | Serialization/deserialization |
| `blocklist.rs` | 13 unit tests | Rule matching, JSON/env parsing, redaction policy |
| `capture/redact.rs` | 9 unit tests | Occlusion geometry + solid/blur pixel ops |
| `main.rs` (params) | 17 unit tests | Lenient parameter parsing (string/float/packed coords) |
| Integration | 3 integration tests | MCP handshake, tools/list, blocklist redaction |
| Python test | ~8 tool calls + blocklist | Real protocol compliance |

### Areas for Future Coverage

- `capture/desktop.rs` — Screenshot rendering (requires display)
- `vision/ocr.rs` — OCR functionality (requires display)
- `system/sys.rs` — System info (can mock)
- Error handling — Failure scenarios
- Concurrency — Thread safety of worker
- Performance — Benchmarks

## Best Practices

### Before Committing

```powershell
# 1. Format code
cargo fmt

# 2. Check for issues
cargo clippy --release

# 3. Run tests
cargo test --release

# Or all-in-one:
.\tests\smoke-test.ps1 -Full
```

### Before Pushing

1. Verify CI/CD passes (check GitHub Actions)
2. Run full test suite locally
3. Manual testing of modified features
4. Update CHANGELOG.md

### Before Releasing

1. Update version in Cargo.toml (semantic versioning)
2. Update CHANGELOG.md with notable changes
3. Run full test suite: `cargo test --release`
4. Build release binary: `cargo build --release`
5. Test binary manually
6. Create git tag and push (triggers release workflow)

## Environment Variables

### CI/CD Secrets (GitHub)

For automated releases, configure:

- **`CARGO_REGISTRY_TOKEN`** — For crates.io publishing (if enabled)
- **`GITHUB_TOKEN`** — Automatically provided by GitHub Actions

## Troubleshooting

### Tests Fail Locally but Pass on CI

- Check Rust version: `rustc --version`
- Update Rust: `rustup update`
- Clean and rebuild: `cargo clean && cargo build --release`

### Binary Not Built After Push

- Check GitHub Actions tab for workflow status
- Review workflow logs for errors
- Ensure tag format is correct: `v0.2.0` (not `0.2.0`)

### Release Not Created

- Verify git tag exists: `git tag -l`
- Check Actions tab for release workflow
- Ensure working directory is clean before tagging

### Integration Tests Timeout

- Increase timeout in `ci.yml`
- Tests may need display access on headless systems
- Can mark as `continue-on-error: true` if needed

## Resources

- **Cargo Book**: <https://doc.rust-lang.org/cargo/>
- **GitHub Actions**: <https://docs.github.com/en/actions>
- **Semantic Versioning**: <https://semver.org/>
- **Keep a Changelog**: <https://keepachangelog.com/>
- **MCP Specification**: <https://spec.modelcontextprotocol.io/>

## Next Steps

1. **First Release**: Run `.\scripts\release.ps1 -Version "0.2.0"`
2. **Monitor**: Watch GitHub Actions build and release
3. **Add Tests**: Expand coverage as new features are added
4. **Optimize**: Add performance benchmarks
5. **Document**: Keep CHANGELOG.md updated
