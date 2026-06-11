# Claude Desktop Setup

This guide walks you through setting up OpenControl with Claude Desktop step-by-step.

## Prerequisites

- Claude Desktop installed ([download](https://claude.ai/download))
- OpenControl.exe downloaded or installed via `Install.ps1`

## Option A: Auto-Setup (Easiest)

If you used `Install.ps1`, configuration is already done!

**Just:**

1. Close Claude Desktop completely
2. Reopen Claude Desktop
3. Ask: *"Take a screenshot"*

Done! Skip to [Testing](#testing) below.

## Option B: Manual Setup

### Step 1: Close Claude Desktop

First, quit Claude Desktop completely. Don't just minimize — fully close it.

```
Click the X button or use: Cmd+Q (Mac) / Alt+F4 (Windows)
```

### Step 2: Open Configuration File

Open the configuration file in a text editor:

**Windows (PowerShell):**

```powershell
notepad $env:APPDATA\Claude\claude_desktop_config.json
```

**Windows (Command Prompt):**

```cmd
notepad %APPDATA%\Claude\claude_desktop_config.json
```

**Mac:**

```bash
open ~/Library/Application\ Support/Claude/claude_desktop_config.json
```

### Step 3: Add OpenControl

In the file, find the `"mcpServers"` section. If it doesn't exist, add it.

**If the file is empty or new:**

```json
{
  "mcpServers": {
    "OpenControl": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

**If you already have other servers:**

```json
{
  "mcpServers": {
    "existing-server": {
      "command": "..."
    },
    "OpenControl": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

### Step 4: Save

Save the file (Ctrl+S or Cmd+S).

### Step 5: Restart Claude Desktop

Restart Claude Desktop:

1. Open Claude Desktop again
2. Wait for it to fully load (5-10 seconds)
3. You should see "OpenControl" in the MCP indicator or tool list

## Step 6: Test

Try asking Claude:

```
"Take a screenshot"
```

Claude should respond with an image of your screen.

If that works, try:

```
"What windows are currently open?"
```

Or:

```
"Click on the File menu"
```

## Troubleshooting

### Claude says "No MCP servers available"

1. ✓ Close Claude Desktop completely
2. ✓ Open config file again and verify:
   - No typos in "OpenControl"
   - Path is correct: `C:\\Program Files\\OpenControl\\OpenControl.exe`
   - File is valid JSON (no missing commas or quotes)
   - Section is under `"mcpServers"`

Example of valid structure:

```json
{
  "mcpServers": {
    "OpenControl": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

1. ✓ Save file
2. ✓ Restart Claude Desktop

### "Windows protected your PC" appears

This is normal on first run. Click:

1. **More info**
2. **Run anyway**

Windows shows this because the binary is unsigned (individual project). Safe to proceed.

### Path not found or file not found error

The binary isn't installed. Run installer first:

```powershell
.\Install.ps1
```

Or manually download from [GitHub Releases](https://github.com/yourusername/computer-use/releases) and place at:

```
C:\Program Files\OpenControl\OpenControl.exe
```

### File is all text, very long

You may have opened the wrong file. Make sure you opened:

- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`
- **Mac:** `~/Library/Application Support/Claude/claude_desktop_config.json`

(Not a system file or something else)

### After restarting, it still doesn't work

1. Check admin rights:
   - Right-click Claude Desktop
   - Select "Run as administrator"
   - Restart it in admin mode
   - Try taking a screenshot again

2. Check antivirus isn't blocking:
   - Windows Security → Virus & threat protection
   - Exceptions → Add exclusion
   - Add folder: `C:\Program Files\OpenControl`
   - Restart Claude

## Questions?

- Review [QUICKSTART.md](../../QUICKSTART.md)
- Check [TROUBLESHOOTING.md](../../TROUBLESHOOTING.md)
- File issue on [GitHub](https://github.com/yourusername/computer-use/issues)

## Next Steps

Once OpenControl is working, try these example prompts:

- "Take a screenshot and describe what you see"
- "List all visible windows"
- "Click on the Firefox window"
- "Type 'Hello World' in Notepad"
- "Find the 'Save' button and click it"
- "Read all text on my screen"

Enjoy! 🎉
