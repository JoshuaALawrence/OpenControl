# ⚡ Quick Start — 2 Minutes to Desktop Control

Get your AI controlling your computer in **two minutes**.

## Installation

### Option 1: One-Click Installer (Easiest)

```powershell
# Download and run
curl -o Install.ps1 https://raw.githubusercontent.com/yourusername/computer-use/main/Install.ps1
.\Install.ps1
```

**What it does:**

- ✓ Downloads OpenControl (~2 MB)
- ✓ Installs to `Program Files\OpenControl`
- ✓ Automatically configures your MCP clients
- ✓ Restarts applications (Claude Desktop, VS Code, Cursor)

### Option 2: Manual Installation

1. **Download** `OpenControl.exe` from [releases](https://github.com/yourusername/computer-use/releases)
2. **Place** in `C:\Program Files\OpenControl\`
3. **Follow setup below** for your client

## Setup by Client

### Claude Desktop

1. Quit Claude Desktop completely
2. Open `%APPDATA%\Claude\claude_desktop_config.json`
3. Add this under `mcpServers`:

```json
{
  "mcpServers": {
    "OpenControl": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

1. Restart Claude Desktop
2. Done! Try: *"Take a screenshot"*

### VS Code + GitHub Copilot

1. Open VS Code Settings (Ctrl+,)
2. Search for `"copilot.advanced"`
3. Edit in JSON (click icon in top-right)
4. Add under settings:

```json
"github.copilot.advanced": {
  "mcp": {
    "opencontrol": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

1. Reload VS Code (Ctrl+Shift+P → Reload Window)
2. Done! Try: *"Tell me what's on my screen"*

### Cursor

1. Quit Cursor
2. Open `%APPDATA%\Cursor\User\settings.json`
3. Add under settings:

```json
"cursor.mcp": {
  "opencontrol": {
    "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
  }
}
```

1. Restart Cursor
2. Done! Try: *"Click the File menu"*

### Custom MCP Host

Use the binary directly:

```bash
./OpenControl.exe
```

It speaks [MCP protocol](https://spec.modelcontextprotocol.io/) over stdio.

## Test It Works

After installation, try these in your AI client:

1. **Screenshot**: *"Take a screenshot"*
   - Should see full screen image

2. **Text Detection**: *"Read the text on my screen"*
   - Should list any visible text

3. **Click**: *"Click on the VS Code window"*
   - Cursor should move to window

4. **Type**: *"Type 'Hello, AI'"*
   - Text should appear in focused window

**Not working?** See [Troubleshooting](#troubleshooting) below.

## What You Can Ask

### Screen & Vision

- "Show me my desktop"
- "What text is visible on screen?"
- "Read the content of this window"
- "Find the 'Save' button"
- "Where is the address bar?"

### Mouse & Keyboard

- "Click on Chrome"
- "Double-click the file"
- "Type my password"
- "Press Enter"
- "Scroll down"

### Window Management

- "List all open windows"
- "Open Notepad"
- "Switch to Firefox"
- "Maximize this window"
- "Close Excel"

### Files

- "List files on my Desktop"
- "Create a file called test.txt"
- "Read my config file"
- "Download this file"

### System

- "How much memory is free?"
- "What's my CPU?"
- "What Windows version?"
- "List installed apps"

## Uninstall

**Via Installer:**

```powershell
.\Install.ps1 -Uninstall
```

**Manual:** Delete `C:\Program Files\OpenControl\`

**Config:** Delete client config entries (see setup above)

## Troubleshooting

### "Permission Denied" or "Access Denied"

**Run as Administrator:**

1. Right-click `Command Prompt` → "Run as administrator"
2. Run installer or client again

**Why:** Full screen control needs elevated privileges on Windows.

### Client can't find OpenControl

**Windows Defender/Antivirus blocked it:**

1. Open Windows Security → Virus & threat protection
2. Add exception: `C:\Program Files\OpenControl\OpenControl.exe`
3. Restart client

**Not in right location:**

- Check `C:\Program Files\OpenControl\OpenControl.exe` exists
- Verify config file path is correct (no typos)

**Client not reloaded:**

1. Fully quit the client (not just minimize)
2. Wait 2 seconds
3. Restart the client
4. Try again

### "Binary is not signed"

Windows SmartScreen warning is normal:

1. Click "More info"
2. Click "Run anyway"
3. This only happens once per version

### Installer downloads but hangs

Check internet connection, then try:

```powershell
.\Install.ps1 -Help
.\Install.ps1 -Install
```

### No response from AI

**Verify it's running:**

```powershell
Get-Process OpenControl
```

**Check client logs:**

- Claude Desktop: Help menu → Open Logs Folder
- VS Code: Output panel (Bottom bar) → GitHub Copilot
- Cursor: Help → Show Logs

### Click / Type not working

**Most common:** Not running as Administrator

- Quit client
- Right-click app → Run as Administrator
- Restart client

**Second:** Focus on wrong window

- Ask AI to list windows first
- Ask it to click the specific window

## System Requirements

- **OS**: Windows 10 or 11 (64-bit)
- **RAM**: 2 GB minimum, 4 GB recommended
- **Disk**: ~20 MB free space
- **Admin**: Yes, required for full access
- **Antivirus**: May need to add exception

## Performance Tips

- **Fastest screenshots**: Run as Administrator
- **Better OCR**: Ensure good screen contrast
- **Smooth clicks**: Keep window in focus
- **Stop action**: Press Escape key to abort

## Privacy & Security

- **Local only**: Runs only on your machine (MCP server, not cloud)
- **No data leaves**: All screenshots/text stay local
- **Ephemeral**: No history saved by default
- **Your client**: Only YOUR MCP client can use it
- **Block apps from the AI**: Hide specific apps (password managers, private chats) so they're redacted from every screenshot/OCR result and can't be controlled

When you share a screenshot with Claude/Copilot/etc., that follows your client's privacy policy (not OpenControl's).

### Block an app from the AI

Add an environment variable to your MCP server config (`;`- or `,`-separated):

```json
"env": {
  "OPENCONTROL_BLOCK_EXE": "KeePass.exe; Bitwarden.exe",
  "OPENCONTROL_BLOCK_TITLE": "*Password*"
}
```

Matching windows are blacked out (or blurred) in every capture before it reaches the model, hidden from window lists, and refused for control. For per-rule blur or class matching, use `%APPDATA%\OpenControl\blocklist.json` — see the [README](./README.md#privacy--app-blocking).

## Next Steps

1. ✅ **Run installer** and test
2. 📖 **Read [README.md](./README.md)** for full feature list
3. 💡 **Try advanced prompts** (upload images, read PDFs, etc.)
4. 🐛 **Report issues** on GitHub if something breaks

## Still Stuck?

1. Check [TROUBLESHOOTING.md](./TROUBLESHOOTING.md)
2. Search [GitHub Issues](https://github.com/yourusername/computer-use/issues)
3. File new issue with:
   - Windows version (run `winver`)
   - Which client (Claude, VS Code, Cursor)
   - Error message
   - Steps to reproduce

## Feedback

Have ideas to make setup easier? [Open an issue](https://github.com/yourusername/computer-use/issues/new)!

---

**That's it!** Enjoy your AI-controlled desktop. 🚀
