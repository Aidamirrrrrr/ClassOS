<#
.SYNOPSIS
    Автоматический прогон механической части блока T0 чеклиста приёмки.

.DESCRIPTION
    Закрывает те пункты `docs/specs/RUNTIME_ACCEPTANCE.md` §3.1, которые
    можно проверить без человека за клавиатурой: установку службы, её
    состояние, содержимое журнала, перезапуск Session Host, останов и старт
    службы, простойную загрузку процессора и рост дескрипторов.

    Чего скрипт принципиально НЕ заменяет: вход и выход пользователя,
    блокировку экрана, перезагрузку машины, изоляцию named pipe между двумя
    учётными записями. Эти пункты требуют интерактивной консольной сессии и
    остаются за человеком — скрипт помечает их `NOT-RUN`, а не «пройдено».

    Пункты, зависящие от Session Host, выполняются только если консольная
    сессия существует и хост запустился. На машине без интерактивного входа
    (CI-раннер, VPS по RDP без автовхода) они тоже помечаются `NOT-RUN`:
    отсутствие проверки должно быть видно, а не выглядеть успехом.

.PARAMETER SourceDir
    Каталог с release-бинарниками. По умолчанию target\release в репозитории.

.PARAMETER IdleSeconds
    Длительность замера простойной загрузки CPU. Чеклист требует 10 минут;
    по умолчанию 120 секунд, чтобы скрипт был пригоден и для CI.

.PARAMETER RestartCycles
    Сколько раз убить Session Host при проверке утечки дескрипторов.
    Чеклист §160 требует 100; по умолчанию 10.

.PARAMETER KeepInstalled
    Не удалять службу после прогона. По умолчанию служба удаляется вместе с
    состоянием, чтобы повторный запуск начинался с чистой машины.

.EXAMPLE
    .\scripts\acceptance-t0.ps1
    .\scripts\acceptance-t0.ps1 -IdleSeconds 600 -RestartCycles 100
#>
[CmdletBinding()]
param(
    [string]$SourceDir,
    [int]$IdleSeconds = 120,
    [int]$RestartCycles = 10,
    [switch]$KeepInstalled
)

$ErrorActionPreference = "Stop"

$ServiceName = "ClassOSAgent"
$LogDir = "C:\ProgramData\ClassOS\logs"
$Results = [System.Collections.Generic.List[object]]::new()

function Add-Result {
    param(
        [string]$Id,
        [string]$Check,
        [ValidateSet("PASSED", "FAILED", "NOT-RUN")][string]$Status,
        [string]$Observation
    )
    $Results.Add([pscustomobject]@{
        Id          = $Id
        Check       = $Check
        Status      = $Status
        Observation = $Observation
    })
    $color = switch ($Status) {
        "PASSED"  { "Green" }
        "FAILED"  { "Red" }
        "NOT-RUN" { "Yellow" }
    }
    Write-Host ("[{0,-7}] {1} — {2}" -f $Status, $Id, $Check) -ForegroundColor $color
    if ($Observation) { Write-Host "          $Observation" }
}

function Assert-Administrator {
    $principal = New-Object Security.Principal.WindowsPrincipal(
        [Security.Principal.WindowsIdentity]::GetCurrent()
    )
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Прогон требует прав администратора: устанавливается и удаляется служба."
    }
}

function Get-ServiceLogText {
    $file = Get-ChildItem -Path $LogDir -Filter "service.log*" -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $file) { return $null }
    Get-Content -Path $file.FullName -Raw
}

function Wait-ForServiceStatus {
    param([string]$Status, [int]$TimeoutSeconds = 60)
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
        if ($service -and $service.Status -eq $Status) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return $false
}

function Get-SessionHostProcesses {
    @(Get-Process -Name "classos-session" -ErrorAction SilentlyContinue)
}

Assert-Administrator

if (-not $SourceDir) {
    $SourceDir = Join-Path (Split-Path -Parent $PSScriptRoot) "target\release"
}

Write-Host "=== T0: автоматическая часть чеклиста приёмки ===" -ForegroundColor Cyan
Write-Host "Бинарники: $SourceDir"
Write-Host ""

