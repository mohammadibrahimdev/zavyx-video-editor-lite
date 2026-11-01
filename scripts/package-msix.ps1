# Build the Microsoft Store package into .\dist\
#
#   ZavyxVE-<ver>-x64.msix     upload this in Partner Center; the Store signs it
#
# Usage (from the repo root):
#   powershell -ExecutionPolicy Bypass -File scripts\package-msix.ps1
#   ... -Register     also install the unpacked package on this PC for testing
#                     (needs Windows Developer Mode; remove it again from
#                     Settings > Apps, or with Remove-AppxPackage)
#
# Only x64: the bundled ffmpeg.exe is 64-bit, and the Store gives 64-bit PCs
# the x64 package. Needs the Windows SDK (makeappx.exe) and bin\ffmpeg.exe
# (scripts\package.ps1 downloads it). The Store identity values are read from
# msix\store-identity.json.

[CmdletBinding()]
param(
    [switch]$NoBuild,
    [switch]$Register
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$identity = Get-Content msix\store-identity.json -Raw | ConvertFrom-Json
Write-Host "=== Packaging Zavyx VE $version for the Microsoft Store ==="

$makeappx = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $makeappx) { throw "makeappx.exe not found: install the Windows SDK." }
if (-not (Test-Path bin\ffmpeg.exe)) { throw "bin\ffmpeg.exe missing: run scripts\package.ps1 once to download it." }

$target = "x86_64-pc-windows-msvc"
if (-not $NoBuild) {
    cargo build --release --target $target -p zavyx-ve-gui
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}

$layout = Join-Path $root "dist\msix-x64"
if (Test-Path $layout) {
    Get-AppxPackage -Name $identity.IdentityName | Where-Object InstallLocation -eq $layout | Remove-AppxPackage
    Remove-Item -Recurse -Force $layout
}
New-Item -ItemType Directory -Force -Path $layout | Out-Null
Copy-Item "target\$target\release\ZavyxVE.exe", bin\ffmpeg.exe, LICENSE, THIRD-PARTY-NOTICES.md $layout
Copy-Item msix\Assets $layout -Recurse

$manifest = (Get-Content msix\AppxManifest.xml -Raw).
    Replace("{IdentityName}", $identity.IdentityName).
    Replace("{PublisherDisplayName}", $identity.PublisherDisplayName).
    Replace("{Publisher}", $identity.Publisher).
    Replace("{Version}", "$version.0").
    Replace("{Arch}", "x64")
[IO.File]::WriteAllText((Join-Path $layout "AppxManifest.xml"), $manifest)

$msix = Join-Path $root "dist\ZavyxVE-$version-x64.msix"
& $makeappx pack /o /h SHA256 /d $layout /p $msix | Select-Object -Last 1
if ($LASTEXITCODE -ne 0) { throw "makeappx failed" }
Write-Host "Wrote $msix"

if ($Register) {
    Add-AppxPackage -Register (Join-Path $layout "AppxManifest.xml")
    Write-Host "Installed for testing: open 'Zavyx Video Editor' from the Start menu."
}
