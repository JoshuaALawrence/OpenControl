@echo off
setlocal enabledelayedexpansion

REM OpenControl build script for Windows
REM Usage: Build [debug|release|test|clean]

set BUILD_TYPE=release
if not "%1"=="" set BUILD_TYPE=%1

if "%BUILD_TYPE%"=="clean" (
    echo Cleaning build artifacts...
    cargo clean
    goto :end
)

if "%BUILD_TYPE%"=="test" (
    echo Running tests...
    cargo test --release --all-features
    if errorlevel 1 goto :error
    echo Running integration tests...
    cargo test --release --test integration_test
    if errorlevel 1 goto :error
    echo All tests passed!
    goto :end
)

if "%BUILD_TYPE%"=="debug" (
    echo Building debug binary...
    cargo build
    if errorlevel 1 goto :error
    echo Debug build complete: target\debug\OpenControl.exe
    goto :end
)

if "%BUILD_TYPE%"=="release" (
    echo Building release binary...
    cargo build --release
    if errorlevel 1 goto :error
    
    set EXE=target\release\OpenControl.exe
    if exist !EXE! (
        for /F "usebackq" %%A in ('!EXE!') do set SIZE=%%~zA
        set /a SIZE_MB=!SIZE! / 1048576
        echo Release build complete: !EXE! (!SIZE_MB! MB)
    ) else (
        echo Error: Binary not found at !EXE!
        goto :error
    )
    goto :end
)

echo Unknown build type: %BUILD_TYPE%
echo.
echo Usage: Build [debug^|release^|test^|clean]
echo.
echo Options:
echo   debug   - Build debug binary
echo   release - Build optimized release binary (default)
echo   test    - Run all tests
echo   clean   - Remove build artifacts
exit /b 1

:error
echo Build failed!
exit /b 1

:end
exit /b 0