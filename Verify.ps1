#!/usr/bin/env powershell
# Setup Verification Script
# Checks if OpenControl is installed and configured correctly
# Usage: .\Verify.ps1

$ErrorActionPreference = "SilentlyContinue"

function Write-Header {
    Clear-Host
    Write-Host "`n╔════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "║     OpenControl Setup Verification      ║" -ForegroundColor Cyan
    Write-Host "╚════════════════════════════════════════╝`n" -ForegroundColor Cyan
}

function Write-Check {
    param([string]$Message, [string]$Status)
    $icon = if ($Status -eq "✓") { "✓" } else { "✗" }
    $color = if ($Status -eq "✓") { "Green" } else { "Red" }
    Write-Host "  [$icon] $Message" -ForegroundColor $color
}

function Write-Info {
    Write-Host "  ℹ $args" -ForegroundColor Gray
}

Write-Header

$allGood = $true
$exePath = "C:\Program Files\OpenControl\OpenControl.exe"

# 1. Check binary exists
Write-Host "1. Installation" -ForegroundColor Yellow
if (Test-Path $exePath) {
    $size = (Get-Item $exePath).Length / 1MB
    Write-Check "Binary found at $exePath" "✓"
    Write-Info "Size: $([math]::Round($size, 2)) MB"
} else {
    Write-Check "Binary not found at $exePath" "✗"
    Write-Info "Run: .\Install.ps1"
    $allGood = $false
}

# 2. Check Claude Desktop
Write-Host "`n2. Claude Desktop" -ForegroundColor Yellow
$claudeConfig = "$env:APPDATA\Claude\claude_desktop_config.json"
if (Test-Path $claudeConfig) {
    try {
        $config = Get-Content $claudeConfig | ConvertFrom-Json
        if ($config.mcpServers.OpenControl) {
            Write-Check "Configuration found" "✓"
            $path = $config.mcpServers.OpenControl.command
            Write-Info "Path: $path"
            if ($path -eq $exePath) {
                Write-Check "Path is correct" "✓"
            } else {
                Write-Check "Path mismatch: $path" "✗"
                $allGood = $false
            }
        } else {
            Write-Check "OpenControl not configured" "✗"
            Write-Info "See QUICKSTART.md for setup"
        }
    } catch {
        Write-Check "Invalid JSON configuration" "✗"
        Write-Info "Check $claudeConfig for syntax errors"
        $allGood = $false
    }
} else {
    Write-Check "Not yet configured (will be on first launch)" "ℹ"
}

# 3. Check VS Code
Write-Host "`n3. VS Code + GitHub Copilot" -ForegroundColor Yellow
$vscodeSettings = "$env:APPDATA\Code\User\settings.json"
if (Test-Path $vscodeSettings) {
    try {
        $config = Get-Content $vscodeSettings | ConvertFrom-Json
        if ($config.'github.copilot.advanced'.mcp.opencontrol) {
            Write-Check "MCP configured" "✓"
            $path = $config.'github.copilot.advanced'.mcp.opencontrol.command
            Write-Info "Path: $path"
            if ($path -eq $exePath -or $path -eq "C:/Program Files/OpenControl/OpenControl.exe") {
                Write-Check "Path is correct" "✓"
            } else {
                Write-Check "Path mismatch: $path" "✗"
                $allGood = $false
            }
        } else {
            Write-Check "OpenControl not configured" "✗"
            Write-Info "See QUICKSTART.md for setup"
        }
    } catch {
        Write-Check "Invalid JSON configuration" "✗"
        $allGood = $false
    }
} else {
    Write-Check "Not yet configured (will be on first launch)" "ℹ"
}

# 4. Check Cursor
Write-Host "`n4. Cursor" -ForegroundColor Yellow
$cursorSettings = "$env:APPDATA\Cursor\User\settings.json"
if (Test-Path $cursorSettings) {
    try {
        $config = Get-Content $cursorSettings | ConvertFrom-Json
        if ($config.'cursor.mcp'.opencontrol) {
            Write-Check "MCP configured" "✓"
            $path = $config.'cursor.mcp'.opencontrol.command
            Write-Info "Path: $path"
        } else {
            Write-Check "OpenControl not configured" "✗"
            Write-Info "See QUICKSTART.md for setup"
        }
    } catch {
        Write-Check "Invalid JSON configuration" "✗"
        $allGood = $false
    }
} else {
    Write-Check "Not yet configured" "ℹ"
}

# 5. Test binary startup
Write-Host "`n5. Binary Test" -ForegroundColor Yellow
if (Test-Path $exePath) {
    Write-Check "Testing if binary starts..." "?"
    try {
        $proc = Start-Process $exePath -NoNewWindow -PassThru -ErrorAction Stop
        Start-Sleep -Milliseconds 500
        if ($proc.HasExited) {
            Write-Check "Binary started and exited (expected)" "✓"
        } else {
            Write-Check "Binary still running" "✓"
            Stop-Process $proc -Force -ErrorAction SilentlyContinue
        }
    } catch {
        Write-Check "Could not start binary: $_" "✗"
        Write-Info "Check Windows Defender/antivirus exclusions"
        $allGood = $false
    }
}

# Summary
Write-Host "`n╔════════════════════════════════════════╗" -ForegroundColor Cyan
if ($allGood) {
    Write-Host "║     Setup Verified ✓                   ║" -ForegroundColor Green
    Write-Host "╚════════════════════════════════════════╝" -ForegroundColor Green
    Write-Host "`nNext steps:`n" -ForegroundColor Green
    Write-Host "  1. Restart your MCP client completely" -ForegroundColor White
    Write-Host "  2. Ask your AI: 'Take a screenshot'" -ForegroundColor White
    Write-Host "  3. See QUICKSTART.md for example prompts" -ForegroundColor White
} else {
    Write-Host "║     Issues Found ✗                     ║" -ForegroundColor Red
    Write-Host "╚════════════════════════════════════════╝" -ForegroundColor Red
    Write-Host "`nNext steps:`n" -ForegroundColor Yellow
    Write-Host "  1. Check errors above" -ForegroundColor White
    Write-Host "  2. Review TROUBLESHOOTING.md" -ForegroundColor White
    Write-Host "  3. Run: .\Install.ps1 again" -ForegroundColor White
}

Write-Host ""