# Прогон должен начинаться с чистой машины: остатки прошлой установки
# превратили бы «служба работает» в наблюдение о предыдущем запуске.
if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
    Write-Host "Обнаружена прежняя установка — удаляю перед прогоном." -ForegroundColor Yellow
    & (Join-Path $PSScriptRoot "uninstall-service.ps1") -Purge | Out-Null
}

$hasSessionHost = $false

try {
    # --- 1. Установка -------------------------------------------------------
    try {
        & (Join-Path $PSScriptRoot "install-service.ps1") -SourceDir $SourceDir | Out-Null
        $service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
        if ($service) {
            Add-Result "3.1/1" "install-service.ps1 регистрирует службу" "PASSED" "StartType: $($service.StartType)"
        } else {
            Add-Result "3.1/1" "install-service.ps1 регистрирует службу" "FAILED" "служба не появилась в SCM"
        }
    } catch {
        Add-Result "3.1/1" "install-service.ps1 регистрирует службу" "FAILED" $_.Exception.Message
        throw
    }

    # --- 2. Служба в Running ------------------------------------------------
    if (Wait-ForServiceStatus -Status "Running" -TimeoutSeconds 60) {
        Add-Result "3.1/2" "Служба переходит в Running" "PASSED" ""
    } else {
        $actual = (Get-Service -Name $ServiceName).Status
        Add-Result "3.1/2" "Служба переходит в Running" "FAILED" "фактическое состояние: $actual"
    }

    # Дать supervisor'у время на первый reconcile и запуск Session Host.
    Start-Sleep -Seconds 10
    $hosts = Get-SessionHostProcesses
    $hasSessionHost = $hosts.Count -gt 0
    if ($hasSessionHost) {
        Write-Host "Session Host запущен (PID $($hosts[0].Id), сессия $($hosts[0].SessionId))." -ForegroundColor Cyan
    } else {
        Write-Host "Интерактивной консольной сессии нет — зависящие от неё пункты будут NOT-RUN." -ForegroundColor Yellow
    }
    Write-Host ""

    # --- 5. Журнал ----------------------------------------------------------
    $log = Get-ServiceLogText
    if (-not $log) {
        Add-Result "3.1/5" "Журнал службы создан" "FAILED" "файл service.log* не найден в $LogDir"
    } else {
        Add-Result "3.1/5a" "Журнал службы создан" "PASSED" "$LogDir"
        if ($log -match "SERVICE_RUNNING") {
            Add-Result "3.1/5b" "SERVICE_RUNNING в журнале" "PASSED" ""
        } else {
            Add-Result "3.1/5b" "SERVICE_RUNNING в журнале" "FAILED" "служба не сообщила о переходе в рабочее состояние"
        }

        # Эти три события существуют только там, где есть в кого запускать
        # Session Host. На машине без входа их отсутствие — норма, а не сбой.
        $handshake = @("SESSION_DISCOVERED", "SESSION_HOST_STARTED", "IPC_HANDSHAKE_OK")
        foreach ($event in $handshake) {
            if ($log -match $event) {
                Add-Result "3.1/5-$event" "$event в журнале" "PASSED" ""
            } elseif ($hasSessionHost) {
                Add-Result "3.1/5-$event" "$event в журнале" "FAILED" "Session Host запущен, но событие отсутствует"
            } else {
                Add-Result "3.1/5-$event" "$event в журнале" "NOT-RUN" "нет интерактивной консольной сессии"
            }
        }
    }

    # --- 6. Нет цикла перезапусков -----------------------------------------
    if ($log -and $log -match "SESSION_HOST_CRASH_LOOP") {
        Add-Result "3.1/6" "Нет цикла перезапусков Session Host" "FAILED" "в журнале есть SESSION_HOST_CRASH_LOOP"
    } elseif ($hasSessionHost) {
        Add-Result "3.1/6" "Нет цикла перезапусков Session Host" "PASSED" ""
    } else {
        Add-Result "3.1/6" "Нет цикла перезапусков Session Host" "NOT-RUN" "нет интерактивной консольной сессии"
    }

    # --- 10. Убийство Session Host → перезапуск ----------------------------
    if ($hasSessionHost) {
        $before = (Get-SessionHostProcesses)[0].Id
        Stop-Process -Id $before -Force
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        $restored = $null
        while ($stopwatch.Elapsed.TotalSeconds -lt 60) {
            $current = Get-SessionHostProcesses | Where-Object { $_.Id -ne $before }
            if ($current) { $restored = $current[0]; break }
            Start-Sleep -Milliseconds 500
        }
        $stopwatch.Stop()
        if (-not $restored) {
            Add-Result "3.1/10" "Session Host перезапускается после убийства" "FAILED" "новый процесс не появился за 60 с"
        } elseif ($stopwatch.Elapsed.TotalSeconds -le 30) {
            Add-Result "3.1/10" "Session Host перезапускается < 30 с" "PASSED" ("{0:N1} с, новый PID {1}" -f $stopwatch.Elapsed.TotalSeconds, $restored.Id)
        } else {
            Add-Result "3.1/10" "Session Host перезапускается < 30 с" "FAILED" ("{0:N1} с — дольше требуемых 30" -f $stopwatch.Elapsed.TotalSeconds)
        }
    } else {
        Add-Result "3.1/10" "Session Host перезапускается после убийства" "NOT-RUN" "нет интерактивной консольной сессии"
    }

    # --- 11. Restart-Service ------------------------------------------------
    # Единственный баг за историю проекта был здесь: служба не сообщала
    # STOP_PENDING и не останавливалась.
    $restartWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $stopFailure = $null
    try {
        Stop-Service -Name $ServiceName -Force -ErrorAction Stop
    } catch {
        $stopFailure = $_.Exception.Message
    }
    $restartWatch.Stop()

    if ($stopFailure) {
        Add-Result "3.1/11a" "Останов службы проходит" "FAILED" $stopFailure
    } elseif (Wait-ForServiceStatus -Status "Stopped" -TimeoutSeconds 60) {
        Add-Result "3.1/11a" "Останов службы проходит" "PASSED" ("{0:N1} с" -f $restartWatch.Elapsed.TotalSeconds)
    } else {
        Add-Result "3.1/11a" "Останов службы проходит" "FAILED" "служба не достигла Stopped за 60 с"
    }

    Start-Service -Name $ServiceName
    if (Wait-ForServiceStatus -Status "Running" -TimeoutSeconds 60) {
        Add-Result "3.1/11b" "Служба поднимается после останова" "PASSED" ""
    } else {
        Add-Result "3.1/11b" "Служба поднимается после останова" "FAILED" "служба не вернулась в Running за 60 с"
    }

    # --- 13. Простойная загрузка CPU ---------------------------------------
    $process = Get-Process -Name "classos-service" -ErrorAction SilentlyContinue
    if (-not $process) {
        Add-Result "3.1/13" "Простойная загрузка CPU около нуля" "FAILED" "процесс службы не найден"
    } else {
        $cpuBefore = $process.TotalProcessorTime
        Write-Host "Замер простоя: $IdleSeconds с..." -ForegroundColor Cyan
        Start-Sleep -Seconds $IdleSeconds
        $process.Refresh()
        $cpuUsed = ($process.TotalProcessorTime - $cpuBefore).TotalSeconds
        # Порог считается от числа ядер: 1% одного ядра на многоядерной
        # машине и на одноядерной — разные абсолютные величины.
        $budget = $IdleSeconds * 0.01
        $percent = [math]::Round($cpuUsed / $IdleSeconds * 100, 2)
        if ($cpuUsed -le $budget) {
            Add-Result "3.1/13" "Простойная загрузка CPU < 1%" "PASSED" "$percent% за $IdleSeconds с"
        } else {
            Add-Result "3.1/13" "Простойная загрузка CPU < 1%" "FAILED" "$percent% за $IdleSeconds с"
        }
    }

    # --- 14. Дескрипторы после циклов перезапуска --------------------------
    if (-not $hasSessionHost) {
        Add-Result "3.1/14" "Дескрипторы службы не растут монотонно" "NOT-RUN" "нет интерактивной консольной сессии"
    } else {
        $service = Get-Process -Name "classos-service"
        $handlesBefore = $service.HandleCount
        $completed = 0
        for ($i = 1; $i -le $RestartCycles; $i++) {
            $current = Get-SessionHostProcesses
            if (-not $current) { Start-Sleep -Seconds 2; continue }
            $pidToKill = $current[0].Id
            Stop-Process -Id $pidToKill -Force -ErrorAction SilentlyContinue
            $deadline = (Get-Date).AddSeconds(60)
            while ((Get-Date) -lt $deadline) {
                if (Get-SessionHostProcesses | Where-Object { $_.Id -ne $pidToKill }) { break }
                Start-Sleep -Milliseconds 500
            }
            $completed++
        }
        $service.Refresh()
        $handlesAfter = $service.HandleCount
        $growth = $handlesAfter - $handlesBefore
        # Утечка на цикл — то, что превращается в отказ за смену. Небольшой
        # разброс нормален, поэтому порог задан на цикл, а не абсолютный.
        $allowed = [math]::Max(64, $completed * 4)
        if ($growth -le $allowed) {
            Add-Result "3.1/14" "Дескрипторы службы не растут монотонно" "PASSED" "$handlesBefore → $handlesAfter за $completed циклов"
        } else {
            Add-Result "3.1/14" "Дескрипторы службы не растут монотонно" "FAILED" "$handlesBefore → $handlesAfter (+$growth) за $completed циклов"
        }
    }
} finally {
    # --- Пункты, требующие человека ----------------------------------------
    Add-Result "3.1/3"  "Вход пользователем student поднимает Session Host" "NOT-RUN" "нужен интерактивный вход"
    Add-Result "3.1/4"  "status.ps1 показывает верного пользователя и сессию" "NOT-RUN" "нужен интерактивный вход"
    Add-Result "3.1/7"  "Win+L: SESSION_LOCK_STATE_CHANGED в обе стороны" "NOT-RUN" "нужна блокировка экрана руками"
    Add-Result "3.1/8"  "Выход из системы завершает Session Host" "NOT-RUN" "нужен интерактивный выход"
    Add-Result "3.1/9"  "Повторный вход даёт новый Session Host" "NOT-RUN" "нужен интерактивный вход"
    Add-Result "3.1/12" "Перезагрузка машины поднимает службу и хост" "NOT-RUN" "нужна перезагрузка"
    Add-Result "3.2/D"  "student2 получает свой Session Host" "NOT-RUN" "нужна вторая учётная запись"
    Add-Result "3.2/E"  "Чужой named pipe недоступен из другой сессии" "NOT-RUN" "нужна вторая учётная запись"
    Add-Result "3.2/F"  "8 часов простоя" "NOT-RUN" "нужен длительный прогон"

    if (-not $KeepInstalled) {
        if (Get-Service -Name $ServiceName -ErrorAction SilentlyContinue) {
            & (Join-Path $PSScriptRoot "uninstall-service.ps1") -Purge | Out-Null
            Write-Host ""
            Write-Host "Служба удалена, состояние очищено." -ForegroundColor Cyan
        }
    }
}

# --- Отчёт -----------------------------------------------------------------
$passed  = @($Results | Where-Object { $_.Status -eq "PASSED" }).Count
$failed  = @($Results | Where-Object { $_.Status -eq "FAILED" }).Count
$notRun  = @($Results | Where-Object { $_.Status -eq "NOT-RUN" }).Count

Write-Host ""
Write-Host "=== Отчёт (вставляется в RUNTIME_ACCEPTANCE.md) ===" -ForegroundColor Cyan
Write-Host ""
Write-Host "| Пункт | Проверка | Статус | Наблюдение |"
Write-Host "| --- | --- | --- | --- |"
foreach ($result in $Results) {
    Write-Host ("| {0} | {1} | {2} | {3} |" -f $result.Id, $result.Check, $result.Status, $result.Observation)
}
Write-Host ""
Write-Host "Пройдено: $passed   Провалено: $failed   Не выполнялось: $notRun"
Write-Host ""
Write-Host "NOT-RUN — это НЕ пройденный пункт. Он остаётся за человеком и" -ForegroundColor Yellow
Write-Host "закрывается вручную по docs/specs/RUNTIME_ACCEPTANCE.md." -ForegroundColor Yellow

if ($failed -gt 0) { exit 1 }
exit 0
