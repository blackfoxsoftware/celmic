<#
.SYNOPSIS
  Builds the Celmic virtual microphone driver package (CelmicVAD.sys/.inf/.cat).

.DESCRIPTION
  Restores the WDK + SDK NuGet packages, locates a Visual Studio 2022 install
  with the "Windows Driver Kit" component, and builds with 64-bit MSBuild.
  Output: drivers\windows\build\<Platform>\<Configuration>\CelmicVAD\

  Prerequisite (one time):
    Visual Studio 2022 with "Desktop development with C++", the MSVC Spectre
    libraries and the "Windows Driver Kit" component
    (Component.Microsoft.Windows.DriverKit).
#>
param(
    [ValidateSet('Release', 'Debug')] [string] $Configuration = 'Release',
    [ValidateSet('x64', 'ARM64')] [string[]] $Platform = @('x64')
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

# --- NuGet ---------------------------------------------------------------
$nuget = Get-Command nuget.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source
if (-not $nuget) {
    $nuget = Join-Path $env:LOCALAPPDATA 'celmic-tools\nuget.exe'
    if (-not (Test-Path $nuget)) {
        New-Item -ItemType Directory -Force (Split-Path $nuget) | Out-Null
        Write-Host 'Downloading nuget.exe...'
        Invoke-WebRequest 'https://dist.nuget.org/win-x86-commandline/latest/nuget.exe' -OutFile $nuget
    }
}
& $nuget restore (Join-Path $root 'packages.config') -PackagesDirectory (Join-Path $root 'packages') -NonInteractive
if ($LASTEXITCODE) { throw 'nuget restore failed' }

# --- Visual Studio with the WDK component ---------------------------------
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
# The WDK integration may come from the VS component or from the WDK's VSIX
# (e.g. GitHub runners), so look for the toolset itself.
$vs = & $vswhere -products * -version '[17.0,18.0)' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath |
    Where-Object { Test-Path (Join-Path $_ 'MSBuild\Microsoft\VC\v170\Platforms\x64\PlatformToolsets\WindowsKernelModeDriver10.0') } |
    Select-Object -First 1
if (-not $vs) {
    throw @'
Visual Studio 2022 with the "Windows Driver Kit" component was not found.
Install it with:
  winget install Microsoft.VisualStudio.2022.Community --override "--passive --add Microsoft.VisualStudio.Workload.NativeDesktop --add Microsoft.VisualStudio.Component.VC.Runtimes.x86.x64.Spectre --add Component.Microsoft.Windows.DriverKit"
'@
}
# 64-bit MSBuild: the 32-bit one looks for WDK tools in x86 folders the NuGet kit doesn't ship.
$msbuild = Join-Path $vs 'MSBuild\Current\Bin\amd64\MSBuild.exe'

foreach ($p in $Platform) {
    Write-Host "Building CelmicVAD $Configuration|$p"
    & $msbuild (Join-Path $root 'CelmicVAD.vcxproj') `
        -p:Configuration=$Configuration -p:Platform=$p `
        -nodeReuse:false -nologo -v:minimal
    if ($LASTEXITCODE) { throw "build failed for $p" }
    Write-Host "Package: $(Join-Path $root "build\$p\$Configuration\CelmicVAD")"
}
