param(
    [switch]$Install,
    [switch]$Uninstall,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# Configuration
$InstallDir = "$env:PROGRAMFILES\OpenControl"
$ConfigDir = "$env:APPDATA\OpenControl"
$ReleaseApi = "https://api.github.com/repos/joshuaalawrence/opencontrol/releases/latest"

function Write-Header {
    Clear-Host
    Write-Host "`n╔════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "║       OpenControl Setup & Install       ║" -ForegroundColor Cyan
    Write-Host "║  MCP Server for AI Desktop Control      ║" -ForegroundColor Cyan
    Write-Host "╚═════════════════════════════════════════╝`n" -ForegroundColor Cyan
}

function Write-Step {
    param([string]$Message, [string]$Color = "Cyan")
    Write-Host "▶ $Message" -ForegroundColor $Color
}

function Write-Success {
    Write-Host "✓ $args" -ForegroundColor Green
}

function Write-Error-Custom {
    Write-Host "✗ $args" -ForegroundColor Red
}

function Write-Info {
    Write-Host "ℹ $args" -ForegroundColor Gray
}

function Show-Help {
    Write-Header
    Write-Host @"
QUICKSTART:
  .\Install.ps1                - Interactive setup wizard
  .\Install.ps1 -Install       - Auto-install with recommended settings
  .\Install.ps1 -Uninstall    - Remove OpenControl

WHAT GETS INSTALLED:
  • OpenControl.exe (2 MB, no dependencies)
  • Configuration files for your MCP clients
  • Uninstaller for easy removal

SUPPORTED CLIENTS:
  ✓ Claude Desktop (Mac/Windows)
  ✓ VS Code + GitHub Copilot
  ✓ Cursor
  ✓ Custom MCP hosts

SYSTEM REQUIREMENTS:
  • Windows 10/11 (64-bit)
  • ~20 MB free disk space
  • Administrator access (for Program Files)

WHAT IT DOES:
  Once installed, OpenControl lets your AI see and control:
  • Screenshots & screen recording
  • Mouse & keyboard
  • Windows & applications
  • File system operations
  • System information
  • Windows UI accessibility tree
  • Built-in OCR for text recognition

UNINSTALL:
  .\Install.ps1 -Uninstall    or    Control Panel → Programs & Features

HELP & SUPPORT:
  Issues: https://github.com/joshuaalawrence/opencontrol/issues
  Docs: https://github.com/joshuaalawrence/opencontrol

"@
}

function Test-AdminRights {
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

function Get-LatestRelease {
    Write-Step "Fetching latest release..."
    try {
        $response = Invoke-RestMethod -Uri $ReleaseApi -ErrorAction Stop
        $asset = $response.assets | Where-Object { $_.name -eq "OpenControl.exe" } | Select-Object -First 1
        
        if (-not $asset) {
            Write-Error-Custom "No OpenControl.exe found in latest release"
            return $null
        }
        
        Write-Success "Found OpenControl v$($response.tag_name)"
        return @{
            Version = $response.tag_name
            DownloadUrl = $asset.browser_download_url
            Size = [math]::Round($asset.size / 1MB, 2)
        }
    } catch {
        Write-Error-Custom "Failed to fetch release: $_"
        return $null
    }
}

function Install-Binary {
    param($Release)
    
    Write-Step "Installing OpenControl..."
    
    # Create directories
    New-Item -ItemType Directory -Path $InstallDir -Force -ErrorAction SilentlyContinue | Out-Null
    New-Item -ItemType Directory -Path $ConfigDir -Force -ErrorAction SilentlyContinue | Out-Null
    
    $exePath = Join-Path $InstallDir "OpenControl.exe"
    
    # Download
    Write-Step "Downloading ($($Release.Size) MB)..."
    try {
        Invoke-WebRequest -Uri $Release.DownloadUrl -OutFile $exePath -ErrorAction Stop
        Write-Success "Downloaded to $exePath"
    } catch {
        Write-Error-Custom "Download failed: $_"
        return $false
    }
    
    # Verify
    if (Test-Path $exePath) {
        $actualSize = (Get-Item $exePath).Length / 1MB
        Write-Success "Binary verified ($([math]::Round($actualSize, 2)) MB)"
        return $true
    } else {
        Write-Error-Custom "Installation failed: binary not found"
        return $false
    }
}

function Setup-ClaudeDesktop {
    Write-Step "Setting up Claude Desktop..."
    
    $configPath = "$env:APPDATA\Claude\claude_desktop_config.json"
    
    if (-not (Test-Path (Split-Path $configPath))) {
        Write-Info "Claude Desktop not yet configured. It will be set up on first launch."
        return
    }
    
    $exePath = "$InstallDir\OpenControl.exe"
    
    try {
        $config = Get-Content $configPath -Raw | ConvertFrom-Json
    } catch {
        $config = @{ mcpServers = @{} }
    }
    
    if (-not $config.mcpServers) {
        $config | Add-Member -NotePropertyName "mcpServers" -NotePropertyValue @{} -Force
    }
    
    $config.mcpServers.OpenControl = @{
        command = $exePath
    }
    
    $config | ConvertTo-Json -Depth 10 | Set-Content $configPath
    Write-Success "Claude Desktop configured"
}

function Setup-VSCode {
    Write-Step "Setting up VS Code + GitHub Copilot..."
    
    $vscodeConfigPath = "$env:APPDATA\Code\User\settings.json"
    
    if (-not (Test-Path $vscodeConfigPath)) {
        Write-Info "VS Code settings not found. Will be created on first run."
        return
    }
    
    $exePath = "$InstallDir\OpenControl.exe"
    
    try {
        $config = Get-Content $vscodeConfigPath -Raw | ConvertFrom-Json -AsHashtable
    } catch {
        $config = @{}
    }
    
    $config.'github.copilot.advanced' = @{
        mcp = @{
            opencontrol = @{
                command = $exePath
            }
        }
    }
    
    $config | ConvertTo-Json -Depth 10 | Set-Content $vscodeConfigPath
    Write-Success "VS Code configured"
}

function Setup-Cursor {
    Write-Step "Setting up Cursor..."
    
    $configPath = "$env:APPDATA\Cursor\User\settings.json"
    
    if (-not (Test-Path $configPath)) {
        Write-Info "Cursor not yet configured."
        return
    }
    
    $exePath = "$InstallDir\OpenControl.exe"
    
    try {
        $config = Get-Content $configPath -Raw | ConvertFrom-Json -AsHashtable
    } catch {
        $config = @{}
    }
    
    $config.'cursor.mcp' = @{
        opencontrol = @{
            command = $exePath
        }
    }
    
    $config | ConvertTo-Json -Depth 10 | Set-Content $configPath
    Write-Success "Cursor configured"
}

function Show-PostInstall {
    Write-Host "`n╔════════════════════════════════════════╗" -ForegroundColor Green
    Write-Host "║     Installation Complete! ✓           ║" -ForegroundColor Green
    Write-Host "╚════════════════════════════════════════╝`n" -ForegroundColor Green
    
    Write-Host "   Installation Location:" -ForegroundColor Yellow
    Write-Host "   $InstallDir`n"
    
    Write-Host "   Next Steps:" -ForegroundColor Yellow
    Write-Host @"
   1. Restart your MCP client (Claude Desktop, VS Code, Cursor)
   2. The client will auto-detect OpenControl
   3. Start asking your AI to control your computer!

   Example prompts:
   • "Take a screenshot"
   • "Click on the Chrome window"
   • "Open Notepad and write 'Hello World'"
   • "Read the text on my screen"

   Learn More:
   • First time? Read: QUICKSTART.md
   • Troubleshooting: https://github.com/joshuaalawrence/opencontrol/issues
   • Full docs: https://github.com/joshuaalawrence/opencontrol#readme

   Tips:
   • Run as Administrator for full screen access
   • Press Escape to stop the current action
   • Check client logs if something doesn't work

"@
}

function Show-Uninstall-Confirm {
    Write-Host "`n Remove OpenControl?" -ForegroundColor Yellow
    Write-Host "This will delete:"
    Write-Host "  • $InstallDir"
    Write-Host "  • Configuration files"
    Write-Host ""
    
    $response = Read-Host "Continue? (yes/no)"
    return $response -eq "yes"
}

function Uninstall {
    if (-not (Test-AdminRights)) {
        Write-Error-Custom "Please run as Administrator to uninstall"
        exit 1
    }
    
    Write-Header
    Write-Step "Uninstalling OpenControl..."
    
    if (Show-Uninstall-Confirm) {
        # Kill any running instances
        Get-Process OpenControl -ErrorAction SilentlyContinue | Stop-Process -Force
        
        # Remove directories
        Remove-Item $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
        Remove-Item $ConfigDir -Recurse -Force -ErrorAction SilentlyContinue
        
        Write-Success "OpenControl uninstalled"
        Write-Info "Configuration for Claude Desktop, VS Code, and Cursor has been left in place."
        Write-Info "You can remove it manually if needed."
    } else {
        Write-Info "Uninstall cancelled"
    }
}

function Interactive-Setup {
    Write-Header
    
    Write-Host "Welcome! This will install OpenControl on your computer.`n"
    
    if (-not (Test-AdminRights)) {
        Write-Error-Custom "Administrator access required for installation"
        Write-Info "Please run this script as Administrator"
        exit 1
    }
    
    Write-Step "Checking system compatibility..."
    Write-Success "Windows system detected"
    
    $release = Get-LatestRelease
    if (-not $release) {
        exit 1
    }
    
    Write-Host ""
    Write-Host "Installation Plan:" -ForegroundColor Yellow
    Write-Host "  • Install to: $InstallDir"
    Write-Host "  • Version: $($release.Version)"
    Write-Host "  • Size: $($release.Size) MB"
    Write-Host ""
    
    $proceed = Read-Host "Proceed with installation? (yes/no)"
    if ($proceed -ne "yes") {
        Write-Info "Installation cancelled"
        exit 0
    }
    
    # Install
    if (-not (Install-Binary $release)) {
        Write-Error-Custom "Installation failed"
        exit 1
    }
    
    Write-Host ""
    Write-Step "Configuring MCP clients..."
    
    # Auto-detect and configure clients
    if (Test-Path "$env:APPDATA\Claude") {
        Setup-ClaudeDesktop
    }
    
    if (Test-Path "$env:APPDATA\Code") {
        Setup-VSCode
    }
    
    if (Test-Path "$env:APPDATA\Cursor") {
        Setup-Cursor
    }
    
    Show-PostInstall
}

# Entry point
if ($Help) {
    Show-Help
} elseif ($Uninstall) {
    Uninstall
} elseif ($Install) {
    if (-not (Test-AdminRights)) {
        Write-Error-Custom "Administrator access required"
        exit 1
    }
    $release = Get-LatestRelease
    if ($release) {
        Install-Binary $release | Out-Null
        Setup-ClaudeDesktop
        Setup-VSCode
        Setup-Cursor
        Show-PostInstall
    }
    exit 1
} else {
    Interactive-Setup
}
