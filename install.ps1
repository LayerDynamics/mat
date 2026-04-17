<#
.SYNOPSIS
    Installs `mat` on Windows.

.DESCRIPTION
    Resolution order:
      1. If a prebuilt mat-<tag>-x86_64-pc-windows-msvc.zip is published to the
         GitHub release tagged $env:MAT_VERSION (default: latest), download,
         verify against SHA256SUMS.txt, and install it.
      2. Otherwise, build from source via `cargo build --release` (requires
         the Rust toolchain — install it from https://rustup.rs first, or set
         $env:INSTALL_RUST="1" to bootstrap rustup automatically).

.EXAMPLE
    PS> iwr -useb https://raw.githubusercontent.com/LayerDynamics/mat/main/install.ps1 | iex

.EXAMPLE
    PS> $env:PREFIX="C:\tools\mat"; .\install.ps1

.NOTES
    Knobs (1:1 with install.sh):
      $env:MAT_REPO           github org/repo (default LayerDynamics/mat)
      $env:MAT_VERSION        release tag to fetch (default: latest)
      $env:MAT_BRANCH         branch to build from when source-building (default: master)
      $env:INSTALL_RUST       bootstrap rustup if cargo is missing (default 0)
      $env:PREFIX             install prefix; binary goes to $PREFIX\bin
      $env:FORCE_SOURCE       skip prebuilt download, always build from source
      $env:MAT_SKIP_CHECKSUM  disable SHA256 verification (default 0; NOT recommended)
#>

# Write-Host is the correct tool for an interactive installer: it targets the
# Information stream (PS 5.0+), supports -ForegroundColor for the colored
# status lines that mirror install.sh's ANSI output, and won't pollute any
# pipeline that captures stdout. Silence the analyzer rule at script scope,
# not per call — this is a deliberate architectural choice, not an oversight.
[Diagnostics.CodeAnalysis.SuppressMessageAttribute(
    'PSAvoidUsingWriteHost', '',
    Justification = 'Installer prints colored status to the user console; Write-Host is the correct API.'
)]
[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

# Pin TLS 1.2+. Windows PowerShell 5.1 defaults to TLS 1.0/1.1 in the
# underlying .NET ServicePointManager, so `Invoke-WebRequest` against
# github.com (which now requires TLS 1.2+) will silently fail. We OR in
# Tls12/Tls13 without stomping other protocols so corporate policies that
# already widened the set keep working. Equivalent to the bash installer's
# `curl --proto '=https' --tlsv1.2` — a user running `iwr | iex` must not be
# MITM'd into downloading a binary over a downgraded TLS session.
try {
    $tls12 = [System.Net.SecurityProtocolType]::Tls12
    $current = [System.Net.ServicePointManager]::SecurityProtocol
    [System.Net.ServicePointManager]::SecurityProtocol = $current -bor $tls12
    if ([System.Enum]::IsDefined([System.Net.SecurityProtocolType], 'Tls13')) {
        $tls13 = [System.Net.SecurityProtocolType]::Tls13
        [System.Net.ServicePointManager]::SecurityProtocol =
            [System.Net.ServicePointManager]::SecurityProtocol -bor $tls13
    }
} catch {
    Write-Host "[mat] warning: could not pin TLS 1.2+: $_" -ForegroundColor Yellow
}

# ---------- knobs (env-tunable) ----------------------------------------------
$repo         = if ($env:MAT_REPO)    { $env:MAT_REPO }    else { "LayerDynamics/mat" }
$version      = if ($env:MAT_VERSION) { $env:MAT_VERSION } else { "latest" }
$branch       = if ($env:MAT_BRANCH)  { $env:MAT_BRANCH }  else { "master" }
$installRust  = ($env:INSTALL_RUST       -eq "1")
$forceSource  = ($env:FORCE_SOURCE       -eq "1")
$skipChecksum = ($env:MAT_SKIP_CHECKSUM  -eq "1")
$prefix       = $env:PREFIX
$destDir      =
    if ($prefix)             { Join-Path $prefix "bin" }
    elseif ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME "bin" }
    else                     { Join-Path $HOME ".cargo\bin" }

# ---------- helpers ----------------------------------------------------------
function Write-Info($m) { Write-Host "[mat] $m" }
function Write-Ok($m)   { Write-Host "[mat] $m" -ForegroundColor Green }
function Write-Warn($m) { Write-Host "[mat] warning: $m" -ForegroundColor Yellow }
function Die($m)        { Write-Host "[mat] error: $m" -ForegroundColor Red; exit 1 }

function Initialize-Dir($p) {
    if (-not (Test-Path $p)) { New-Item -ItemType Directory -Force -Path $p | Out-Null }
}

function Resolve-LatestTag() {
    try {
        $api = "https://api.github.com/repos/$repo/releases/latest"
        $resp = Invoke-RestMethod -Uri $api -Headers @{ "User-Agent" = "mat-installer" }
        return $resp.tag_name
    } catch {
        Write-Warn "could not query latest release: $_"
        return $null
    }
}

