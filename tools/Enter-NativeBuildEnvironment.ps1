# Dot-source this file. It configures only the current PowerShell process.
# Reuses installed tools/dependency cache; keeps sources, outputs, and game homes local.
[CmdletBinding()]
param(
    [string]$LauncherHome = (Join-Path $env:APPDATA 'horizon-launcher')
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$toolchain = Join-Path $LauncherHome 'rust\1.99.1-x86_64-pc-windows-gnu'
$rustBin = Join-Path $toolchain 'bin'
$mingwBin = Join-Path $toolchain 'mingw64\bin'
foreach ($required in @('rustc.exe', 'cargo.exe', 'rustdoc.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $rustBin $required))) {
        throw "Required installed tool is missing: $required"
    }
}
if (-not (Test-Path -LiteralPath (Join-Path $mingwBin 'gcc.exe'))) {
    throw 'The installed MinGW C compiler is missing.'
}

$env:PATH = "$rustBin;$mingwBin;$env:PATH"
$env:RUSTC = Join-Path $rustBin 'rustc.exe'
$env:RUSTDOC = Join-Path $rustBin 'rustdoc.exe'
$env:CARGO_HOME = Join-Path $LauncherHome 'cargo'
$env:CARGO_TARGET_DIR = Join-Path $projectRoot '.local\build\core-target'
$env:CARGO_BUILD_JOBS = '4'
$env:HORIZON_LAUNCHER_NO_SELF_UPDATE = '1'
$env:HORIZON_NO_AUDIO = '1'

$scratch = Join-Path $projectRoot '.local\native-test-environment'
$env:HORIZON_HOME = Join-Path $scratch 'game'
$env:HORIZON_LAUNCHER_HOME = Join-Path $scratch 'launcher'
$env:XDG_DATA_HOME = Join-Path $scratch 'xdg-data'
$env:XDG_CONFIG_HOME = Join-Path $scratch 'xdg-config'
$env:HORIZON_LAUNCHER_TRASH = Join-Path $scratch 'trash'
$env:HORIZON_LAUNCHER_CLIPBOARD = Join-Path $scratch 'clipboard.txt'
$env:HORIZON_LAUNCHER_DESKTOP = Join-Path $scratch 'desktop'
