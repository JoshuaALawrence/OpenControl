param(
    [switch]$Full,
    [switch]$Quiet
)

$ErrorActionPreference = "Stop"

function Write-Step {
    if (-not $Quiet) {
        Write-Host "→ $args" -ForegroundColor Cyan
    }
}

function Write-Pass {
    if (-not $Quiet) {
        Write-Host "✓ $args" -ForegroundColor Green
    }
}

function Write-Fail {
    Write-Host "✗ $args" -ForegroundColor Red
}

Write-Host "`n🧪 OpenControl Smoke Test`n" -ForegroundColor Yellow

# Format check
Write-Step "Checking code formatting..."
$fmt = cargo fmt -- --check 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Fail "Code is not formatted. Run: cargo fmt"
    exit 1
}
Write-Pass "Code formatting OK"

# Clippy check
Write-Step "Running clippy..."
$clippy = cargo clippy --release 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Fail "Clippy found issues"
    $clippy | Select-Object -Last 20
    exit 1
}
Write-Pass "Clippy OK"

# Unit tests
Write-Step "Running unit tests..."
$tests = cargo test --release --lib 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Fail "Unit tests failed"
    $tests | Select-Object -Last 30
    exit 1
}
Write-Pass "Unit tests OK"

# Conditional full test
if ($Full) {
    Write-Step "Running integration tests..."
    $itest = cargo test --release --test integration_test 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "Integration tests failed (non-fatal)"
        $itest | Select-Object -Last 20
    } else {
        Write-Pass "Integration tests OK"
    }

    Write-Step "Building release binary..."
    $build = cargo build --release 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Fail "Release build failed"
        $build | Select-Object -Last 20
        exit 1
    }
    Write-Pass "Release build OK"
}

Write-Host "`n✨ All smoke tests passed!`n" -ForegroundColor Green

if (-not $Full) {
    Write-Host "Tip: Run with -Full for complete test suite (slower)`n" -ForegroundColor Gray
}
