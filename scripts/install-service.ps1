#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Installs the ClassOS Agent Windows Service (T0 spec §91-93).

.DESCRIPTION
    1. Verifies the script is running elevated.
    2. Locates the release binaries (classos-service.exe, classos-session.exe).
    3. Creates C:\Program Files\ClassOS and copies both binaries there.
    4. Creates the ClassOSAgent service (LocalSystem, auto-start).
    5. Starts the service.
    6. Prints service status.

.PARAMETER SourceDir
    Directory containing the built release binaries. Defaults to
    target\release relative to the repository root (two levels up from
    this script).

.EXAMPLE
    cargo build --release
    .\scripts\install-service.ps1
#>

[CmdletBinding()]
param(
    [string]$SourceDir
)

$ErrorActionPreference = "Stop"

$ServiceName = "ClassOSAgent"
$DisplayName = "ClassOS Agent"
$Description = "ClassOS classroom management agent."
$InstallDir = "C:\Program Files\ClassOS"
$DataDir = "C:\ProgramData\ClassOS"

function Assert-Admin {
    $currentPrincipal = New-Object Security.Principal.WindowsPrincipal(
        [Security.Principal.WindowsIdentity]::GetCurrent()
    )
    if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "This script must be run as Administrator."
    }
}

Assert-Admin

if (-not $SourceDir) {
    $repoRoot = Split-Path -Parent $PSScriptRoot
    $SourceDir = Join-Path $repoRoot "target\release"
}

$serviceExeSrc = Join-Path $SourceDir "classos-service.exe"
$sessionExeSrc = Join-Path $SourceDir "classos-session.exe"

if (-not (Test-Path $serviceExeSrc)) {
    throw "classos-service.exe not found at '$serviceExeSrc'. Run 'cargo build --release' first, or pass -SourceDir."
}
if (-not (Test-Path $sessionExeSrc)) {
    throw "classos-session.exe not found at '$sessionExeSrc'. Run 'cargo build --release' first, or pass -SourceDir."
}

Write-Host "Installing ClassOS to $InstallDir ..."
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $DataDir "logs") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $DataDir "state") | Out-Null

Copy-Item -Force $serviceExeSrc (Join-Path $InstallDir "classos-service.exe")
Copy-Item -Force $sessionExeSrc (Join-Path $InstallDir "classos-session.exe")

# Restrict Program Files\ClassOS to Read & Execute for standard users
# (spec §137-138): only privileged identities may modify installed
# binaries, so a student session can never tamper with what gets launched.
$acl = Get-Acl $InstallDir
$acl.SetAccessRuleProtection($true, $false)
$adminRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    "BUILTIN\Administrators", "FullControl", "ContainerInherit,ObjectInherit", "None", "Allow"
)
$systemRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    "NT AUTHORITY\SYSTEM", "FullControl", "ContainerInherit,ObjectInherit", "None", "Allow"
)
$usersRule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    "BUILTIN\Users", "ReadAndExecute", "ContainerInherit,ObjectInherit", "None", "Allow"
)
$acl.AddAccessRule($adminRule)
$acl.AddAccessRule($systemRule)
$acl.AddAccessRule($usersRule)
Set-Acl $InstallDir $acl

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host "Service '$ServiceName' already exists; stopping and removing before reinstall..."
    if ($existing.Status -ne "Stopped") {
        Stop-Service -Name $ServiceName -Force
    }
    sc.exe delete $ServiceName | Out-Null
    Start-Sleep -Seconds 1
}

$binPath = "`"$InstallDir\classos-service.exe`" service"
New-Service -Name $ServiceName `
    -BinaryPathName $binPath `
    -DisplayName $DisplayName `
    -Description $Description `
    -StartupType Automatic | Out-Null

# SCM failure recovery (spec §95): restart 5s / 15s / 60s+.
sc.exe failure $ServiceName reset= 86400 actions= restart/5000/restart/15000/restart/60000 | Out-Null

Start-Service -Name $ServiceName

Write-Host ""
Get-Service -Name $ServiceName | Format-List Name, DisplayName, Status, StartType
