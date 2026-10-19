# Build and package Spine 3.8.99 Viewer
param(
    [string]$Configuration = "release"
)

$ErrorActionPreference = "Stop"

Write-Host "Building Spine 3.8.99 Viewer ($Configuration)..." -ForegroundColor Cyan
cargo build --$Configuration -p sv-38

$OutDir = "bin"
if (!(Test-Path $OutDir)) {
    New-Item -ItemType Directory -Path $OutDir | Out-Null
}

$TargetExe = "target\$Configuration\sv-3-8.exe"
$TargetCli = "target\$Configuration\sv-cli.exe"
$Dst1 = Join-Path $OutDir "sv-3.8.exe"
$Dst2 = Join-Path $OutDir "SpineViewer.exe"
$Dst3 = Join-Path $OutDir "sv-cli.exe"

Copy-Item -Force $TargetExe $Dst1
Copy-Item -Force $TargetExe $Dst2
Copy-Item -Force $TargetCli $Dst3

Write-Host "Created: $Dst1" -ForegroundColor Green
Write-Host "Created: $Dst2" -ForegroundColor Green
Write-Host "Created: $Dst3 (headless CLI for MCP/automation)" -ForegroundColor Green
Write-Host "Spine 3.8.99 Viewer successfully deployed to $OutDir/" -ForegroundColor Green