# ---------- prebuilt path ----------------------------------------------------
function Invoke-Prebuilt() {
    if ($forceSource) { return $false }
    $tag = if ($version -eq "latest") { Resolve-LatestTag } else { $version }
    if (-not $tag) { return $false }

    $target  = "x86_64-pc-windows-msvc"
    if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { $target = "aarch64-pc-windows-msvc" }

    $asset   = "mat-$tag-$target.zip"
    $url     = "https://github.com/$repo/releases/download/$tag/$asset"
    $shaUrl  = "https://github.com/$repo/releases/download/$tag/SHA256SUMS.txt"

    Write-Info "trying prebuilt: $url"
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("mat-dl-" + [System.IO.Path]::GetRandomFileName())
    Initialize-Dir $tmp
    $zipPath = Join-Path $tmp $asset
    try {
        Invoke-WebRequest -Uri $url -OutFile $zipPath -UseBasicParsing
    } catch {
        Write-Warn "no prebuilt for $target at $tag — falling back to source"
        Remove-Item -Recurse -Force $tmp
        return $false
    }

    # Mandatory checksum verification. A SHA256SUMS.txt that is unreachable
    # or a hash that fails to match is a hard error — installing a binary we
    # cannot verify defeats the entire purpose of pinning releases, and is
    # the exact attack surface supply-chain compromise exploits. Use
    # $env:MAT_SKIP_CHECKSUM="1" to knowingly override (air-gapped mirrors,
    # release drafts); warn loudly when that happens so it never becomes
    # routine.
    if ($skipChecksum) {
        Write-Warn "MAT_SKIP_CHECKSUM=1 — integrity verification disabled by user"
    } else {
        $sumsPath = Join-Path $tmp "SHA256SUMS.txt"
        try {
            Invoke-WebRequest -Uri $shaUrl -OutFile $sumsPath -UseBasicParsing
        } catch {
            Remove-Item -Recurse -Force $tmp
            Die "could not fetch $shaUrl — refusing to install an unverified binary (set `$env:MAT_SKIP_CHECKSUM='1' to override, at your own risk)"
        }

        $expected = Select-String -Path $sumsPath -Pattern $asset -SimpleMatch | ForEach-Object {
            ($_.Line -split "\s+")[0]
        } | Select-Object -First 1
        if (-not $expected) {
            Remove-Item -Recurse -Force $tmp
            Die "checksum entry for $asset missing from SHA256SUMS.txt — refusing to install a binary that does not match $shaUrl"
        }
        $actual = (Get-FileHash -Algorithm SHA256 -Path $zipPath).Hash.ToLower()
        if ($actual -ne $expected.ToLower()) {
            Remove-Item -Recurse -Force $tmp
            Die "checksum verification FAILED for $asset (expected $expected, got $actual) — refusing to install a binary that does not match $shaUrl"
        }
    }

    Expand-Archive -LiteralPath $zipPath -DestinationPath $tmp -Force

    # Expect the same archive layout the bash installer assumes
    # (`mat-<tag>-<target>/mat.exe`) and fall back to a recursive search if
    # the release was zipped with a different prefix.
    $expectedBin = Join-Path $tmp "mat-$tag-$target\mat.exe"
    if (Test-Path $expectedBin) {
        $binSrc = $expectedBin
    } else {
        $fallback = Get-ChildItem -Path $tmp -Recurse -Filter "mat.exe" -ErrorAction SilentlyContinue |
            Select-Object -First 1
        if (-not $fallback) {
            Write-Warn "archive contents unexpected — falling back to source"
            Remove-Item -Recurse -Force $tmp
            return $false
        }
        $binSrc = $fallback.FullName
    }

    Initialize-Dir $destDir
    Copy-Item $binSrc -Destination (Join-Path $destDir "mat.exe") -Force
    Remove-Item -Recurse -Force $tmp
    Write-Ok "installed prebuilt mat ($tag, $target) → $destDir\mat.exe"
    return $true
}

