#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$Version = '0.2.0',
    [string]$Configuration = 'release',
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSCommandPath | Split-Path -Parent
$BinPath  = Join-Path $RepoRoot "target\$Configuration\voice-typing.exe"
$IconPath = Join-Path $RepoRoot 'assets\icons\windows\daydream.ico'
$Wxs      = Join-Path $RepoRoot 'wix\voice-typing.wxs'
$OutDir   = Join-Path $RepoRoot 'target\wix'
$OutMsi   = Join-Path $OutDir "voice-typing-$Version-x64.msi"

if (-not $SkipBuild) {
    Push-Location $RepoRoot
    try {
        $cargoArgs = @('build', '--package', 'voice-typing')
        if ($Configuration -eq 'release') {
            $cargoArgs += '--release'
        } elseif ($Configuration -ne 'debug') {
            $cargoArgs += @('--profile', $Configuration)
        }

        cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally {
        Pop-Location
    }
}

foreach ($p in @($BinPath, $IconPath, $Wxs)) {
    if (-not (Test-Path $p)) { throw "Missing required file: $p" }
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

& wix eula accept wix7 | Out-Null
if ($LASTEXITCODE -ne 0) { throw "wix eula acceptance failed with exit code $LASTEXITCODE" }

$wixArgs = @(
    'build',
    '-arch', 'x64',
    '-d', "ProductVersion=$Version",
    '-d', "BinPath=$BinPath",
    '-d', "IconPath=$IconPath",
    '-o', $OutMsi,
    $Wxs
)

Write-Host "wix $($wixArgs -join ' ')"
& wix @wixArgs
if ($LASTEXITCODE -ne 0) { throw "wix build failed with exit code $LASTEXITCODE" }

Write-Host ""
Write-Host "MSI built: $OutMsi"
