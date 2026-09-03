# ClassOS — T8 Implementation Specification

**Файл:** `docs/specs/T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md`
**Статус:** Spec-ready
**Milestone:** T8
**Предпосылка:** T7 завершён (health/software management работают)

---

## 1. Цель T8

Сделать продукт **воспроизводимым** без разработчика на месте (`03_EXECUTION_PLAN_90_DAYS.md` §29–31: второй филиал не должен зависеть от «ручного шаманства»). Три параллельных блока:

1. Signed installer (zero-touch enrollment).
2. Auto-update agent'а.
3. Cloud v0 — минимальный реальный backend (Organization/Branch/Room/User/RBAC), заменяющий локальную enrollment-заглушку из T1 §6.2.

---

## 2. Definition of Done

```text
Новый Student PC, чистая Windows
↓
classos-installer.exe --token ABC...
↓
< 30–60 минут (цель) без разработчика на месте
↓
устройство enrolled, service установлен, agent online, видно в Teacher/Admin Console нужного Room
```

```text
Cloud публикует новую версию agent'а
↓
устройства в канале "stable" обновляются автоматически
↓
health check после обновления — если fail, автоматический rollback на предыдущую версию
```

---

## 3. Non-goals

```text
полноценная multi-tenant enterprise-архитектура (Enterprise — отдельная поздняя фаза, см. 01_ROADMAP.md Phase 10)
SSO
Policy inheritance HQ→Region→Branch→Room (иерархия появляется, но не в полном enterprise-объёме — T8 даёт только плоскую Organization→Branch→Room→Device модель)
Lesson Engine / AlfaCRM (следующие фазы, см. FUTURE_PHASES_OVERVIEW.md)
```

---

## 4. Cloud v0 architecture

Один modular monolith, не microservices (`01_TECHNICAL_ARCHITECTURE.md` §98). Стек — уже зафиксирован (`CLAUDE.md`): Bun + TypeScript + PostgreSQL, Redis не используется, пока реальная нагрузка не потребует.

```text
API
├── Auth
├── Organizations
├── Branches
├── Rooms
├── Devices
├── Policies
├── Enrollment
├── Updates
└── Audit
```

### 4.1 Схема БД (минимум T8)

```text
organizations
users
organization_users
branches
rooms
devices
device_certificates
policies
room_policies
lesson_profiles       -- placeholder, полноценно наполнится в Lesson Engine фазе
enrollment_tokens
audit_events
agent_versions
```

(`01_TECHNICAL_ARCHITECTURE.md` §100)

### 4.2 Device secrets — что БД никогда не хранит

PostgreSQL никогда не хранит device private key (§102). Cloud знает только public identity/certificate metadata. Это прямое продолжение ADR-0005/T1 §6.1 — если реализация T1 уже это соблюдала для локальной заглушки, T8 не должен ослаблять требование при переезде на реальный backend.

---

## 5. Roles

```text
Owner  — branches, rooms, devices, teachers, profiles, health, billing
Admin  — управление конкретным branch
Teacher — view classroom, control classroom, apply lesson profile
```

Teacher **не может**: изменять organization policies, управлять billing, устанавливать произвольное привилегированное ПО (`03_EXECUTION_PLAN_90_DAYS.md` §33–34). RBAC-проверки выполняются на Cloud API и переносятся в signed lease (см. §7), который Agent проверяет offline.

---

## 6. Enrollment — переезд с T1-заглушки на Cloud issuer

T1 §6.2 реализовал enrollment локально через Teacher Console как временный authority. T8 переносит issuance в Cloud, **не меняя формат протокольных сообщений** `EnrollmentRequest`/`EnrollmentResult` (это было explicit требование ещё в T1 §6.2 — если оно было нарушено при реализации T1, потребуется отдельный ADR на миграцию схемы).

```text
Admin (Cloud/Admin Console)
↓ Create enrollment code (one-time, expires, привязан к organization/branch)
Installer
↓ Enter/scan code
Student Agent
↓ generate device key pair (как в T1, без изменений)
Cloud
↓ validate enrollment code
Cloud
↓ issue device certificate (теперь настоящий CA, не локальная заглушка)
```

---

## 7. Local offline authorization — signed classroom lease

Решает проблему: интернет пропал, а teacher должен продолжить урок (ADR-0005). Cloud заранее выдаёт Teacher Console:

```text
teacherId
organizationId
branchId
allowedRooms[]
permissions[]
issuedAt
expiresAt   (например 12–24 часа)
```

Agent проверяет подпись **локально**, без сетевого запроса. `Cloud unavailable + valid lease = classroom works` (`01_TECHNICAL_ARCHITECTURE.md` §46) — это прямая проверка инварианта 5 из `CLAUDE.md`.

---

## 8. Update architecture

### 8.1 Update channel

```text
stable | beta | canary
```

Room/branch может быть на `stable`; design-partner room — на `beta` (`01_TECHNICAL_ARCHITECTURE.md` §104).

### 8.2 Update manifest

```text
version, url, sha256, signature, minimumSupportedVersion, releaseChannel
```

Agent pipeline:

```text
download → verify hash → verify signature → stage → install → health check
```

