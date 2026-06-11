# VS Code + GitHub Copilot Setup

This guide walks you through setting up OpenControl with VS Code and GitHub Copilot.

## Prerequisites

- VS Code installed ([download](https://code.visualstudio.com/))
- GitHub Copilot extension installed and signed in
- OpenControl.exe downloaded or installed via `Install.ps1`

## Option A: Auto-Setup (Easiest)

If you used `Install.ps1`, configuration is already done!

**Just:**

1. Reload VS Code (Ctrl+Shift+P → "Reload Window")
2. Wait 5 seconds
3. Ask Copilot: *"Take a screenshot"*

Done! Skip to [Testing](#testing) below.

## Option B: Manual Setup

### Step 1: Open VS Code Settings

Press **Ctrl+,** (or Cmd+, on Mac)

This opens Settings.

### Step 2: Open Settings JSON

Click the **`{}`** icon in the top-right corner to edit settings as JSON.

### Step 3: Add MCP Configuration

Find your current settings and add this:

```json
"github.copilot.advanced": {
  "mcp": {
    "opencontrol": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

**Complete example:**

```json
{
  "editor.fontSize": 14,
  "editor.formatOnSave": true,
  "github.copilot.advanced": {
    "mcp": {
      "opencontrol": {
        "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
      }
    }
  }
}
```

### Step 4: Save

Save the file (Ctrl+S).

### Step 5: Reload VS Code

Press **Ctrl+Shift+P** and type:

```
Reload Window
```

Press Enter. VS Code will reload and initialize MCP.

### Step 6: Test

Open the VS Code chat (Ctrl+I or Copilot chat icon).

Ask:

```
Take a screenshot
```

Copilot should respond with an image of your screen.

Try:

```
What text is visible on my screen?
```

Or:

```
Click on the Explorer icon
```

## Troubleshooting

### Copilot says "No tools available" or shows no OpenControl tools

1. ✓ Check GitHub Copilot extension is installed
   - Ctrl+Shift+X (Extensions)
   - Search "GitHub Copilot"
   - Must be installed and you must be signed in

2. ✓ Check settings JSON is valid
   - Ctrl+, (Settings)
   - Click {} to open JSON view
   - Press Ctrl+Shift+P → "Preferences: Open JSON"
   - Check for syntax errors (missing quotes, commas, etc.)

3. ✓ Reload VS Code
   - Ctrl+Shift+P
   - Type "Reload Window"
   - Press Enter
   - Wait 10 seconds

### Settings.json has invalid syntax error

Common mistakes:

- Missing comma between properties
- Missing quotes around keys or values
- Extra comma after last item

**Example of WRONG:**

```json
{
  "github.copilot.advanced": {
    "mcp": {
      "opencontrol": {
        "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"  ← MISSING COMMA
      }
    }
  },  ← EXTRA COMMA HERE
}
```

**Example of CORRECT:**

```json
{
  "github.copilot.advanced": {
    "mcp": {
      "opencontrol": {
        "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
      }
    }
  }
}
```

### "Path not found" when using MCP

The binary isn't at the expected location. Run installer:

```powershell
.\Install.ps1
```

Or manually place `OpenControl.exe` at:

```
C:\Program Files\OpenControl\OpenControl.exe
```

### After reload, still no OpenControl tools

1. Check admin mode:
   - Right-click VS Code
   - "Run as administrator"
   - Restart
   - Reload window again (Ctrl+Shift+P → Reload Window)

2. Check Output panel for errors:
   - Press Ctrl+J (Opens terminal)
   - Look for "Output" tab
   - From dropdown, select "GitHub Copilot"
   - Check for error messages

### "Windows protected your PC"

Normal on first run. Click:

1. **More info**
2. **Run anyway**

Windows shows this for unsigned binaries. Safe to proceed.

## Command Palette (Advanced)

You can also access OpenControl through Copilot's tool menu:

1. Open Copilot Chat (Ctrl+I)
2. Type `@` to see available tools
3. Look for `OpenControl` tools in the list

## Using with GitHub Copilot Chat

In **Copilot Chat** (Ctrl+L or Copilot icon), you can now use OpenControl:

### Example Prompts

```
Take a screenshot and tell me what's open
```

```
Click on the VS Code Explorer icon
```

```
What is the current time shown in my taskbar?
```

```
Open Notepad and type 'Hello'
```

## Using with Inline Chat

In **Inline Chat** (Ctrl+I within editor), you can also use OpenControl:

```
Can you see my screen? If so, take a screenshot and describe it
```

## Troubleshooting Output

If something isn't working, check the Output panel for diagnostics:

1. Press **Ctrl+J** (or View > Terminal)
2. Click the **Output** tab (if not visible)
3. From the dropdown (top-right), select **GitHub Copilot**
4. Look for:
   - "MCP initialized" — Good
   - "Error" messages — Problem to debug
   - "opencontrol" — Confirms MCP is found

## Performance Tips

- First screenshot may take 1-2 seconds (Windows encoding)
- Subsequent screenshots are faster (cached)
- Use observe for UI automation (faster than screenshots for UI)

## Questions?

- Review [QUICKSTART.md](../../QUICKSTART.md)
- Check [TROUBLESHOOTING.md](../../TROUBLESHOOTING.md)
- See [VS Code Docs](https://code.visualstudio.com/docs)
- File issue on [GitHub](https://github.com/yourusername/computer-use/issues)

## Next Steps

1. ✅ Set up MCP in settings.json
2. ✅ Reload VS Code
3. ✅ Test with a screenshot
4. 💡 Try advanced prompts with file operations, OCR, etc.
5. 🔧 Check [DEVELOPMENT.md](../../DEVELOPMENT.md) if contributing code

Enjoy! 🚀