# ---------- source-build path ------------------------------------------------
function Build-FromSource() {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        if ($installRust) {
            Write-Info "cargo not found — installing Rust via rustup"
            $rustupInit = Join-Path $env:TEMP "rustup-init.exe"
            Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupInit -UseBasicParsing
            & $rustupInit -y --default-toolchain stable
            $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
        } else {
            Die "cargo not found on PATH. Install Rust from https://rustup.rs or re-run with `$env:INSTALL_RUST='1'."
        }
    }

    $srcDir  = $null
    $cleanup = $false
    if ((Test-Path "Cargo.toml") -and ((Get-Content "Cargo.toml" -Raw) -match 'name\s*=\s*"mat"')) {
        $srcDir = (Get-Location).Path
        Write-Info "using local source tree: $srcDir"
    } else {
        if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
            Die "git is required to clone source — install Git from https://git-scm.com"
        }
        $srcDir = Join-Path ([System.IO.Path]::GetTempPath()) ("mat-src-" + [System.IO.Path]::GetRandomFileName())
        $cleanup = $true
        Write-Info "cloning https://github.com/$repo.git (branch: $branch) into $srcDir"
        git clone --depth 1 --branch $branch "https://github.com/$repo.git" $srcDir 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) {
            Die "git clone failed — check MAT_REPO / MAT_BRANCH / network"
        }
    }

    Push-Location $srcDir
    try {
        Write-Info "building release binary (this takes ~30s on a warm cache)"
        cargo build --release --locked
        if ($LASTEXITCODE -ne 0) { Die "cargo build failed" }
        $bin = Join-Path $srcDir "target\release\mat.exe"
        if (-not (Test-Path $bin)) { Die "build completed but $bin is missing" }
        Initialize-Dir $destDir
        Copy-Item $bin -Destination (Join-Path $destDir "mat.exe") -Force
        Write-Ok "installed mat → $destDir\mat.exe (built from source)"
    } finally {
        Pop-Location
        if ($cleanup) { Remove-Item -Recurse -Force $srcDir }
    }
}

# ---------- main -------------------------------------------------------------
if (-not (Invoke-Prebuilt)) { Build-FromSource }

# ---------- PATH setup -------------------------------------------------------
# Mirror install.sh `ensure_dir_on_path`: if $destDir isn't on the resolved
# PATH, append it to the current user's persistent Path via the Registry-
# backed `User` scope. That's the Windows analog of editing the user's
# shell rc — the change survives reboots and propagates to every new shell
# session (PowerShell, cmd, Windows Terminal, MSYS2, WSL interop) without
# touching the Machine-wide PATH (which would require admin).
#
# Honors $env:MAT_NO_PATH_UPDATE="1" for users managing PATH themselves.
$skipPathUpdate = ($env:MAT_NO_PATH_UPDATE -eq "1")

function Test-DirOnPath([string]$dir) {
    $userPath    = [Environment]::GetEnvironmentVariable("Path", "User")
    $machinePath = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $procPath    = $env:Path
    $all = @()
    foreach ($p in @($userPath, $machinePath, $procPath)) {
        if ($p) { $all += $p.Split(";") | Where-Object { $_ } }
    }
    # Windows paths are case-insensitive and tolerate trailing backslashes.
    # Normalize both sides for a clean comparison.
    $want = $dir.TrimEnd('\').ToLowerInvariant()
    foreach ($entry in $all) {
        if ($entry.TrimEnd('\').ToLowerInvariant() -eq $want) { return $true }
    }
    return $false
}

function Add-UserPath([string]$dir) {
    $current = [Environment]::GetEnvironmentVariable("Path", "User")
    if (-not $current) { $current = "" }
    # Idempotent: bail if an equivalent entry is already present in the
    # persistent User Path (case-insensitive, trailing-slash-tolerant).
    $want = $dir.TrimEnd('\').ToLowerInvariant()
    foreach ($entry in $current.Split(";") | Where-Object { $_ }) {
        if ($entry.TrimEnd('\').ToLowerInvariant() -eq $want) {
            return $false
        }
    }
    $updated = if ($current.TrimEnd(';') -eq '') { $dir } else { ($current.TrimEnd(';') + ";" + $dir) }
    [Environment]::SetEnvironmentVariable("Path", $updated, "User")
    # Also surface the entry in the *current* process so the immediately-
    # following `mat --version` check succeeds without requiring a new shell.
    $env:Path = "$dir;$env:Path"
    return $true
}

if (Test-DirOnPath $destDir) {
    Write-Ok "$destDir is already on PATH"
} elseif ($skipPathUpdate) {
    Write-Warn "$destDir is not on PATH, and MAT_NO_PATH_UPDATE=1 — skipping User PATH edit"
    Write-Host "       add this to your User PATH manually:" -ForegroundColor DarkGray
    Write-Host "           [Environment]::SetEnvironmentVariable('Path', ([Environment]::GetEnvironmentVariable('Path','User') + ';$destDir'), 'User')"
} else {
    try {
        $added = Add-UserPath $destDir
        if ($added) {
            Write-Ok "added $destDir to User PATH"
            Write-Host "       open a new PowerShell window to pick it up in other shells." -ForegroundColor DarkGray
        } else {
            Write-Ok "$destDir is already registered in User PATH"
        }
    } catch {
        Write-Warn "could not update User PATH: $_"
        Write-Host "       add this to your User PATH manually:" -ForegroundColor DarkGray
        Write-Host "           [Environment]::SetEnvironmentVariable('Path', ([Environment]::GetEnvironmentVariable('Path','User') + ';$destDir'), 'User')"
    }
}

# ---------- success ----------------------------------------------------------
$bin = Join-Path $destDir "mat.exe"
try {
    $ver = (& $bin --version 2>$null) -join ""
    if ($LASTEXITCODE -eq 0 -and $ver) { Write-Ok $ver }
} catch {
    Write-Warn "could not run $bin --version: $_"
}
Write-Host "[mat] try it: mat README.md"
