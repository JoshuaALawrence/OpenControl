@echo off
REM Install.cmd - OpenControl Installer Wrapper
REM Double-click this file to install OpenControl
REM Requires Windows 10/11

setlocal enabledelayedexpansion

REM Check for PowerShell
where pwsh >nul 2>&1
if errorlevel 1 (
    REM Try PowerShell 5 (built-in to Windows)
    where powershell >nul 2>&1
    if errorlevel 1 (
        echo PowerShell not found!
        echo This script requires PowerShell (built-in to Windows 10/11)
        pause
        exit /b 1
    )
    set PS_EXE=powershell
) else (
    set PS_EXE=pwsh
)

REM Run the PowerShell installer
%PS_EXE% -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install.ps1"
