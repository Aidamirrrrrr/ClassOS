#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Удаляет Windows-службу ClassOS Agent (спека T0 §94).

.DESCRIPTION
    Останавливает службу, удаляет регистрацию и каталог программы.
    ProgramData сохраняется, если не передан параметр -Purge.

.PARAMETER Purge
    Также удалить журналы, состояние устройства и конфигурацию из ProgramData.

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
        throw "Скрипт необходимо запускать от имени администратора."
    }
}

Assert-Admin

$existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($existing) {
    if ($existing.Status -ne "Stopped") {
        Write-Host "Остановка службы '$ServiceName'..."
        Stop-Service -Name $ServiceName -Force
    }
    Write-Host "Удаление службы '$ServiceName'..."
    sc.exe delete $ServiceName | Out-Null
} else {
    Write-Host "Служба '$ServiceName' не установлена."
}

if (Test-Path $InstallDir) {
    Write-Host "Удаление $InstallDir ..."
    Remove-Item -Recurse -Force $InstallDir
}

if ($Purge -and (Test-Path $DataDir)) {
    Write-Host "Полное удаление $DataDir ..."
    Remove-Item -Recurse -Force $DataDir
} elseif (Test-Path $DataDir) {
    Write-Host "$DataDir сохранён. Передайте -Purge для удаления журналов, состояния и конфигурации."
}

Write-Host "Удаление завершено."
