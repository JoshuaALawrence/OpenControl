# Changelog

All notable changes to OpenControl will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-06-10

### Added

- Initial release
- All-Rust MCP server (opencontrol)
- 54 desktop automation tools
- UI Automation support with semantic element access
- Screenshot capture (Windows.Graphics.Capture) with Set-of-Marks overlay
- Built-in OCR (Windows.Media.Ocr)
- Mouse and keyboard control with precise coordinates
- Window and process management
- Clipboard operations
- File system operations
- System information reporting
- PowerShell command execution
- 2 MB self-contained executable
- Multi-monitor DPI-aware support
- Comprehensive test suite
- CI/CD pipeline with GitHub Actions
- Automated release workflow
- Application blocklist with screenshot redaction, a user-owned privacy
  control (the agent cannot modify it) that blocks chosen apps by exe name,
  exe path, window title (substring or `*` glob), or window class. Matching
  windows are redacted (solid fill or Gaussian blur) from every capture
  (`take_screenshot`, `zoom`, `save_screenshot`, `ocr`, `find_text`,
  `find_image_on_screen`) before encoding/OCR/disk, filtered from
  `list_windows`/`list_apps`/`get_active_window`, and refused by all
  window/element/tree tools and `launch_app`. Z-order-aware occluder
  subtraction, full-frame guard, and fail-closed behavior on enumeration
  failure. Configured via `%APPDATA%\OpenControl\blocklist.json`
  (or `OPENCONTROL_BLOCKLIST`) and the `OPENCONTROL_BLOCK_EXE` /
  `OPENCONTROL_BLOCK_TITLE` environment variables. New read-only
  `get_blocklist` tool surfaces the active rules.

### Security

- Added security audit workflow
- Application blocklist redacts blocked windows from captures and OCR and
  refuses control of them, independent of model cooperation; fail-closed by
  default if window enumeration is unavailable

### Features

- **Screen Capture**: Direct3D 11 GDI capture with proper DPI handling
- **UI Automation**: Real accessibility tree with stable element indices
- **Input**: Unicode-aware keyboard input with X11 keysym names
- **OCR**: Windows Media OCR for on-screen text recognition
- **Reliability**: 4-layer fallback system (UI, visual, OCR, raw input)
- **Privacy**: User-defined application blocklist with screenshot redaction

### Notes

- Single binary (~2 MB)
- No external dependencies
- MCP protocol 2024-11-05

[0.1.0]: https://github.com/joshuaalawrence/opencontrol/releases/tag/v0.1.0
