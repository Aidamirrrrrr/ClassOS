<#
.SYNOPSIS
    Собирает и подписывает релиз ClassOS (spec T8 §8.3, §10).

.DESCRIPTION
    Единственный поддерживаемый способ получить сборку, которую можно ставить
    на устройство школы. Скрипт намеренно **отказывается** работать без ключей:
    неподписанный бинарник и сборка без ключа издателя выглядят рабочими и
    расходятся с production только в момент, когда что-то пошло не так.

    Шаги:
      1. Публичный ключ издателя вшивается в бинарники при компиляции — без
         него агент не примет ни одного обновления (`updater::publisher_key`).
      2. Authenticode-подпись всех исполняемых файлов.
      3. Проверка подписи: signtool молча пропускает часть ошибок конфигурации,
         поэтому результат перепроверяется явно.

    Подпись манифеста обновления выполняется отдельно и другим ключом:
        CLASSOS_PUBLISHER_SEED_HEX=... bun services/cloud/scripts/sign-manifest.ts ...
    Приватный ключ издателя не должен попадать в этот скрипт: здесь он не нужен.

.PARAMETER CertificateThumbprint
    Отпечаток Authenticode-сертификата в хранилище текущего пользователя.

.PARAMETER TimestampUrl
    Сервер меток времени. Без метки подпись перестаёт быть действительной
    вместе с истечением сертификата, а установленные агенты — нет.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$CertificateThumbprint,
    [string]$TimestampUrl = "http://timestamp.digicert.com",
    [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not $env:CLASSOS_PUBLISHER_KEY_HEX) {
    throw @"
Не задан CLASSOS_PUBLISHER_KEY_HEX — публичный ключ издателя обновлений (32 байта в hex).
Сборка без него компилируется, но отказывается обновляться, и это выяснится
только на устройстве. Задайте ключ и повторите.
"@
}
if ($env:CLASSOS_PUBLISHER_KEY_HEX -notmatch '^[0-9a-fA-F]{64}$') {
    throw "CLASSOS_PUBLISHER_KEY_HEX должен содержать ровно 32 байта в hex."
}

$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if (-not $signtool) {
    throw "signtool.exe не найден. Установите Windows SDK или добавьте его в PATH."
}

$certificate = Get-ChildItem -Path "Cert:\CurrentUser\My\$CertificateThumbprint" -ErrorAction SilentlyContinue
if (-not $certificate) {
    throw "Сертификат $CertificateThumbprint не найден в Cert:\CurrentUser\My."
}

Write-Host "==> Сборка релиза ($Target)"
Push-Location $repoRoot
try {
    & cargo build --workspace --release --target $Target
    if ($LASTEXITCODE -ne 0) { throw "cargo build завершился с ошибкой." }

    $releaseDir = Join-Path $repoRoot "target\$Target\release"
    $binaries = @("classos-service.exe", "classos-session.exe", "classos-updater.exe") |
        ForEach-Object { Join-Path $releaseDir $_ }

    foreach ($binary in $binaries) {
        if (-not (Test-Path $binary)) { throw "Не найден $binary — сборка неполная." }
    }

    Write-Host "==> Authenticode-подпись"
    & $signtool.Source sign /sha1 $CertificateThumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 $binaries
    if ($LASTEXITCODE -ne 0) { throw "Подпись не выполнена." }

    # Отдельная проверка: signtool sign может завершиться успешно там, где
    # подпись не пройдёт проверку политикой на целевой машине.
    Write-Host "==> Проверка подписи"
    & $signtool.Source verify /pa /all $binaries
    if ($LASTEXITCODE -ne 0) { throw "Проверка подписи не пройдена." }

    Write-Host ""
    Write-Host "Подписанные бинарники: $releaseDir"
    Write-Host "Дальше: упаковать их и подписать манифест обновления —"
    Write-Host "  bun services/cloud/scripts/sign-manifest.ts --file <архив> --version <версия> --url <адрес> --channel stable"
}
finally {
    Pop-Location
}
