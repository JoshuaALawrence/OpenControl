param(
    [string]$Version,
    [switch]$Help,
    [switch]$DryRun
)

if ($Help) {
    Write-Host @"
OpenControl Release Script

USAGE:
    .\scripts\release.ps1 -Version "0.2.0" [-DryRun]

OPTIONS:
    -Version <string>   Version number (e.g., 0.2.0). Required.
    -DryRun             Show what would be done without making changes.
    -Help               Show this help message.

STEPS:
    1. Verify working directory is clean
    2. Update version in Cargo.toml
    3. Update CHANGELOG.md
    4. Run tests
    5. Create git tag
    6. Push to origin (triggers GitHub Actions release)

NOTES:
    - Must be run from project root
    - Requires git
    - GitHub Actions handles binary build and release creation
    - Manual edits to CHANGELOG.md may be needed before running

EXAMPLE:
    .\scripts\release.ps1 -Version "0.2.0"
    # Review git diff
    # GitHub Actions will automatically create release when tag is pushed
"@
    exit 0
}

if (-not $Version) {
    Write-Host "Error: -Version is required" -ForegroundColor Red
    Write-Host "Use -Help for usage information"
    exit 1
}

if (-not (Test-Path "Cargo.toml")) {
    Write-Host "Error: Cargo.toml not found. Run from project root." -ForegroundColor Red
    exit 1
}

# Validate version format
if ($Version -notmatch "^\d+\.\d+\.\d+(-[a-zA-Z0-9]+)?$") {
    Write-Host "Error: Invalid version format '$Version'. Use semantic versioning (e.g., 0.2.0)" -ForegroundColor Red
    exit 1
}

$tag = "v$Version"

function Write-Step {
    param([string]$Message)
    Write-Host "  → $Message" -ForegroundColor Cyan
}

function Write-Check {
    param([string]$Message)
    Write-Host "  ✓ $Message" -ForegroundColor Green
}

function Write-Error-Custom {
    param([string]$Message)
    Write-Host "  ✗ $Message" -ForegroundColor Red
}

Write-Host "`n📦 OpenControl Release: v$Version`n" -ForegroundColor Yellow

# Step 1: Check git status
Write-Step "Checking git status..."
$status = git status --porcelain
if ($status) {
    Write-Error-Custom "Working directory has uncommitted changes"
    git status --short
    exit 1
}
Write-Check "Working directory is clean"

# Step 2: Verify tag doesn't exist
Write-Step "Checking for existing tag..."
$existingTag = git tag -l $tag
if ($existingTag) {
    Write-Error-Custom "Tag $tag already exists"
    exit 1
}
Write-Check "Tag $tag is available"

# Step 3: Update Cargo.toml version
Write-Step "Updating Cargo.toml version..."
$cargoContent = Get-Content "Cargo.toml" -Raw
$previousVersion = $cargoContent -match 'version = "([0-9.]+)"' | Out-Null
if ($Matches) {
    $oldVersion = $Matches[1]
    $newContent = $cargoContent -replace "version = `"$oldVersion`"", "version = `"$Version`""
    if ($DryRun) {
        Write-Host "    [DRY RUN] Would update version: $oldVersion → $Version"
    } else {
        $newContent | Set-Content "Cargo.toml" -Encoding UTF8
        Write-Check "Updated Cargo.toml: $oldVersion → $Version"
    }
}

# Step 4: Test build
Write-Step "Running tests..."
if ($DryRun) {
    Write-Host "    [DRY RUN] Would run: cargo test --release"
} else {
    cargo test --release --quiet
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Custom "Tests failed"
        exit 1
    }
    Write-Check "All tests passed"
}

# Step 5: Build release binary
Write-Step "Building release binary..."
if ($DryRun) {
    Write-Host "    [DRY RUN] Would run: cargo build --release"
} else {
    cargo build --release --quiet
    if ($LASTEXITCODE -ne 0) {
        Write-Error-Custom "Build failed"
        exit 1
    }
    $exe = "target\release\OpenControl.exe"
    if (Test-Path $exe) {
        $size = (Get-Item $exe).Length / 1MB
        Write-Check "Release binary built ($($size.ToString('F2')) MB)"
    } else {
        Write-Error-Custom "Binary not found at $exe"
        exit 1
    }
}

# Step 6: Create commit with version change
Write-Step "Creating commit..."
if ($DryRun) {
    Write-Host "    [DRY RUN] Would run: git add Cargo.toml && git commit -m 'chore: release v$Version'"
} else {
    git add Cargo.toml
    git commit -m "chore: release v$Version" --quiet
    Write-Check "Commit created"
}

# Step 7: Create and push tag (triggers release workflow)
Write-Step "Creating git tag..."
if ($DryRun) {
    Write-Host "    [DRY RUN] Would run: git tag -a $tag -m 'Release $Version'"
    Write-Host "    [DRY RUN] Would run: git push origin main && git push origin $tag"
} else {
    git tag -a $tag -m "Release $Version"
    Write-Check "Tag created: $tag"

    Write-Step "Pushing to origin..."
    git push origin main --quiet
    git push origin $tag --quiet
    Write-Check "Pushed to origin"
    
    Write-Host "`n✨ Release created successfully!`n" -ForegroundColor Green
    Write-Host "📋 Next steps:" -ForegroundColor Yellow
    Write-Host "  1. GitHub Actions will automatically:"
    Write-Host "     - Build the release binary"
    Write-Host "     - Create a GitHub Release"
    Write-Host "     - Upload OpenControl.exe as release asset"
    Write-Host "  2. Monitor: https://github.com/yourusername/computer-use/actions"
    Write-Host "  3. Verify release: https://github.com/yourusername/computer-use/releases/tag/$tag"
}

if ($DryRun) {
    Write-Host "`n📝 DRY RUN COMPLETE - No changes made`n" -ForegroundColor Yellow
}
