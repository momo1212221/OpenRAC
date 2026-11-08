$ErrorActionPreference = "Stop"

Write-Host "OpenRAC Ultra - prepared Windows data installer"
Write-Host ""

$packageRoot = Split-Path -Parent $PSScriptRoot
$sourceData = Join-Path $packageRoot "PreparedData\active\rac1\data"
if (-not (Test-Path $sourceData)) {
    throw "PreparedData was not found next to this script."
}

$defaultRoot = "C:\OpenRAC"
$answer = Read-Host "OpenRAC game data folder [$defaultRoot]"
$installRoot = if ([string]::IsNullOrWhiteSpace($answer)) { $defaultRoot } else { $answer }
$targetData = Join-Path $installRoot "active\rac1\data"

New-Item -ItemType Directory -Force -Path $targetData | Out-Null

function Copy-Tree([string]$name) {
    $src = Join-Path $sourceData $name
    $dst = Join-Path $targetData $name
    if (-not (Test-Path $src)) { return }
    New-Item -ItemType Directory -Force -Path $dst | Out-Null
    Write-Host "Copying $name ..."
    robocopy $src $dst /E /COPY:DAT /DCOPY:T /R:2 /W:1 /NFL /NDL /NP | Out-Host
    if ($LASTEXITCODE -ge 8) {
        throw "robocopy failed for $name with exit code $LASTEXITCODE"
    }
}

Copy-Tree "iso_data"
Copy-Tree "port\pal"
Copy-Tree "probes\pal"
Copy-Tree "texture_packs\rac1-pal"

$settingsSource = Join-Path $packageRoot "PreparedData\settings\rac1-pal.ini"
$settingsDir = Join-Path $env:LOCALAPPDATA "openrac\settings"
New-Item -ItemType Directory -Force -Path $settingsDir | Out-Null
Copy-Item -Force $settingsSource (Join-Path $settingsDir "rac1-pal.ini")

Write-Host ""
Write-Host "Prepared data installed to: $targetData"
Write-Host "Graphics settings installed to: $settingsDir"
Write-Host ""
Write-Host "Next:"
Write-Host "1. Install the OpenRAC Windows installer from WindowsInstaller."
Write-Host "2. Start OpenRAC Launcher."
Write-Host "3. If asked for the game/install folder, choose: $installRoot"
Write-Host "4. Use Vulkan. NVIDIA RTX 20/30 GPUs are preferred automatically by the game."
Write-Host ""
Write-Host "The Ultra4K textures and probe lighting are already pre-rendered; no runtime texture upscale is required."
Read-Host "Press Enter to close"
