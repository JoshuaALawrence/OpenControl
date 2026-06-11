# FAQ - Frequently Asked Questions

**Can't find your answer?** [File an issue](https://github.com/joshuaalawrence/opencontrol/issues) or check [TROUBLESHOOTING.md](./TROUBLESHOOTING.md).

## Installation & Setup

### How do I install OpenControl?

**Easiest:** Run the installer
```powershell
curl -o Install.ps1 https://raw.githubusercontent.com/joshuaalawrence/opencontrol/main/Install.ps1
.\Install.ps1
```

**Or manually:** 
1. Download `OpenControl.exe` from [GitHub Releases](https://github.com/joshuaalawrence/opencontrol/releases)
2. Place in `C:\Program Files\OpenControl\`
3. Follow client-specific setup in [QUICKSTART.md](./QUICKSTART.md)

### Why do I need to restart my client?

MCP connections are established on client startup. Restarting refreshes the connection to OpenControl.

### Do I need to run as Administrator?

Yes, for full functionality. Right-click your client app → "Run as administrator".

Without admin:
- ✓ Screenshots work
- ✓ UI automation works
- ✗ Keyboard/mouse might be blocked
- ✗ Window focus might not work

### Can I install OpenControl to a different location?

Yes. Just update the config file path:
- Claude Desktop: `mcpServers.OpenControl.command`
- VS Code: `github.copilot.advanced.mcp.opencontrol.command`
- Cursor: `cursor.mcp.opencontrol.command`

Use forward slashes or escape backslashes: `C:/Program Files/OpenControl/OpenControl.exe`

### Do I need to restart Windows to use OpenControl?

No. Just restart your MCP client (Claude Desktop, VS Code, Cursor, etc.).

## Features & Capabilities

### What can OpenControl do?

Take screenshots, click, type, read text, manage windows, run commands, and much more. See [README.md](./README.md#tools-32-total) for full list.

### Can OpenControl record videos?

Not yet. It can take individual screenshots. Full video recording is on the [roadmap](./README.md#roadmap).

### Can I use OpenControl with ChatGPT?

Not directly (ChatGPT doesn't support MCP yet). But you can:
- Use with Claude Desktop or VS Code Copilot
- Build your own agent using OpenControl
- [Vote for ChatGPT MCP support](https://openai.com/form/feedback)

### Does OpenControl work on Mac/Linux?

Not yet. Windows only for now. [Roadmap](./README.md#roadmap) includes future ports.

### Can OpenControl access network resources?

Yes. It can run PowerShell commands, which can access network shares, APIs, etc.

### Can I use this to automate repetitive tasks?

Yes. You can build scripts using any MCP client. Ask your AI to do repetitive tasks and let it automate for you.

## Security & Privacy

### Is OpenControl safe?

Yes:
- ✓ Open source (code auditable on GitHub)
- ✓ Local only (runs on your machine only)
- ✓ No cloud transmission
- ✓ No telemetry or tracking
- ✓ Builds from source with Rust

### Does my data go to the cloud?

No. OpenControl runs only on your computer. When you share a screenshot with Claude/Copilot, that follows their privacy policy.

### How do I stop the AI from seeing or controlling a specific app?

Use the built-in **application blocklist** — a user-owned control the agent cannot change. Any matching
window is redacted (blacked out or blurred) from every screenshot and OCR result *before* the image is
sent or saved, hidden from `list_windows`/`list_apps`, and refused for control.

Quickest setup — add environment variables to the server entry in your MCP host config (values are
`;`- or `,`-separated):

```json
"env": {
  "OPENCONTROL_BLOCK_EXE": "KeePass.exe; Bitwarden.exe",
  "OPENCONTROL_BLOCK_TITLE": "*Password*; *Incognito*"
}
```

For per-rule blur, exact window class, or a custom fill color, create
`%APPDATA%\OpenControl\blocklist.json`:

```json
{
  "fail_closed": true,
  "default_mode": { "type": "solid", "color": "#000000" },
  "rules": [
    { "exe_name": "KeePass.exe" },
    { "title": "*Incognito*", "mode": { "type": "blur", "sigma": 24 } }
  ]
}
```

Matching is case-insensitive; fields within a rule are AND-ed, separate rules are OR-ed, and `title` is
a substring match or a `*` glob. The AI can view (but not edit) the rules with the `get_blocklist` tool.

### Can I use this in a corporate environment?

Yes, with restrictions:
- Follows AGPL-3.0 license (you must share improvements)
- For proprietary use, contact us for commercial licensing
- Works on any Windows machine with admin access

### What if I don't trust the binary?

Build it yourself:
```bash
git clone https://github.com/joshuaalawrence/opencontrol
cd opencontrol
cargo build --release
```

Binary at `target/release/OpenControl.exe`. Same code, built by you.

### Does it log my actions?

No. OpenControl does not retain screenshots, keystrokes, or actions by default.

Your client (Claude Desktop, VS Code, etc.) may log based on their settings.

## Troubleshooting

### OpenControl installed but my client says no tools available

1. ✓ Restart your client completely (quit and reopen)
2. ✓ Check config file path is correct (see client-specific guides)
3. ✓ Verify binary exists: `C:\Program Files\OpenControl\OpenControl.exe`
4. ✓ Run client as Administrator

See [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for more.

### Screenshots return blank/black images

Usually a display driver issue. Try:
1. Restart your client
2. Run as Administrator
3. Update graphics drivers
4. Check display scaling (set to 100% temporarily)

### My clicks/typing don't work

Needs Administrator. Run client as admin → Restart → Try again.

### Antivirus blocks OpenControl

Add exception in your antivirus:
- Path: `C:\Program Files\OpenControl`
- Then restart your client

### "Windows protected your PC" on first run

Normal for unsigned binaries. Click "Run anyway".

## Performance

### Why are screenshots slow?

Screenshots encode as JPEG/PNG which takes time. Expected:
- Full 4K screenshot: 300-500ms
- 1080p: 150-300ms
- Smaller region: 50-100ms

**Tip:** Ask for specific regions instead of full screen.

### Why is OCR slow on first run?

Windows.Media.Ocr loads language pack on first use (5-10s). Subsequent calls are fast (1-2s).

### How can I make it faster?

- Request smaller screenshots (specify max width)
- Use `observe` for UI automation (faster than screenshots)
- Run as Administrator (fewer permission checks)
- Close other resource-heavy apps

## Licensing & Contributing

### What license is this under?

AGPL-3.0-or-later. See [LICENSE](./LICENSE).

You can use it freely, but if you build on it, you must share improvements.

### Can I use this commercially?

Not under AGPL. For commercial/proprietary use, contact us for licensing options. File an issue to inquire.

### How do I contribute?

1. Fork on GitHub
2. Make changes
3. File pull request
4. Follow code style (cargo fmt, cargo clippy)

See [DEVELOPMENT.md](./DEVELOPMENT.md) for full contributor guide.

### Can I report bugs?

Yes! [File an issue](https://github.com/joshuaalawrence/opencontrol/issues) with:
- Windows version (run `winver`)
- Which client (Claude, VS Code, Cursor)
- What you were trying to do
- Error message
- Steps to reproduce

### Can I request features?

Yes! [Open a discussion](https://github.com/joshuaalawrence/opencontrol/discussions) or file an issue with the feature tag.

## Technical

### What MCP protocol version does OpenControl use?

**2024-11-05** (latest)

### Can I use an older version of OpenControl?

Yes. All releases available on [GitHub](https://github.com/joshuaalawrence/opencontrol/releases). Download specific version and point your client to it.

### How do I uninstall?

```powershell
.\Install.ps1 -Uninstall
```

Or manually:
1. Delete `C:\Program Files\OpenControl\`
2. Remove config entries from your client (see client-specific guides)

### Can I run multiple instances?

Technically yes, but not recommended. Both would try to control the same desktop and conflict.

### What if my Windows version is old?

OpenControl requires Windows 10 or 11. Older versions (7, 8) are not supported.

### Does it work on Windows Server?

Should work, but not officially tested. Report issues if you try.

## Still have questions?

- 📖 Read [QUICKSTART.md](./QUICKSTART.md)
- 🔧 Check [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)
- 💬 [Discussions on GitHub](https://github.com/joshuaalawrence/opencontrol/discussions)
- 🐛 [File an issue](https://github.com/joshuaalawrence/opencontrol/issues)
- 📚 [Development guide](./DEVELOPMENT.md) for technical details

---

**Last updated:** 2026-06-10

Have a suggestion to improve this FAQ? [Edit on GitHub](https://github.com/joshuaalawrence/opencontrol/blob/main/FAQ.md)
