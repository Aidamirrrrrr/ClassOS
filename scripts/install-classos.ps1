<#
.SYNOPSIS
    Установщик ClassOS Agent (spec T8 §9).

.DESCRIPTION
    Проверяет версию Windows, устанавливает бинарники, регистрирует службу,
    настраивает восстановление службы и правила брандмауэра, выполняет
    enrollment и запускает службу.

    Скрипт обязан быть подписан Authenticode перед реальным развёртыванием:
    неподписанный установщик — блокирующий пункт чеклиста §10, а не
    пожелание. Подпись выполняется в релизном пайплайне и здесь не
    подменяется чем-то другим.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$EnrollmentCode,
    [string]$SourceDir = (Join-Path $PSScriptRoot "..\target\release"),
    [string]$InstallDir = "C:\Program Files\ClassOS",
    [switch]$SkipFirewall
)

$ErrorActionPreference = "Stop"
$ServiceName = "ClassOSAgent"
$DiscoveryPort = 45900
$ControlPort = 45901

function Assert-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Установка требует прав администратора."
    }
}

function Assert-WindowsVersion {
    # Агент рассчитан на Windows 10 1809+/11: ниже нет части используемых API.
    $build = [Environment]::OSVersion.Version.Build
    if ($build -lt 17763) {
        throw "Требуется Windows 10 1809 или новее (текущая сборка: $build)."
    }
    Write-Host "Windows build $build — поддерживается."
}

function Install-Binaries {
    if (-not (Test-Path $SourceDir)) {
        throw "Каталог с бинарниками не найден: $SourceDir"
    }
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    foreach ($name in @("classos-service.exe", "classos-session.exe")) {
        $source = Join-Path $SourceDir $name
        if (-not (Test-Path $source)) { throw "Не найден $name в $SourceDir" }
        Copy-Item $source (Join-Path $InstallDir $name) -Force
    }
    Write-Host "Бинарники установлены в $InstallDir."
}

function Register-Service {
    $binary = Join-Path $InstallDir "classos-service.exe"
    $existing = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
    if ($existing) {
        Write-Host "Служба уже зарегистрирована — обновляем путь."
        & sc.exe config $ServiceName binPath= "`"$binary`" service" start= auto | Out-Null
    } else {
        & sc.exe create $ServiceName binPath= "`"$binary`" service" start= auto DisplayName= "ClassOS Agent" | Out-Null
    }
    # Восстановление после сбоя: служба должна подниматься сама, а не ждать
    # человека в кабинете (T0 §85).
    & sc.exe failure $ServiceName reset= 86400 actions= restart/5000/restart/10000/restart/30000 | Out-Null
    & sc.exe description $ServiceName "Управление компьютерным классом ClassOS" | Out-Null
    Write-Host "Служба $ServiceName зарегистрирована."
}

function Set-FirewallRules {
    if ($SkipFirewall) {
        Write-Host "Правила брандмауэра пропущены по ключу -SkipFirewall."
        return
    }
    # Только конкретные порты агента; общего разрешения для программы не даём.
    foreach ($rule in @(
        @{ Name = "ClassOS Discovery"; Port = $DiscoveryPort; Protocol = "UDP" },
        @{ Name = "ClassOS Control"; Port = $ControlPort; Protocol = "TCP" }
    )) {
        Get-NetFirewallRule -DisplayName $rule.Name -ErrorAction SilentlyContinue |
            Remove-NetFirewallRule -ErrorAction SilentlyContinue
        New-NetFirewallRule -DisplayName $rule.Name -Direction Inbound -Action Allow `
            -Protocol $rule.Protocol -LocalPort $rule.Port -Profile Domain,Private | Out-Null
    }
    Write-Host "Правила брандмауэра настроены (UDP $DiscoveryPort, TCP $ControlPort)."
}

function Register-Enrollment {
    $binary = Join-Path $InstallDir "classos-service.exe"
    & $binary enroll --code $EnrollmentCode
    if ($LASTEXITCODE -ne 0) {
        throw "Не удалось сохранить enrollment-код (код возврата $LASTEXITCODE)."
    }
    Write-Host "Enrollment-код сохранён."
}

function Start-AgentService {
    Start-Service -Name $ServiceName
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline) {
        $service = Get-Service -Name $ServiceName
        if ($service.Status -eq "Running") {
            Write-Host "Служба запущена."
            return
        }
        Start-Sleep -Seconds 2
    }
    throw "Служба не перешла в состояние Running за 60 секунд."
}

Assert-Administrator
Assert-WindowsVersion
Install-Binaries
Register-Service
Set-FirewallRules
Register-Enrollment
Start-AgentService

Write-Host ""
Write-Host "ClassOS Agent установлен. Устройство должно появиться в консоли преподавателя."
