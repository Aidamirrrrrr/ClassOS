#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Устанавливает Windows-службу ClassOS Agent (спека T0 §91-93).

.DESCRIPTION
    Проверяет права администратора, находит release-бинарники, копирует их
    в Program Files, создаёт и запускает службу ClassOSAgent.

.PARAMETER SourceDir
    Каталог с release-бинарниками. По умолчанию target\release в репозитории.

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
        throw "Скрипт необходимо запускать от имени администратора."
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
    throw "Не найден classos-service.exe: '$serviceExeSrc'. Выполните cargo build --release или задайте -SourceDir."
}
if (-not (Test-Path $sessionExeSrc)) {
    throw "Не найден classos-session.exe: '$sessionExeSrc'. Выполните cargo build --release или задайте -SourceDir."
}

Write-Host "Установка ClassOS в $InstallDir ..."
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $DataDir "logs") | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $DataDir "state") | Out-Null

Copy-Item -Force $serviceExeSrc (Join-Path $InstallDir "classos-service.exe")
Copy-Item -Force $sessionExeSrc (Join-Path $InstallDir "classos-session.exe")

# Оставляем стандартным пользователям только чтение и запуск. Изменять
# установленные бинарники могут лишь привилегированные учётные записи.
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
    Write-Host "Служба '$ServiceName' уже существует; останавливаем и удаляем перед переустановкой..."
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

# Восстановление SCM: перезапуск через 5, 15 и 60 секунд.
sc.exe failure $ServiceName reset= 86400 actions= restart/5000/restart/15000/restart/60000 | Out-Null

Start-Service -Name $ServiceName

Write-Host ""
Get-Service -Name $ServiceName | Format-List Name, DisplayName, Status, StartType
