# 🔧 Troubleshooting

Can't get OpenControl working? This guide covers the most common issues.

## Installation Issues

### "PowerShell cannot be loaded because running scripts is disabled"

**Error:** `cannot be loaded because running scripts is disabled on this system`

**Solution:**
```powershell
# Run this ONCE as Administrator:
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser

# Then run installer:
.\Install.ps1
```

### "Administrator access required"

**Error:** `Administrator access required for installation`

**Solution:**
1. Right-click `PowerShell` or `Command Prompt`
2. Click "Run as administrator"
3. Navigate to download folder: `cd Downloads`
4. Run: `.\Install.ps1`

### Download hangs or times out

**Symptom:** Installer downloads forever

**Cause:** Network timeout or GitHub API rate limit

**Solution:**
1. Press Ctrl+C to stop
2. Wait 1 minute
3. Try again: `.\Install.ps1`

**Alternative:** Download manually from [GitHub Releases](https://github.com/yourusername/computer-use/releases), then place `OpenControl.exe` in `C:\Program Files\OpenControl\`

### Installation hangs during download

**Solution:**
```powershell
# Clear cache and retry
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
.\Install.ps1
```

### "Path too long" error

**Error:** `The specified path is too long`

**Cause:** Windows path length limit (usually not an issue with default installation)

**Solution:** Install to shorter path using manual installation:
1. Create `C:\OC\` folder
2. Download and place `OpenControl.exe` there
3. Update client configs to point to `C:\OC\OpenControl.exe`

## Setup & Configuration Issues

### Client says OpenControl tool is not available

**After installation, client still doesn't see OpenControl**

**Checklist:**

1. ✓ **Did you restart the client?**
   - Quit completely (not just minimize)
   - Wait 2 seconds
   - Restart client
   - Try again

2. ✓ **Check binary exists:**
   ```powershell
   Test-Path "C:\Program Files\OpenControl\OpenControl.exe"
   ```
   Should return `True`. If `False`, reinstall.

3. ✓ **Check config file syntax** (JSON must be valid):
   
   **Claude Desktop:** `%APPDATA%\Claude\claude_desktop_config.json`
   ```powershell
   # Paste into PowerShell to validate:
   Get-Content "$env:APPDATA\Claude\claude_desktop_config.json" | ConvertFrom-Json
   ```
   If no error, it's valid. If error, fix the JSON.

   **VS Code:** `%APPDATA%\Code\User\settings.json`
   ```powershell
   Get-Content "$env:APPDATA\Code\User\settings.json" | ConvertFrom-Json
   ```

4. ✓ **Check path is correct:**
   - Should be: `C:\Program Files\OpenControl\OpenControl.exe`
   - Not: `C:\Program Files\OpenControl` (missing `\OpenControl.exe`)
   - Not: `C:\\Program Files\\OpenControl\\OpenControl.exe` (double backslashes wrong for JSON)
   - Use single backslash or forward slash: `C:/Program Files/OpenControl/OpenControl.exe` ✓

### Claude Desktop still shows "MCP not enabled"

**Solution:**

1. **Check config file exists and is valid:**
   ```powershell
   # Should show JSON content, no errors:
   Get-Content "$env:APPDATA\Claude\claude_desktop_config.json" | ConvertFrom-Json
   ```

2. **Add `"mcpServers"` if missing:**
   Open `%APPDATA%\Claude\claude_desktop_config.json` and ensure it has:
   ```json
   {
     "mcpServers": {
       "OpenControl": {
         "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
       }
     }
   }
   ```

3. **Restart Claude:**
   - Quit completely
   - Restart
   - Type `@` to see tools — OpenControl should appear

### VS Code Copilot doesn't show OpenControl tools

**Solution:**

1. **Check GitHub Copilot is installed:**
   - Open VS Code Extensions (Ctrl+Shift+X)
   - Search "GitHub Copilot"
   - Must be installed and logged in

2. **Check MCP setting format:**
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

3. **Reload VS Code:**
   - Press Ctrl+Shift+P
   - Type "Reload Window"
   - Press Enter
   - Wait 10 seconds
   - Try asking: "take a screenshot"

4. **Check extension activation:**
   - Open VS Code Output panel (View > Output, or Ctrl+J)
   - From dropdown, select "GitHub Copilot"
   - Should show "MCP initialized" or similar

### Cursor can't find OpenControl

**Solution:**

Same as Claude Desktop — check `%APPDATA%\Cursor\User\settings.json` has:
```json
{
  "cursor.mcp": {
    "opencontrol": {
      "command": "C:\\Program Files\\OpenControl\\OpenControl.exe"
    }
  }
}
```

Then restart Cursor.

## Runtime Issues

### "Permission Denied" when clicking/typing

**Error:** Click or keyboard action has no effect

**Cause:** OpenControl not running with Administrator privileges

**Solution:**

1. **Restart client as Administrator:**
   - Right-click Claude Desktop / VS Code / Cursor
   - "Run as administrator"
   - Restart the client
   - Try clicking/typing again

2. **Verify it's running as admin:**
   ```powershell
   Get-Process OpenControl | Select-Object *
   # Look for "NPM" column - if high, likely running as admin
   ```

3. **If still blocked:**
   - Check Task Manager (Ctrl+Shift+Esc)
   - Kill any other `OpenControl.exe` processes
   - Restart client
   - Try again

### Screenshot returns black/blank image

**Issue:** Screenshot is all black or shows nothing

**Cause:** Often a display driver or DPI scaling issue

**Solution:**

1. **Try again** (may be temporary):
   ```
   Ask: "Take a screenshot"
   ```

2. **Check your display scaling:**
   - Right-click Desktop → Display Settings
   - Note the scaling percentage
   - If not 100%, try resetting to 100% (temporary)
   - Take screenshot again
   - Reset scaling if it was helpful

3. **Update graphics drivers:**
   - NVIDIA/AMD/Intel drivers may be outdated
   - Download latest from manufacturer
   - Install and restart
   - Try screenshot again

4. **Check for overlays:**
   - Close Discord overlay, OBS, streaming software
   - Try screenshot again

5. **Run as Administrator:**
   - Client must run as admin (see above)

### OCR (text recognition) not working

**Issue:** "Can't read text on screen" or OCR returns nothing

**Cause:** Windows Media OCR language pack missing or display issue

**Solution:**

1. **Install language pack:**
   ```powershell
   # For English (usually installed by default)
   # Settings → Time & Language → Language → Add language
   # Add English if missing
   ```

2. **Check text is visible:**
   - Ask: "Take a screenshot"
   - Visually confirm text is clear and readable
   - Black text on white background works best

3. **Improve contrast:**
   - If text is light gray/very faint, OCR won't recognize it
   - Ask AI to read specific areas instead

4. **Run as Administrator:**
   - Some OCR features need admin rights
   - Ensure client is running as admin (see Permission issues above)

### Keyboard input (typing) doesn't appear

**Issue:** AI types but nothing shows in application

**Cause:** Wrong window focused or permission issue

**Solution:**

1. **Click first:**
   ```
   Ask: "Click on the Notepad window then type 'hello'"
   ```
   AI needs to focus the window first.

2. **Run as Administrator:**
   - Client must have admin rights (see Permission issues above)

3. **Check window type:**
   - Elevated apps (Command Prompt as admin, System Settings) may block input
   - Try typing in regular app first (Notepad, browser)

4. **Try administrator mode for client:**
   - Right-click app → "Run as administrator"
   - Restart app
   - Try typing again

### Mouse doesn't move to expected location

**Issue:** Clicks happen in wrong place

**Cause:** Multi-monitor setup, DPI scaling, or coordinate mismatch

**Solution:**

1. **Single monitor:**
   - If using multiple displays, move app to main monitor
   - Ask AI: "Take a screenshot first" (establishes reference)
   - Then ask to click

2. **Check DPI scaling:**
   - Settings → Display
   - If not 100%, try setting to 100% temporarily
   - Test clicks
   - This is often the issue on laptops

3. **Restart and try again:**
   - Quit client
   - Restart as Administrator
   - Try clicking again

4. **Tell AI to use observe:**
   ```
   Ask: "Use observe to see all elements, then click [#5]"
   ```
   The `[index]` system is more reliable than coordinates.

## Performance Issues

### Takes too long to take a screenshot

**Symptom:** Screenshot request hangs for 10+ seconds

**Cause:** GPU capture bottleneck or too-large screenshot

**Solution:**

1. **Ask for smaller screenshot:**
   ```
   Ask: "Take a screenshot at max 1280 width"
   ```

2. **Update graphics drivers:**
   - Outdated drivers slow down capture
   - Download from NVIDIA/AMD/Intel

3. **Close resource-heavy apps:**
   - OBS, streaming software, video players
   - These use GPU and slow capture

4. **Restart client:**
   - Sometimes helps with GPU caching

### OCR very slow

**Symptom:** "Read text on screen" takes 20+ seconds

**Cause:** Large area of text or language pack loading

**Solution:**

1. **Ask for specific region:**
   ```
   Ask: "Read the text in the upper left corner"
   ```
   Instead of whole screen.

2. **Install language pack:**
   - First time OCR uses language, it must load
   - This is slower (5-10s)
   - Second time is faster (1-2s)

3. **Check CPU usage:**
   - Task Manager (Ctrl+Shift+Esc)
   - If CPU 100%, close other apps

### Client freezes when clicking

**Symptom:** AI clicks, then client hangs

**Cause:** Usually a focus/permission issue

**Solution:**

1. **Restart client:**
   - Quit completely
   - Wait 2 seconds
   - Restart

2. **Check for stuck processes:**
   ```powershell
   Get-Process OpenControl
   # If hung, kill it:
   Stop-Process -Name OpenControl -Force
   ```

3. **Run as Administrator:**
   - Restart client as admin
   - Try again

## Antivirus & Security

### "Windows protected your PC" / SmartScreen warning

**On first run, Windows may warn:**

```
"Windows protected your PC"
More info → Run anyway
```

This is normal. OpenControl is unsigned (being an individual project). Click "Run anyway".

### Antivirus blocks OpenControl

**Symptom:** Installed but then deleted, or client says "access denied"

**Cause:** Antivirus flagged it (false positive)

**Solution:**

1. **Add exception:**
   - Windows Security → Virus & threat protection
   - Manage settings → Exclusions
   - Add folder: `C:\Program Files\OpenControl`

2. **Disable realtime scanning temporarily:**
   - Windows Security → Virus & threat protection
   - Turn off "Realtime protection"
   - Reinstall/run
   - Re-enable when done

3. **Third-party antivirus:**
   - Add exception in your antivirus GUI
   - Path: `C:\Program Files\OpenControl\OpenControl.exe`

4. **Restore from quarantine:**
   - Check antivirus logs
   - Restore `OpenControl.exe`
   - Add exception
   - Restart

## Network & Firewall Issues

### "Connection refused" or "Network unreachable"

**Cause:** Firewall blocking MCP communication

**Solution:**

1. **Allow through Windows Firewall:**
   - Windows Security → Firewall & network protection
   - Advanced settings → Inbound Rules
   - Click "New Rule"
   - Select Program, browse to `OpenControl.exe`
   - Choose Allow
   - Finish

2. **Third-party firewall:**
   - Allow `OpenControl.exe` through
   - Or whitelist `localhost`

3. **Try again:**
   - Restart client
   - Test screenshot

## Debugging & Logs

### Check client logs

**Claude Desktop:**
- Help menu → "Open Logs Folder"
- Look for errors with "opencontrol" or "OpenControl"

**VS Code:**
- Output panel (Ctrl+J)
- Select "GitHub Copilot" from dropdown
- Look for MCP initialization messages

**Cursor:**
- Help menu → Show Logs
- Look for Cursor config or MCP messages

### Test binary directly

```powershell
# Run the binary standalone:
C:\Program Files\OpenControl\OpenControl.exe

# Send test JSON-RPC message:
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}'

# Should respond with JSON (ctrl+c to exit)
```

### Enable verbose logging

**For development/debugging:**

```powershell
# Set debug environment variable
$env:RUST_LOG = "debug"

# Then run client or binary
C:\Program Files\OpenControl\OpenControl.exe
```

## Still Stuck?

1. **Search issues:** https://github.com/yourusername/computer-use/issues
2. **File new issue** with:
   - Windows version: `winver`
   - Which client (Claude, VS Code, Cursor)
   - Full error message
   - Steps you took
   - Output of: `C:\Program Files\OpenControl\OpenControl.exe` (run once)

3. **Include logs:**
   - Client logs (see above)
   - Screenshot of config file (hide sensitive info)

---

**Most issues are permission or configuration related.** Try:
1. Run as Administrator
2. Restart client completely (not just minimize)
3. Check JSON config syntax is valid
4. Verify binary exists at correct path

If none of this works, file an issue with details!
