# ClassOS — T7 Implementation Specification

**Файл:** `docs/specs/T7_DEVICE_HEALTH_SOFTWARE_SPEC.md`
**Статус:** Spec-ready
**Milestone:** T7
**Предпосылка:** T6 завершён (Policy Engine, Focus Mode работают)

---

## 1. Цель T7

Продать продукт не только преподавателю, но и владельцу филиала/IT-администратору (`02_MARKET_AND_INVESTMENT_ANALYSIS.md` §4). Реализовать device health, software inventory и Repair — killer feature №2 из продуктового анализа (§17).

---

## 2. Definition of Done

```text
Admin Console → Room 2
↓
PC-01 ✓  PC-02 ✓  PC-03 ⚠ Python mismatch  PC-04 ✓  PC-05 ⚠ VS Code extension missing
↓
[Repair all]
↓
Python устанавливается на PC-03 через WinGet, extension — на PC-05
↓
Дашборд обновляется: все PC ✓
```

---

## 3. Non-goals

```text
полноценный SCCM/Intune-уровень deployment (§25 арх. RFC — "нам достаточно нескольких приложений, которые нужны design partners")
uninstall для teacher (только admin, если вообще есть в этом milestone)
WinGet Configuration/DSC decl. YAML pipeline (future v1, §79 арх. RFC — MVP ограничивается простыми package operations)
```

---

## 4. Device Health

### 4.1 Собираемые метрики

```text
uptime, CPU, RAM, disk
Windows version, hostname
agent version
active session
software profile status
policy status
```

(`01_TECHNICAL_ARCHITECTURE.md` §55)

### 4.2 Health state — считается локально на Agent, не только в облаке

```text
Healthy
Warning
Critical
Offline
```

Пример правил (§56):

```text
disk > 90%              → Warning
required package missing → Warning
policy apply failed      → Critical
```

Teacher должен видеть состояние даже без облака (local-first, ADR-0005 распространяется и на health).

### 4.3 Новые сообщения

```protobuf
message DeviceHealthReport {
  string device_id = 1;
  enum State { HEALTHY = 0; WARNING = 1; CRITICAL = 2; }
  State state = 2;
  double cpu_percent = 3;
  double ram_percent = 4;
  double disk_percent = 5;
  string os_version = 6;
  string agent_version = 7;
  repeated string warnings = 8;   // machine-readable коды, не свободный текст
  int64 reported_at_unix_ms = 9;
}
```

Отправляется периодически (P2 priority, см. `T3_*` §5 — не должен конкурировать с control-каналом) плюс по запросу.

### 4.4 Process monitoring

MVP — Win32 process enumeration (pid, exe path, user, start time). ETW — explicitly future, не внедрять до реальной необходимости (`01_TECHNICAL_ARCHITECTURE.md` §57, §20 арх. RFC).

---

## 5. Application identity — через `ApplicationDefinition`, не голое имя процесса

Не идентифицировать программы по `processName == "Code.exe"` (`01_TECHNICAL_ARCHITECTURE.md` §58). Нужна абстракция:

```text
ApplicationDefinition
  id: vscode
  displayName: Visual Studio Code
  executables: [Code.exe]
  publisher: Microsoft Corporation
  installDetection: ...
  wingetId: Microsoft.VisualStudioCode
```

Этот же catalog уже частично использовался в T5 (`LaunchApplication.application_id`) — T7 расширяет его полями для detection/install/version, не создаёт параллельную структуру.

---

## 6. Software Manager

Крейт `software-manager`. Операции:

```text
detect
install
uninstall   // admin only
repair
version
```

### 6.1 WinGet

Используется там, где пакет доступен через Windows Package Manager (`01_TECHNICAL_ARCHITECTURE.md` §78). `ApplicationDefinition.wingetId` + `approvedVersion` — школе нужна конкретная одобренная версия, не всегда `latest` (`01_ROADMAP.md` §37: обновление Python/Unity/Roblox посреди программы может всё сломать).

### 6.2 Package execution security

Teacher/Admin **не** отправляет произвольный WinGet-запрос. Все установки идут через approved package catalog — иначе Teacher Console превращается в remote code execution систему (`01_TECHNICAL_ARCHITECTURE.md` §84). Это прямое продолжение инварианта из T5 §5/§9.2.

---

## 7. Software Profile & Desired State / Drift

```text
Python Classroom v1
  Python
  VS Code
  Git
  Chrome
```

Для каждого Room — `desiredProfileId`. Для устройства рассчитывается:

```text
Desired State vs Actual State
```

Пример drift:

```text
PC-07
  Python   required 3.13.x   actual missing     → DRIFTED
  VS Code  required installed actual installed  → OK
```

(`01_TECHNICAL_ARCHITECTURE.md` §80–82)

---

## 8. Repair

```protobuf
message RepairDesiredState {
  string device_id = 1;
  string profile_id = 2;
}

message RepairResult {
  string device_id = 1;
  repeated RepairItemResult items = 2;
}

message RepairItemResult {
  string application_id = 1;
  bool success = 2;
  string error_code = 3;
}
```

Agent получает `RepairDesiredState`, выполняет `install Python` → `verify` → `report` (§83 арх. RFC). Тот же `Command`/idempotency паттерн из T5.

---

## 9. Admin Console (минимум T7)

```text
Room 2
PC-01    Online    Healthy
PC-02    Online    Healthy
PC-03    Offline
PC-04    Python mismatch
PC-05    Disk warning

Действия: Repair, Restart, Update, Move room, Apply profile, Reinstall package, Open logs
```

(`01_ROADMAP.md` §36) — не все действия обязательны к реализации в T7 (`Move room` предполагает Cloud v0/Organization-модель из T8), но Repair/Restart/Apply profile/Open logs — да.

---

## 10. Security invariants T7

1. Установка ПО — только через approved catalog, никогда arbitrary package query (§6.2).
2. Uninstall (если реализован) — только admin role, не teacher.
3. Health-предупреждения — machine-readable коды (§4.3), не свободный текст, который может стать вектором для UI injection или просто нестабильным для автоматизации.
4. Repair не должен молча "чинить" что-то за пределами явно указанного `profile_id`.

---

## 11. Тесты

### Unit

```text
DeviceHealthReport encode-decode
health state rules: disk>90%→Warning, policy failed→Critical, комбинации
drift calculation: desired vs actual → корректный список расхождений
```

### Integration

```text
установка приложения через WinGet на чистой машине → detect подтверждает наличие после install
drift: удалить вручную нужный пакет → следующий health report показывает DRIFTED
Repair all на нескольких устройствах параллельно (тот же bulk-паттерн, что T5) → partial failure отчёт
approved version pinning: попытка "install latest" без явного approved override не проходит
```

---

## 12. Acceptance criteria

1. Health report корректно отражает реальное состояние диска/CPU/RAM/software на устройстве.
2. Drift между desired и actual state вычисляется корректно для типового Software Profile.
3. Repair устанавливает недостающие/некорректные компоненты и обновляет статус до Healthy.
4. Установка ограничена approved catalog — попытка выйти за его пределы явно отклоняется.
5. Новое устройство можно подключить и привести к стандарту кабинета практически без ручной настройки (`01_ROADMAP.md`, Phase 3 DoD).

---

## 13. Что дальше

`T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md` — auto-update, signed installer, минимальный реальный Cloud backend (Organization/Branch/Room/User/RBAC), после чего Enrollment (T1 §6.2) переезжает с локальной заглушки на настоящий cloud issuer.
