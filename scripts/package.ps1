# Build every Zavyx VE release file into .\dist\
#
#   ZavyxVE-Setup-<ver>-x64.exe        installer, 64-bit Windows (needs Inno Setup's iscc)
#   ZavyxVE-Setup-<ver>-x86.exe        installer, 32-bit build for older PCs
#   ZavyxVE-<ver>-x64-portable.zip     no-install version, 64-bit
#   ZavyxVE-<ver>-x86-portable.zip     no-install version, 32-bit
#
# Usage (from the repo root):
#   powershell -ExecutionPolicy Bypass -File scripts\package.ps1
#
# ffmpeg.exe is bundled from .\bin\. If it is missing it is downloaded from
# gyan.dev (skip with -NoTools). It is a 64-bit program: the 32-bit app uses
# it on 64-bit Windows; on 32-bit Windows put a 32-bit ffmpeg.exe next to it.

[CmdletBinding()]
param(
    [switch]$NoTools,
    [switch]$NoInstaller,
    # Windows 7 builds (Rust's *-win7-windows-msvc targets; needs nightly with rust-src).
    [switch]$Win7
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "=== Packaging Zavyx VE $version ==="

$bin = Join-Path $root "bin"
New-Item -ItemType Directory -Force -Path $bin | Out-Null
if (-not $NoTools -and -not (Test-Path "$bin\ffmpeg.exe")) {
    Write-Host "Downloading ffmpeg..."
    $zip = Join-Path $env:TEMP "ffmpeg-essentials.zip"
    Invoke-WebRequest "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip" -OutFile $zip
    $tmp = Join-Path $env:TEMP "ffmpeg-essentials"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Archive $zip $tmp
    Copy-Item (Get-ChildItem $tmp -Recurse -Filter ffmpeg.exe | Select-Object -First 1).FullName "$bin\ffmpeg.exe"
}

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$iscc = (Get-Command iscc -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
    $iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe", "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") |
        Where-Object { Test-Path $_ } | Select-Object -First 1
}

foreach ($arch in "x64", "x86") {
    $cpu = if ($arch -eq "x64") { "x86_64" } else { "i686" }
    $label = if ($Win7) { "win7-$arch" } else { $arch }
    $target = if ($Win7) { "$cpu-win7-windows-msvc" } else { "$cpu-pc-windows-msvc" }
    Write-Host "=== Building $label ($target) ==="
    if ($Win7) { cargo +nightly build -Z build-std=std,panic_abort --release --target $target }
    else { cargo build --release --target $target }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $target" }

    $staging = Join-Path $dist "staging-$label"
    if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
    New-Item -ItemType Directory -Force -Path $staging | Out-Null
    foreach ($exe in "ZavyxVE.exe", "zavyx-ve-cli.exe") {
        Copy-Item "target\$target\release\$exe" $staging
    }
    if (Test-Path "$bin\ffmpeg.exe") { Copy-Item "$bin\ffmpeg.exe" $staging }
    else { Write-Warning "bin\ffmpeg.exe missing: the app will not open media without it." }
    Copy-Item LICENSE-MIT, LICENSE-APACHE, THIRD-PARTY-NOTICES.md $staging

    $portable = Join-Path $dist "ZavyxVE-$version-$label-portable.zip"
    Remove-Item $portable -ErrorAction SilentlyContinue
    Compress-Archive -Path "$staging\*" -DestinationPath $portable
    Write-Host "Wrote $portable"

    if (-not $NoInstaller) {
        if ($iscc) {
            & $iscc "/DAppVersion=$version" "/DArch=$arch" "/DLabel=$label" installer\zavyx-ve.iss
            if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
            Write-Host "Wrote dist\ZavyxVE-Setup-$version-$label.exe"
        } else {
            Write-Warning "Inno Setup (iscc) not found; skipped the $arch installer. GitHub Actions builds it on release."
        }
    }
}
