<#
.SYNOPSIS
    Shows ClassOS Agent service status and recent log tail (T0 smoke test
    helper, spec §159).
#>

$ServiceName = "ClassOSAgent"
$LogPath = "C:\ProgramData\ClassOS\logs\service.log"

$service = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if (-not $service) {
    Write-Host "Service '$ServiceName' is not installed."
    exit 1
}

$service | Format-List Name, DisplayName, Status, StartType

$sessionHosts = Get-Process -Name "classos-session" -ErrorAction SilentlyContinue
if ($sessionHosts) {
    Write-Host ""
    Write-Host "classos-session.exe processes:"
    $sessionHosts | Format-Table Id, SessionId, StartTime -AutoSize
} else {
    Write-Host ""
    Write-Host "No classos-session.exe processes currently running."
}

if (Test-Path $LogPath) {
    Write-Host ""
    Write-Host "Last 40 lines of $LogPath :"
    Get-Content -Path $LogPath -Tail 40
} else {
    Write-Host ""
    Write-Host "No log file found at $LogPath yet."
}