(§105) — при провале health check: **rollback**, не оставить устройство в сломанном состоянии.

### 8.3 Code signing

Все Windows-исполняемые файлы (`classos-service.exe`, `classos-session.exe`, `classos-installer.exe`, будущий `classos-updater.exe`) — Authenticode signed для production (§106). Это блокер для реального пилота, не «можно потом».

### 8.4 Self-update problem

Service не заменяет сам себя вживую:

```text
Service downloads update
↓
spawns/stages updater helper
↓
service stops
↓
updater replaces binaries
↓
service starts
↓
health check
```

(§107) Rollback при провале health check — обязателен (§108), это распространение инварианта IV/9 (`CLAUDE.md`) на updater.

---

## 9. Installer

Responsibilities (§109):

```text
verify Windows version
install binaries
register ClassOSAgent service
configure recovery (SCM failure actions, как уже сделано в T0 §85)
configure firewall rules
enrollment
start service
```

MVP — собственный signed bootstrapper; production — желательно MSI ради совместимости с Intune/GPO/SCCM/RMM (§110–111), но это не блокер T8 DoD, если bootstrapper уже signed и надёжен.

---

## 10. Security checklist перед реальным rollout (обязателен к прохождению до первого платного пилота)

```text
signed installer
signed binaries
encrypted connections
device identity
authorization
audit remote sessions
secure auto-update
no plaintext secrets
standard-user student accounts
```

(`03_EXECUTION_PLAN_90_DAYS.md` §35) — этот чеклист логически завершает T8: если хоть один пункт не выполнен, продукт не готов для первого реального (не design-partner-only) филиала.

---

## 11. Resilience (наследуется и формализуется на уровне всей системы)

```text
Session Host crash          → T0 supervisor (уже реализовано)
Teacher disconnect          → T1/T4 (уже реализовано)
Internet outage             → T8 offline lease (§7)
User logout                 → T0
Windows reboot              → T0
sleep/wake                  → должно быть протестировано явно в T8 acceptance (не было отдельным пунктом раньше)
network reconnect           → T1
```

(`01_TECHNICAL_ARCHITECTURE.md` §37 из т0-спеки + §338 execution plan) T8 — точка, где все эти сценарии должны быть протестированы вместе как единый chaos-test набор, а не по отдельности в разных milestone'ах.

---

## 12. Security invariants T8

1. Приватный ключ устройства никогда не покидает устройство даже при переезде issuance в Cloud (§6.1 продолжение).
2. Cloud не хранит device private key (§4.2).
3. Обновления — только signed (manifest + binary), hash проверяется до установки (§8.2).
4. Offline lease проверяется криптографически локально, без доверия одному лишь факту "он у меня есть" без подписи.
5. Teacher role не может эскалировать себя до Admin/Owner permissions через протокол — только через явный Cloud RBAC flow.

---

## 13. Тесты

### Unit

```text
enrollment token: expiry/single-use логика (та же, что в T1, теперь против реального Cloud API)
update manifest verification: hash mismatch → reject, signature invalid → reject
offline lease: signature verification без сети
RBAC: permission matrix для Owner/Admin/Teacher
```

### Integration / Chaos (объединённый набор, §11)

```text
pull network cable во время урока → classroom продолжает работать на offline lease
kill Session Host / Teacher Console / restart Windows / sleep-wake / switch user / lock-unlock / cloud offline
   — все вместе, без ручного вмешательства между сценариями
zero-touch install на чистой машине, замер времени (<30-60 мин)
update rollout на design-partner (beta channel) → health check pass → stable channel позже
update rollout с намеренно битым manifest/подписью → отклонён, устройство не пострадало
update, вызывающий сбой health check после install → automatic rollback
```

---

## 14. Acceptance criteria

1. Полный security checklist (§10) пройден.
2. Zero-touch install укладывается в целевое время на чистой машине.
3. Update pipeline: happy path + rollback на сбое подтверждены тестами.
4. Cloud v0 поддерживает Organization/Branch/Room/Device/User/RBAC минимально достаточно для второго реального филиала.
5. Классная комната переживает полный chaos-test набор (§13) без ручного вмешательства.
6. Enrollment работает end-to-end через настоящий Cloud issuer, с тем же протокольным контрактом, что был спроектирован в T1.

---

## 15. Что дальше

Технический MVP (`01_TECHNICAL_ARCHITECTURE.md` §159) закрыт. Дальнейшие шаги — не T-milestones низкоуровневой инфраструктуры, а бизнес-домены: Lesson Engine, AlfaCRM, AI Supervisor/Tutor, Analytics, Enterprise. Они описаны на roadmap-уровне в `FUTURE_PHASES_OVERVIEW.md` и **требуют отдельного implementation-ready spec каждый**, прежде чем их можно будет кодировать — по аналогии с T0–T8.

Прежде чем продолжать, — это естественная точка для повторного go/no-go-чекпоинта из `03_EXECUTION_PLAN_90_DAYS.md` §66–68: подтвердилась ли гипотеза «преподаватели предпочитают ClassOS Veyon», прежде чем инвестировать в Lesson Engine/AlfaCRM/AI.
