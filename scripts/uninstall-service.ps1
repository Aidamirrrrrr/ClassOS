#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Uninstalls the ClassOS Agent Windows Service (T0 spec §94).

.DESCRIPTION
    1. Stops the ClassOSAgent service if running.
    2. Deletes the service registration.
    3. Removes C:\Program Files\ClassOS.
    4. By default, leaves C:\ProgramData\ClassOS (logs, device id,
       config) in place. Pass -Purge to remove it too.

.PARAMETER Purge
    Also delete C:\ProgramData\ClassOS (logs, device state, config).

.EXAMPLE
    .\scripts\uninstall-service.ps1
    .\scripts\uninstall-service.ps1 -Purge
#>

[CmdletBinding()]
param(
    [switch]$Purge
)

$ErrorActionPreference = "Stop"

$ServiceName = "ClassOSAgent"
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

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    if ($existing.Status -ne "Stopped") {
        Write-Host "Stopping service '$ServiceName'..."
        Stop-Service -Name $ServiceName -Force
    }
    Write-Host "Deleting service '$ServiceName'..."
    sc.exe delete $ServiceName | Out-Null
} else {
    Write-Host "Service '$ServiceName' is not installed."
}

if (Test-Path $InstallDir) {
    Write-Host "Removing $InstallDir ..."
    Remove-Item -Recurse -Force $InstallDir
}

if ($Purge -and (Test-Path $DataDir)) {
    Write-Host "Purging $DataDir ..."
    Remove-Item -Recurse -Force $DataDir
} elseif (Test-Path $DataDir) {
    Write-Host "Leaving $DataDir in place (pass -Purge to remove logs/state/config too)."
}

Write-Host "Uninstall complete."
