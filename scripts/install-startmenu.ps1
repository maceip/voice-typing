#Requires -Version 5.1
[CmdletBinding()]
param(
    [string]$SourceExe,
    [string]$SourceIcon,
    [string]$AppName = 'voice-typing'
)

$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSCommandPath | Split-Path -Parent
if (-not $SourceExe)  { $SourceExe  = Join-Path $RepoRoot 'target\release\voice-typing.exe' }
if (-not $SourceIcon) { $SourceIcon = Join-Path $RepoRoot 'assets\icons\windows\daydream.ico' }

$SourceExe = (Resolve-Path $SourceExe).Path
$SourceIcon = (Resolve-Path $SourceIcon).Path

$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName"
$StartMenuDir = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$ShortcutPath = Join-Path $StartMenuDir "$AppName.lnk"

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$DestExe = Join-Path $InstallDir 'voice-typing.exe'
$DestIcon = Join-Path $InstallDir 'daydream.ico'

Copy-Item -Force $SourceExe  $DestExe
Copy-Item -Force $SourceIcon $DestIcon

$WshShell = New-Object -ComObject WScript.Shell
$Shortcut = $WshShell.CreateShortcut($ShortcutPath)
$Shortcut.TargetPath       = $DestExe
$Shortcut.WorkingDirectory = $InstallDir
$Shortcut.IconLocation     = "$DestIcon,0"
$Shortcut.Description      = 'Fast local voice typing'
$Shortcut.Save()

Write-Host "Installed to:  $InstallDir"
Write-Host "Shortcut:      $ShortcutPath"
