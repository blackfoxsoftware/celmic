<#
.SYNOPSIS
  Prepares THIS machine to load a locally built (test-signed) Celmic driver.

.DESCRIPTION
  Locally built drivers are signed with an auto-generated "WDKTestCert"
  certificate. Windows only loads them when:
    1. test signing is enabled (bcdedit /set testsigning on + reboot,
       which requires Secure Boot to be OFF), and
    2. the test certificate is trusted (Root + TrustedPublisher stores).

  This script does (2) and checks (1). It only changes boot configuration if
  you pass -EnableTestSigning. Prefer a VM for kernel driver development.

  Release builds signed through Microsoft (attestation/WHQL) need none of this.

.EXAMPLE
  # elevated PowerShell
  .\dev-setup.ps1 -EnableTestSigning
#>
#Requires -RunAsAdministrator
param(
    [ValidateSet('x64', 'ARM64')] [string] $Platform = 'x64',
    [ValidateSet('Release', 'Debug')] [string] $Configuration = 'Release',
    [switch] $EnableTestSigning
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$cer = Join-Path $root "build\$Platform\$Configuration\CelmicVAD.cer"
if (-not (Test-Path $cer)) {
    throw "Test certificate not found at $cer. Build first: .\scripts\build.ps1"
}

foreach ($store in 'Root', 'TrustedPublisher') {
    Import-Certificate -FilePath $cer -CertStoreLocation "Cert:\LocalMachine\$store" | Out-Null
    Write-Host "Trusted test certificate in LocalMachine\$store"
}

$testsigning = (bcdedit /enum '{current}') -match 'testsigning\s+Yes'
if ($testsigning) {
    Write-Host 'Test signing is ON. Install the driver from the Celmic app (Output > Install Celmic Microphone).'
    return
}

if (-not $EnableTestSigning) {
    Write-Warning @'
Test signing is OFF, so Windows will refuse the locally built driver.
Re-run with -EnableTestSigning (then reboot), or test in a VM.
Note: test mode needs Secure Boot disabled and shows a "Test Mode" watermark;
some anti-cheat software refuses to run in test mode.
'@
    return
}

bcdedit /set testsigning on | Out-Null
Write-Host 'Test signing enabled. Reboot, then install the driver from the Celmic app.'
Write-Host 'To undo later: bcdedit /set testsigning off'
