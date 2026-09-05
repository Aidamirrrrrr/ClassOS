<#
.SYNOPSIS
    Показывает состояние службы ClassOS Agent, процессы Session Host и
    последние строки журнала для smoke test T0.
#>

$ServiceName = "ClassOSAgent"
$LogDir = "C:\ProgramData\ClassOS\logs"

$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if (-not $service) {
    Write-Host "Служба '$ServiceName' не установлена."
    exit 1
}

$service | Format-List Name, DisplayName, Status, StartType

$sessionHosts = Get-Process -Name "classos-session" -ErrorAction SilentlyContinue
if ($sessionHosts) {
    Write-Host ""
    Write-Host "Процессы classos-session.exe:"
    $sessionHosts | Format-Table Id, SessionId, StartTime -AutoSize
} else {
    Write-Host ""
    Write-Host "Запущенных процессов classos-session.exe нет."
}

$LogPath = Get-ChildItem -Path $LogDir -Filter "service.log*" -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if ($LogPath) {
    Write-Host ""
    Write-Host "Последние 40 строк $LogPath :"
    Get-Content -Path $LogPath.FullName -Tail 40
} else {
    Write-Host ""
    Write-Host "Файл журнала в $LogDir ещё не создан."
}
