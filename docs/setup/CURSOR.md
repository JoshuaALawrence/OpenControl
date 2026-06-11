# Cursor Setup

This guide walks you through setting up OpenControl with Cursor.

## Prerequisites

- Cursor installed ([download](https://cursor.com))
- OpenControl.exe downloaded or installed via `Install.ps1`

## Option A: Auto-Setup (Easiest)

If you used `Install.ps1`, configuration is already done!

**Just:**

1. Close Cursor completely
2. Reopen Cursor
3. Ask: *"Take a screenshot"*

Done! Skip to [Testing](#testing) below.

## Option B: Manual Setup

### Step 1: Close Cursor

Quit Cursor completely. Don't just minimize.

### Step 2: Open Settings File

Open the Cursor settings file in a text editor:

**Windows (PowerShell):**

```powershell
notepad $env:APPDATA\Cursor\User\settings.json
```

**Windows (Command Prompt):**

```cmd
notepad %APPDATA%\Cursor\User\settings.json
```

**Mac:**

```bash
open ~/Library/Application\ Support/Cursor/User/settings.json
```

### Step 3: Add OpenControl

Find `"cursor.mcp"` section. If it doesn't exist, add it.

**If file is empty or new:**

```json
{
  "cursor.mcp": {
    "opencontrol": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

**If you have other settings:**

```json
{
  "editor.fontSize": 14,
  "cursor.mcp": {
    "opencontrol": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

### Step 4: Save

Save the file (Ctrl+S or Cmd+S).

### Step 5: Restart Cursor

Reopen Cursor. Wait for it to fully load (10-15 seconds).

### Step 6: Test

Try asking Cursor:

```
Take a screenshot
```

Cursor should respond with an image of your screen.

Then try:

```
What's on my screen?
```

Or:

```
Click the File menu
```

## Troubleshooting

### Cursor says no MCP tools available

1. ✓ Close Cursor completely
2. ✓ Check settings.json for:
   - Correct path: `C:\\Program Files\\OpenControl\\OpenControl.exe`
   - Valid JSON (check for typos, missing quotes)
   - Section under `"cursor.mcp"`

Example of CORRECT:

```json
{
  "cursor.mcp": {
    "opencontrol": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

1. ✓ Save file
2. ✓ Restart Cursor completely

### Settings.json syntax error

Check for:

- Missing commas between properties
- Missing or extra quotes
- Extra comma after last item

### Binary not found

Run installer:

```powershell
.\Install.ps1
```

Or place binary manually at:

```
C:\Program Files\OpenControl\OpenControl.exe
```

### "Windows protected your PC" message

Normal on first run. Click:

1. **More info**
2. **Run anyway**

Safe to proceed.

### After restart, still no tools

1. Check admin privileges:
   - Right-click Cursor
   - "Run as administrator"
   - Restart
   - Wait 15 seconds for full load

2. Check logs:
   - Help menu → Show Logs
   - Look for "opencontrol" or "MCP" messages
   - Report any errors on GitHub

## Using OpenControl in Cursor

### In Chat

Open Cursor's AI chat and ask:

```
Take a screenshot
```

Or:

```
What text is visible on my screen?
```

### In Composer

You can also use OpenControl in Cursor's Composer for complex automations:

```
1. Take a screenshot
2. Find the Chrome window
3. Click on it
4. Type "hello"
```

## Example Prompts

- "Take a screenshot and describe what you see"
- "List open windows"
- "Click on the VS Code icon in my taskbar"
- "Read the text in my terminal"
- "Open Notepad and type 'test'"
- "What's the weather in my taskbar?"

## Performance

- First screenshot: 1-2 seconds (Windows encoding)
- Subsequent: Fast (cached)
- OCR: 1-2 seconds first run, faster after

## Questions?

- Review [QUICKSTART.md](../../QUICKSTART.md)
- Check [TROUBLESHOOTING.md](../../TROUBLESHOOTING.md)
- See [Cursor Docs](https://docs.cursor.com)
- File issue on [GitHub](https://github.com/yourusername/computer-use/issues)

## Next Steps

1. ✅ Setup settings.json
2. ✅ Restart Cursor
3. ✅ Test screenshot
4. 💡 Try automations and OCR
5. 🔧 Share feedback or report issues

Enjoy! 🚀
