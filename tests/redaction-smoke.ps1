#!/usr/bin/env pwsh
# Redaction / application-blocklist smoke test for OpenControl.
#
# Verifies the privacy feature end-to-end against a real Notepad window:
#   1. Builds the release binary (unless -SkipBuild).
#   2. Runs the Rust blocklist integration test.
#   3. Captures a baseline screenshot (Notepad visible) and a blocked screenshot
#      (Notepad redacted) and saves both PNGs so you can eyeball the masking.
#   4. Runs the Python MCP smoke (which includes the blocklist section).
#
# Usage:
#   .\tests\redaction-smoke.ps1
#   .\tests\redaction-smoke.ps1 -SkipBuild
#   .\tests\redaction-smoke.ps1 -KeepArtifacts   # keep the saved PNGs

param(
    [switch]$SkipBuild,
    [switch]$KeepArtifacts,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$exe = Join-Path $root "target\release\OpenControl.exe"
$outDir = Join-Path $root "target\redaction-smoke"

function Write-Step { if (-not $Quiet) { Write-Host "→ $args" -ForegroundColor Cyan } }
function Write-Pass { Write-Host "✓ $args" -ForegroundColor Green }
function Write-Fail { Write-Host "✗ $args" -ForegroundColor Red }
function Write-Skip { Write-Host "• $args" -ForegroundColor Yellow }

Write-Host "`n🛡️  OpenControl Redaction Smoke`n" -ForegroundColor Yellow

# Free the binary if a previous server is holding it.
Get-Process OpenControl, opencontrol -ErrorAction SilentlyContinue | Stop-Process -Force

# 1. Build ------------------------------------------------------------------
if (-not $SkipBuild) {
    Write-Step "Building release binary..."
    cargo build --release | Out-Null
    if ($LASTEXITCODE -ne 0) { Write-Fail "build failed"; exit 1 }
    Write-Pass "Build OK"
}
if (-not (Test-Path $exe)) { Write-Fail "binary not found: $exe"; exit 1 }

# 2. Rust integration test --------------------------------------------------
Write-Step "Running blocklist integration test..."
cargo test --release --test integration_test test_blocklist_redaction_and_refusal -- --nocapture
if ($LASTEXITCODE -ne 0) { Write-Fail "integration test failed"; exit 1 }
Write-Pass "Integration test OK"

# 3. Visual baseline vs blocked capture -------------------------------------
# Minimal stdio JSON-RPC client so we can drive two server instances directly.
function Invoke-McpCapture {
    param([hashtable]$Env, [string]$SavePath, [int[]]$Region)

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $exe
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    if ($Env) { foreach ($k in $Env.Keys) { $psi.EnvironmentVariables[$k] = $Env[$k] } }

    $proc = [System.Diagnostics.Process]::Start($psi)
    $stdin = $proc.StandardInput
    $stdout = $proc.StandardOutput
    $id = 0
    $result = @{ windows = $null; blocklist = $null }

    function Send($obj) { $stdin.WriteLine(($obj | ConvertTo-Json -Depth 12 -Compress)); $stdin.Flush() }
    function ReadId([int]$want) {
        while (-not $stdout.EndOfStream) {
            $line = $stdout.ReadLine()
            if (-not $line) { continue }
            try { $msg = $line | ConvertFrom-Json } catch { continue }
            if ($msg.id -eq $want) { return $msg }
        }
        return $null
    }

    $id++; Send @{ jsonrpc = "2.0"; id = $id; method = "initialize"; params = @{ protocolVersion = "2024-11-05"; capabilities = @{}; clientInfo = @{ name = "ps-smoke"; version = "0" } } }
    [void](ReadId $id)
    Send @{ jsonrpc = "2.0"; method = "notifications/initialized" }

    $id++; Send @{ jsonrpc = "2.0"; id = $id; method = "tools/call"; params = @{ name = "list_windows"; arguments = @{} } }
    $lw = ReadId $id
    $result.windows = ($lw.result.content | Where-Object { $_.type -eq "text" } | Select-Object -First 1).text

    $id++; Send @{ jsonrpc = "2.0"; id = $id; method = "tools/call"; params = @{ name = "get_blocklist"; arguments = @{} } }
    $bl = ReadId $id
    $result.blocklist = ($bl.result.content | Where-Object { $_.type -eq "text" } | Select-Object -First 1).text

    if ($SavePath) {
        $ssArgs = @{ path = $SavePath }
        if ($Region) { $ssArgs.region = $Region }
        $id++; Send @{ jsonrpc = "2.0"; id = $id; method = "tools/call"; params = @{ name = "save_screenshot"; arguments = $ssArgs } }
        [void](ReadId $id)
    }

    $stdin.Close()
    if (-not $proc.WaitForExit(5000)) { $proc.Kill() }
    return $result
}

# Win32 GetWindowRect so we can sample exactly the rectangle the server redacts.
if (-not ('Win32Rect' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public struct RECT { public int Left, Top, Right, Bottom; }
public static class Win32Rect {
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
}
'@
}

function Get-WindowRect([IntPtr]$hwnd) {
    $r = New-Object RECT
    if ([Win32Rect]::GetWindowRect($hwnd, [ref]$r)) {
        return @($r.Left, $r.Top, ($r.Right - $r.Left), ($r.Bottom - $r.Top))
    }
    return $null
}

# Mean luminance (0-255) of a PNG, sampled on a grid for speed.
function Get-MeanLuminance([string]$path) {
    Add-Type -AssemblyName System.Drawing
    $bmp = [System.Drawing.Bitmap]::FromFile($path)
    try {
        $stepX = [Math]::Max(1, [int]($bmp.Width / 64))
        $stepY = [Math]::Max(1, [int]($bmp.Height / 64))
        $sum = 0.0; $n = 0
        for ($y = 0; $y -lt $bmp.Height; $y += $stepY) {
            for ($x = 0; $x -lt $bmp.Width; $x += $stepX) {
                $p = $bmp.GetPixel($x, $y)
                $sum += (0.299 * $p.R + 0.587 * $p.G + 0.114 * $p.B)
                $n++
            }
        }
        if ($n -eq 0) { return 0 }
        return [Math]::Round($sum / $n, 2)
    }
    finally { $bmp.Dispose() }
}

Write-Step "Launching Notepad for the visual check..."
$np = Start-Process notepad.exe -PassThru
Start-Sleep -Seconds 1

New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$baselinePng = Join-Path $outDir "baseline.png"
$blockedPng = Join-Path $outDir "blocked.png"
$baselineRegionPng = Join-Path $outDir "baseline-region.png"
$blockedRegionPng = Join-Path $outDir "blocked-region.png"

try {
    Write-Step "Capturing baseline (no blocklist)..."
    $base = Invoke-McpCapture -Env $null -SavePath $baselinePng
    $notepadVisible = $base.windows -match "(?i)notepad\.exe"
    if (-not $notepadVisible) {
        Write-Skip "Notepad window not detected (headless desktop?). Visual check skipped."
    }
    else {
        Write-Pass "Baseline captured: $baselinePng"

        Write-Step "Capturing with Notepad blocked..."
        $blocked = Invoke-McpCapture -Env @{ OPENCONTROL_BLOCK_EXE = "notepad.exe" } -SavePath $blockedPng
        if ($blocked.windows -match "(?i)notepad\.exe") {
            Write-Fail "Notepad still listed while blocked!"
            exit 1
        }
        Write-Pass "Notepad hidden from list_windows while blocked"
        if ($blocked.blocklist -match '"active":\s*true') {
            Write-Pass "get_blocklist reports active"
        }
        if (Test-Path $blockedPng) { Write-Pass "Blocked screenshot saved: $blockedPng" }

        # Programmatic pixel proof: sample the exact rect the server redacts.
        $npProc = Get-Process Notepad -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
        $region = if ($npProc) { Get-WindowRect $npProc.MainWindowHandle } else { $null }
        if ($region -and $region[2] -gt 0 -and $region[3] -gt 0) {
            Write-Step ("Sampling Notepad rect [{0},{1} {2}x{3}] in both captures..." -f $region[0], $region[1], $region[2], $region[3])
            [void](Invoke-McpCapture -Env $null -SavePath $baselineRegionPng -Region $region)
            [void](Invoke-McpCapture -Env @{ OPENCONTROL_BLOCK_EXE = "notepad.exe" } -SavePath $blockedRegionPng -Region $region)

            $lumBase = Get-MeanLuminance $baselineRegionPng
            $lumBlocked = Get-MeanLuminance $blockedRegionPng
            Write-Host ("  mean luminance: baseline={0}  blocked={1}" -f $lumBase, $lumBlocked) -ForegroundColor Gray

            if ($lumBlocked -gt 24) {
                Write-Fail ("blocked Notepad region is not redacted (luminance {0}, expected <=24)" -f $lumBlocked)
                exit 1
            }
            if ($lumBase -le $lumBlocked) {
                Write-Fail ("baseline ({0}) should be brighter than blocked ({1})" -f $lumBase, $lumBlocked)
                exit 1
            }
            Write-Pass ("Notepad region redacted to near-black (luminance {0} vs baseline {1})" -f $lumBlocked, $lumBase)
        }
        else {
            Write-Skip "Could not resolve Notepad window rect; pixel proof skipped."
            Write-Host "  Compare the two PNGs to confirm Notepad is redacted in the blocked one." -ForegroundColor Gray
        }
    }
}
finally {
    if ($np -and -not $np.HasExited) { Stop-Process -Id $np.Id -Force -ErrorAction SilentlyContinue }
    Get-Process notepad, Notepad -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
}

# 4. Python smoke (includes its own blocklist section) ----------------------
$py = Get-Command py -ErrorAction SilentlyContinue
if (-not $py) { $py = Get-Command python -ErrorAction SilentlyContinue }
if ($py) {
    Write-Step "Running Python MCP smoke..."
    & $py.Source (Join-Path $root "tests\mcp_rust_test.py")
    if ($LASTEXITCODE -ne 0) { Write-Fail "python smoke failed"; exit 1 }
    Write-Pass "Python smoke OK"
}
else {
    Write-Skip "Python not found; skipped tests\mcp_rust_test.py"
}

# Cleanup -------------------------------------------------------------------
if (-not $KeepArtifacts) {
    Remove-Item -Recurse -Force $outDir -ErrorAction SilentlyContinue
}
else {
    Write-Host "  Artifacts kept in $outDir" -ForegroundColor Gray
}

Write-Host "`n✨ Redaction smoke passed!`n" -ForegroundColor Green
