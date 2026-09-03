# ClassOS — Technical Architecture RFC

**Файл:** `docs/architecture/01_TECHNICAL_ARCHITECTURE.md`
**Статус:** Draft RFC
**Версия:** 0.1
**Платформа MVP:** Windows 10/11 x64
**Основной язык агента:** Rust
**Teacher Console:** Tauri 2 + React + TypeScript
**Cloud:** TypeScript + Bun + PostgreSQL

---

## 1. Purpose

Этот документ определяет техническую архитектуру ClassOS.

Он отвечает на вопросы:

* какие процессы работают на ученическом ПК;
* какие привилегии имеет каждый процесс;
* как Teacher Console находит компьютеры;
* как происходит аутентификация;
* как передаётся экран;
* как реализуется remote control;
* как применяются Windows policies;
* как реализуется Focus Mode;
* как устанавливается ПО;
* как ClassOS переживает reboot/crash/network failure;
* как обновляется agent;
* какие API и protocol messages существуют;
* где хранится состояние;
* что работает локально;
* что требует cloud;
* какие security boundaries нельзя нарушать.

Главный architectural principle:

> **ClassOS не заменяет Windows. ClassOS оркестрирует Windows.**

---

## 2. Scope v0

Архитектура должна поддерживать следующие функции.

### Classroom

* device discovery;
* online/offline status;
* screen thumbnails;
* full-screen live view;
* remote keyboard/mouse;
* lock/unlock;
* message;
* launch application;
* open URL;
* restart;
* shutdown.

### Control

* Focus Mode;
* allowed applications;
* blocked applications;
* Windows settings restrictions;
* lesson profiles.

### Device Management

* hardware/software information;
* application inventory;
* health;
* software installation;
* repair;
* agent updates.

### Future

Архитектура не должна мешать добавить:

* Lesson Engine;
* AlfaCRM;
* Student Identity;
* AI Supervisor;
* AI Tutor;
* telemetry;
* Linux agent;
* enterprise hierarchy.

---

## 3. Non-goals

На первой архитектуре не строим:

* собственную ОС;
* replacement `explorer.exe`;
* kernel driver;
* собственный display driver;
* собственный package manager;
* собственный firewall engine;
* собственную систему Windows accounts;
* постоянную cloud video streaming infrastructure;
* full MDM competitor для Intune;
* spyware/keylogger;
* скрытый мониторинг пользователей.

---

## 4. High-Level Architecture

```text
                            CLASSOS CLOUD

                  ┌──────────────────────────┐
                  │        API Gateway       │
                  ├──────────────────────────┤
                  │ Auth / Organizations     │
                  │ Device Registry          │
                  │ Policies                 │
                  │ Integrations             │
                  │ Update Service           │
                  │ Audit                    │
                  │ Analytics                │
                  │ AI                       │
                  └────────────┬─────────────┘
                               │
                         HTTPS / WSS
                               │
                               │
                         SCHOOL NETWORK

                    ┌─────────────────────┐
                    │   Teacher Console   │
                    │   Tauri + React     │
                    └──────────┬──────────┘
                               │
                 authenticated local network
                               │
                 ┌─────────────┼─────────────┐
                 │             │             │
                 ▼             ▼             ▼

              Student       Student       Student
               PC-01         PC-02         PC-03

            ┌─────────┐   ┌─────────┐   ┌─────────┐
            │ Service │   │ Service │   │ Service │
            └────┬────┘   └────┬────┘   └────┬────┘
                 │ IPC          │ IPC          │ IPC
            ┌────▼────┐   ┌────▼────┐   ┌────▼────┐
            │ Session │   │ Session │   │ Session │
            │  Host   │   │  Host   │   │  Host   │
            └────┬────┘   └────┬────┘   └────┬────┘
                 │             │             │
                 ▼             ▼             ▼
              Windows       Windows       Windows
```

---

## 5. Local-first architecture

Самое важное требование:

### Урок не должен зависеть от интернета

Если внешний интернет школы исчез:

должны продолжить работать:

* discovery;
* screen streaming;
* remote control;
* Focus Mode;
* application launch;
* lock;
* messages;
* restart;
* shutdown;
* уже загруженные Lesson Profiles.

Cloud недоступен:

```text
ClassOS Cloud
     X
     │

Teacher Console
     │
     ├──────── Student 1
     ├──────── Student 2
     └──────── Student 3
```

урок продолжается.

Cloud используется для:

* login;
* organization state;
* synchronization;
* billing;
* integration;
* update metadata;
* analytics;
* AI.

---

## 6. Student Agent is not one process

Нельзя делать:

```text
classos-agent.exe
    │
    ├── LocalSystem
    ├── screen capture
    ├── UI
    ├── network
    ├── policies
    └── everything
```

Windows services работают в Session 0 и начиная с Windows Vista не должны напрямую взаимодействовать с пользовательским desktop. Microsoft рекомендует для взаимодействия с interactive session запускать отдельный пользовательский процесс.

Поэтому Student Agent делится минимум на два runtime-компонента.

---

## 7. ClassOS Agent Service

Executable:

```text
classos-service.exe
```

Windows Service:

```text
Service Name:
ClassOSAgent

Account:
LocalSystem

Startup:
Automatic
```

Основные обязанности:

```text
device identity
network server
authentication
authorization

policy management
software management
device health

Windows services
reboot / shutdown

session tracking
Session Host lifecycle

updates

cloud sync
audit forwarding
```

Service **не занимается UI**.

Service **не захватывает desktop пользователя**.

Service **не вызывает SendInput напрямую в Session 0**.

---

## 8. ClassOS Session Host

Executable:

```text
classos-session.exe
```

Запускается внутри interactive Windows session пользователя.

Основные обязанности:

```text
desktop capture
mouse / keyboard injection

active window
idle state

student notification overlay
remote control indicator

future:
student identity
AI UI
lesson UI
```

Один interactive session:

```text
one Session Host
```

Если используется несколько Windows sessions:

```text
Session 1
→ Session Host 1

Session 2
→ Session Host 2
```

Для MVP мы официально поддерживаем:

> одну активную interactive console session.

---

## 9. Session lifecycle

Service получает Windows session change events.

Windows передаёт service идентификатор session при session-change notification через `WTSSESSION_NOTIFICATION`.

Состояния:

```text
NoUser
↓
UserLogon
↓
SessionHostStarting
↓
Active
↓
UserLogoff
↓
NoUser
```

Service должен обрабатывать:

```text
logon
logoff
lock
unlock
console connect
console disconnect
```

---

## 10. Launching Session Host

Service определяет active user session.

Затем запускает:

```text
classos-session.exe
```

в контексте этого пользователя.

Conceptually:

```text
WTS active session
↓
user token
↓
CreateProcessAsUser
↓
ClassOS Session Host
```

Session Host:

* не admin;
* не LocalSystem;
* работает с integrity level пользователя;
* не получает privileged credentials.

---

## 11. Service ↔ Session Host IPC

Используем:

## Windows Named Pipe

Пример:

```text
\\.\pipe\classos\agent-session-{sessionId}
```

Почему Named Pipes:

* native Windows;
* duplex;
* быстрые;
* не требуют TCP port;
* поддерживают ACL;
* хорошо подходят для service ↔ user process.

Windows позволяет назначить pipe собственный security descriptor и проводить стандартную ACL-проверку при подключении.

---

## 12. Named Pipe security

Нельзя использовать default ACL.

Microsoft отмечает, что default security descriptor named pipe может предоставлять read access достаточно широкому набору пользователей, поэтому ClassOS обязательно создаёт explicit DACL.

Разрешаем:

```text
LocalSystem
+
конкретный logon SID текущей session
```

Запрещаем:

```text
Everyone
Anonymous
NETWORK
other sessions
```

Дополнительно pipe должен быть предназначен только для local IPC. Microsoft отдельно рекомендует запрещать network access для локальных named pipes либо использовать локальный IPC-механизм.

---

## 13. IPC protocol

Service и Session Host используют тот же общий schema layer, но отдельную transport abstraction.

```text
SessionHello
SessionHeartbeat

CaptureStart
CaptureStop
Frame

RemoteInput
RemoteControlStarted
RemoteControlStopped

ShowMessage
ShowOverlay

GetForegroundApp
ForegroundAppChanged

SessionState
```

---

## 14. Repository layout

```text
classos/
│
├── apps/
│   │
│   ├── teacher/
│   │   ├── src/
│   │   ├── src-tauri/
│   │   └── package.json
│   │
│   └── admin/
│
├── crates/
│   │
│   ├── agent-service/
│   │
│   ├── agent-session/
│   │
│   ├── agent-core/
│   │
│   ├── protocol/
│   │
│   ├── transport/
│   │
│   ├── windows-platform/
│   │
│   ├── screen-capture/
│   │
│   ├── remote-input/
│   │
│   ├── policy-engine/
│   │
│   ├── software-manager/
│   │
│   ├── device-health/
│   │
│   ├── updater/
│   │
│   └── common/
│
├── services/
│   │
│   ├── api/
│   └── integrations/
│
├── packages/
│   ├── ui/
│   ├── api-client/
│   └── shared/
│
├── proto/
│   └── classos.proto
│
├── installer/
│
├── docs/
│
└── scripts/
```

---

## 15. Rust crate boundaries

### `agent-service`

Composition root Windows Service.

Не содержит напрямую Windows API implementation.

Использует abstractions:

```text
DeviceIdentity
SessionManager
PolicyEngine
SoftwareManager
NetworkServer
CloudClient
Updater
HealthCollector
```

---

## 16. `windows-platform`

Здесь находится грязная Windows-specific часть.

Пример модулей:

```text
windows-platform/
│
├── service.rs
├── sessions.rs
├── users.rs
├── processes.rs
├── power.rs
├── registry.rs
├── firewall.rs
├── policies.rs
├── applocker.rs
├── winget.rs
└── security.rs
```

Любая Win32-specific логика максимально изолирована здесь.

---

## 17. `screen-capture`

API:

```rust
trait ScreenCapture {
    fn displays(&self) -> Result<Vec<Display>>;
    fn start(&mut self, display: DisplayId) -> Result<()>;
    fn next_frame(&mut self) -> Result<Frame>;
    fn stop(&mut self);
}
```

Implementations:

```text
DxgiDesktopCapture
WindowsGraphicsCapture // future/alternative
MockCapture
```

---

## 18. Desktop capture technology

Основной backend:

## DXGI Desktop Duplication API

Microsoft позиционирует Desktop Duplication API именно для desktop collaboration и remote-desktop scenarios, включая enterprise и educational software. Windows отдаёт кадры в GPU memory вместе с dirty regions, move metadata и cursor information.

Преимущества:

```text
GPU surface
dirty rects
move rects
cursor metadata
multi-monitor
full-screen DirectX apps
```

Это подходящий фундамент для Veyon replacement.

---

## 19. Capture pipeline

```text
Windows Desktop
      │
      ▼
DXGI Output Duplication
      │
      ▼
GPU Texture
      │
      ├── cursor metadata
      ├── dirty rectangles
      └── move rectangles
      │
      ▼
Frame Processor
      │
      ├── scale
      ├── crop
      └── encode
      │
      ▼
Transport
```

---

## 20. Capture modes

Не существует одного stream profile.

### Thumbnail mode

Используется grid Teacher Console.

Target:

```text
640×360 max
1–2 FPS
medium JPEG/WebP quality
```

Цель:

минимальный network/CPU overhead.

20 machines × 1 FPS гораздо разумнее, чем 20 × 30 FPS.

---

## 21. Selected device mode

Когда teacher открывает один PC:

Target MVP:

```text
1280p-ish
8–15 FPS
```

Target v1:

```text
native/near-native
15–30 FPS
adaptive bitrate
```

---

## 22. Encoding roadmap

### MVP

Использовать простой image encoder:

```text
JPEG
```

Почему:

* очень быстро реализуется;
* достаточно для проверки classroom UX;
* не требуется сложный video pipeline.

---

### v1

Добавить:

```text
H.264
```

с hardware acceleration там, где возможно.

Architecture должна уже иметь:

```rust
trait FrameEncoder {
    fn encode(&mut self, frame: RawFrame) -> Result<EncodedFrame>;
}
```

То есть network/UI вообще не должны знать, JPEG сейчас используется или H.264.

---

## 23. Dirty-region optimization

DXGI отдаёт metadata изменившихся областей.

На MVP:

```text
ignore optimization
encode complete scaled frame
```

После стабильного MVP:

```text
dirty region
↓
partial update
```

Это особенно эффективно для IDE:

80–90% экрана часто вообще не меняется.

---

## 24. Multiple displays

Device reports:

```text
Display 0
1920×1080
primary

Display 1
1920×1080
```

Teacher по умолчанию видит primary.

Device Detail:

```text
[Display 1]
[Display 2]
```

MVP:

поддержать хотя бы detection + primary.

Multi-display switching — P1.

---

## 25. Remote Input

Module:

```text
remote-input
```

Используется Session Host.

Windows API:

```text
SendInput()
```

`SendInput` умеет синтезировать mouse/keyboard input; Microsoft также отмечает ограничение UIPI — процесс не может inject input в процессы с более высоким integrity level.

Это хорошее security property.

Teacher не должен через ClassOS управлять elevated admin applications.

---

## 26. Remote input events

Wire schema:

```text
MouseMove
MouseButtonDown
MouseButtonUp
MouseWheel

KeyDown
KeyUp

ClipboardText // future
```

Mouse coordinates передаются normalized:

```text
0.0 → 1.0
```

а не абсолютными pixels.

Так stream scaling не ломает input mapping.

---

## 27. Remote session state

Remote control — explicit state machine:

```text
Idle
↓
Requesting
↓
Active
↓
Stopping
↓
Idle
```

Agent хранит:

```text
teacherId
teacherDeviceId
startedAt
sessionId
```

Одновременно:

> только один active remote-control owner.

Другие teachers могут смотреть stream, но control exclusive.

---

## 28. Student indication

Во время remote control Session Host показывает:

```text
┌───────────────────────────┐
│ Teacher connected         │
└───────────────────────────┘
```

или постоянный небольшой indicator.

Нельзя делать скрытый remote control.

---

## 29. Local device discovery

Discovery ≠ authorization.

Для MVP используем простой local multicast/broadcast discovery protocol.

Agent периодически объявляет:

```text
ClassOSDeviceAnnouncement
```

Поля:

```text
protocolVersion
deviceId
hostname
roomHint
agentVersion
ip
controlPort
```

Никаких:

```text
credentials
tokens
student names
secret keys
```

---

## 30. Discovery lifecycle

Teacher Console:

```text
join multicast group
↓
receive announcements
↓
show candidate devices
↓
authenticate
↓
connect
```

Offline определяется:

```text
lastSeen > timeout
```

---

## 31. Discovery attack model

Любой пользователь LAN потенциально может подделать announcement.

Это допустимо.

Announcement означает только:

> «какой-то компьютер говорит, что он ClassOS device».

До cryptographic authentication он считается:

```text
UNTRUSTED
```

---

## 32. Teacher ↔ Agent transport

Нужен собственный transport abstraction.

```rust
trait DeviceTransport {
    async fn connect(...);
    async fn send_command(...);
    async fn subscribe_events(...);
    async fn subscribe_screen(...);
}
```

Первоначальный wire transport:

```text
secure TCP/TLS
```

или QUIC.

Рекомендуемый production target:

## QUIC + TLS 1.3

Почему:

* encrypted transport;
* streams;
* multiplexing;
* быстрое reconnect;
* потенциальные datagrams для realtime data.

Но MVP допускает:

```text
TLS TCP
```

если это заметно ускоряет первый prototype.

Главное:

> protocol layer не должен зависеть от конкретного transport.

---

## 33. Logical channels

Одно connection может содержать logical channels:

```text
CONTROL
EVENTS
SCREEN
REMOTE_INPUT
FILE_TRANSFER
```

При QUIC каждый может стать отдельным stream.

---

## 34. Priority

Очень важно не позволить stream экранов забить control commands.

Priority:

```text
P0
Control

P1
Remote Input

P2
Health / Events

P3
Selected Screen

P4
Thumbnail Screen
```

Нажатие:

> Lock Class

не должно ждать, пока передастся JPEG.

---

## 35. Protocol format

Используем schema-first protocol.

Рекомендация:

## Protocol Buffers

Файл:

```text
proto/classos.proto
```

Преимущества:

* versioning;
* generated Rust types;
* generated TS types;
* compact;
* clear message contracts.

---

## 36. Base envelope

Conceptual:

```protobuf
message Envelope {
  uint32 protocol_version = 1;
  string message_id = 2;
  int64 timestamp_ms = 3;

  oneof payload {
    DeviceHello device_hello = 10;
    Command command = 11;
    CommandResult command_result = 12;
    Event event = 13;
  }
}
```

---

## 37. DeviceHello

```text
DeviceHello

device_id
hostname
agent_version
protocol_version

os_version
architecture

capabilities[]
```

Capabilities:

```text
screen.dxgi
remote_input
policy.applocker
software.winget
health.basic
```

Feature detection важнее assumption.

---

## 38. Protocol versioning

Нельзя делать:

```text
Teacher v2
+
Agent v1
=
undefined behaviour
```

Handshake:

```text
Teacher:
minProtocol = 1
maxProtocol = 3

Agent:
minProtocol = 2
maxProtocol = 4
```

Выбирается:

```text
highest mutually supported
```

Если пересечения нет:

```text
UpgradeRequired
```

---

## 39. Commands

Каждая команда:

```text
commandId
type
deadline
parameters
```

Response:

```text
commandId
status
errorCode
message
data
```

---

## 40. Command examples

```text
LockDevice
UnlockDevice

ShowMessage

LaunchApplication
OpenUrl

RestartDevice
ShutdownDevice

ApplyPolicy
RollbackPolicy

InstallPackage

StartRemoteControl
StopRemoteControl
```

---

## 41. Idempotency

Опасные команды должны иметь idempotency semantics.

Например network reconnect может привести к повторной отправке.

```text
commandId = UUID
```

Agent хранит короткий cache выполненных command IDs.

Повтор:

```text
same commandId
```

не выполняется заново.

---

## 42. Command deadlines

Команда:

```text
Launch VS Code
```

отправленная 10 минут назад после reconnect уже может быть бессмысленна.

Каждая команда имеет:

```text
expiresAt
```

Agent отклоняет expired command.

---

## 43. Authentication architecture

Нельзя доверять:

```text
IP
hostname
LAN
room
```

Каждое устройство получает cryptographic identity.

---

## 44. Device enrollment

Первоначальный workflow:

```text
Admin
↓
Create enrollment code

Installer
↓
Enter/scan code

Student Agent
↓
generate device key pair

Cloud
↓
validate enrollment code

Cloud
↓
issue device certificate
```

Enrollment code:

* one-time;
* expires;
* привязан к organization/branch.

---

## 45. Device private key

Не хранить:

```text
private-key.pem
```

в `ProgramData`.

Предпочтительно использовать Windows cryptographic storage:

```text
CNG / machine key store
```

и по возможности non-exportable key.

Future:

```text
TPM-backed key
```

при наличии TPM.

---

## 46. Local offline authorization

Проблема:

интернет пропал, а teacher должен продолжить урок.

Решение:

Cloud заранее выдаёт Teacher Console:

## signed classroom authorization lease

Например:

```text
teacherId
organizationId
branchId
allowedRooms[]
permissions[]
issuedAt
expiresAt
```

Срок:

например 12–24 часа.

Agent может проверить signature локально.

Следовательно:

```text
Cloud unavailable
+
valid lease
=
classroom works
```

---

## 47. Teacher authorization

Teacher может иметь permissions:

```text
classroom.view

classroom.remote_control

classroom.command

classroom.focus

device.restart

software.install
```

Обычный teacher не должен автоматически иметь:

```text
organization.admin
software.arbitrary_install
device.unenroll
```

---

## 48. Rooms

Устройство принадлежит:

```text
Organization
↓
Branch
↓
Room
```

Пример:

```text
KIBERone
└── Tushino
    ├── Room A
    └── Room B
```

Teacher lease может разрешать:

```text
Room A
```

но не весь филиал.

---

## 49. Audit

Любое privileged действие:

```text
teacher
action
device(s)
timestamp
result
```

Пример:

```text
2026-09-03T15:03

teacher_42
REMOTE_CONTROL_STARTED
device_PC_07

SUCCESS
```

Audit должен быть append-only с точки зрения обычного пользователя.

---

## 50. Local audit buffering

Интернет отсутствует:

```text
audit
↓
local durable queue
```

После reconnect:

```text
sync cloud
```

Audit events не должны теряться при обычном reboot.

---

## 51. Local persistent storage

Agent Service хранит состояние:

```text
C:\ProgramData\ClassOS\
```

Пример:

```text
ClassOS/
├── config/
├── state/
├── logs/
├── cache/
├── updates/
└── policies/
```

---

## 52. Local database

Для структурированного состояния:

## SQLite

Хранить:

```text
device metadata
cached policies
command deduplication
audit queue
update state
software inventory cache
```

Не хранить:

```text
screenshots
video
passwords
```

по умолчанию.

---

## 53. Logging

Structured logs.

Формат:

```text
timestamp
level
component
event
deviceId
sessionId
errorCode
context
```

Components:

```text
service
session
network
capture
input
policy
software
update
```

---

## 54. Sensitive logging rules

Никогда не логировать:

```text
private keys
access tokens
student passwords
full screen image
clipboard content
AI prompt containing secret data
```

---

## 55. Device health

Service собирает:

```text
uptime
CPU
RAM
disk

Windows version
hostname

agent version

active session

software profile status

policy status
```

Health state:

```text
Healthy
Warning
Critical
Offline
```

---

## 56. Health evaluation

Не cloud-only.

Agent сам рассчитывает basic status.

Пример:

```text
disk > 90%
→ Warning

required package missing
→ Warning

policy apply failed
→ Critical
```

Teacher видит состояние даже локально.

---

## 57. Process monitoring

На MVP:

Win32 process enumeration.

Получаем:

```text
pid
exe path
user
start time
```

Future:

ETW.

Не внедрять ETW до появления реальной необходимости.

---

## 58. Application identity

Не идентифицировать программы только по:

```text
processName == "Code.exe"
```

Нужна abstraction:

```text
ApplicationDefinition
```

```text
id:
vscode

displayName:
Visual Studio Code

executables:
Code.exe

publisher:
Microsoft Corporation

installDetection:
...
```

---

## 59. Application catalog

Cloud поддерживает curated catalog:

```text
VS Code
Python
Node.js
Git
Chrome
Roblox Studio
Unity Hub
Blender
```

Branch может добавить custom application.

---

## 60. Launching applications

Teacher не должен отправлять:

```text
C:\whatever\program.exe
```

по умолчанию.

Teacher отправляет:

```text
LaunchApplication {
  applicationId: "vscode"
}
```

Agent разрешает path через Application Catalog.

Это уменьшает возможность abuse.

---

## 61. Arbitrary execution

Arbitrary executable/PowerShell command:

> только Admin role.

И лучше вообще не включать в Teacher Console.

---

## 62. Policy Engine

Создаём независимый crate:

```text
policy-engine
```

API:

```rust
trait PolicyProvider {
    fn check_support(&self) -> Capability;
    fn current_state(&self) -> Result<State>;
    fn apply(&self, policy: &Policy) -> Result<ApplyResult>;
    fn rollback(&self, snapshot: &PolicySnapshot) -> Result<()>;
}
```

---

## 63. Policy model

Product-facing policy:

```text
LessonPolicy
```

не должна напрямую содержать registry keys.

Пример:

```yaml
name: Python Focus

applications:
  allow:
    - vscode
    - python
    - chrome

system:
  settings: blocked
  personalization: blocked
  powershell: blocked
  cmd: blocked

browser:
  allowed_urls:
    - docs.python.org
    - github.com
```

---

## 64. Policy translation

```text
ClassOS Policy
      │
      ▼
Policy Compiler
      │
      ├── Assigned Access
      ├── AppLocker
      ├── registry/GPO/CSP
      ├── Browser policies
      └── Firewall
```

Product layer не знает implementation details.

---

## 65. Assigned Access

Windows Assigned Access поддерживает restricted user experience, где пользователь может запускать только заданный список приложений; Windows также применяет соответствующие policy settings и AppLocker rules.

Это один из enforcement providers.

Но:

> ClassOS Policy Engine не должен зависеть исключительно от Assigned Access.

Причина:

* разные editions;
* разные сценарии;
* нужен dynamic Lesson Policy;
* некоторые ограничения удобнее реализовать иначе.

---

## 66. AppLocker

AppLocker имеет PowerShell tooling для:

```text
Get policy
Create policy
Set policy
Test policy
```

что позволяет ClassOS автоматически создавать, проверять и применять правила.

Очень важно:

перед enforcement применять:

```text
Test-AppLockerPolicy
```

или эквивалентную validation логику.

---

## 67. Safe policy rollout

Никогда:

```text
generate policy
↓
apply blindly
```

Workflow:

```text
Compile
↓
Validate
↓
Check required ClassOS components
↓
Snapshot current state
↓
Apply
↓
Verify
↓
Commit
```

Failure:

```text
Rollback snapshot
```

---

## 68. Never block ClassOS

Policy Compiler автоматически добавляет allow rules для:

```text
classos-service.exe
classos-session.exe
classos-updater.exe
```

иначе одна ошибка policy может заблокировать собственный management layer.

---

## 69. Break-glass

Admin должен иметь emergency method:

```text
ClassOS Recovery
```

который:

* отключает active Lesson Policy;
* восстанавливает base state;
* доступен только local administrator.

Это необходимо на случай bug.

---

## 70. Policy layering

Не существует одной policy.

```text
BASE DEVICE POLICY
       +
BRANCH POLICY
       +
ROOM POLICY
       +
LESSON POLICY
       +
TEMPORARY TEACHER OVERRIDE
```

Compiler создаёт:

```text
EffectivePolicy
```

---

## 71. Example

Base:

```text
Block Steam
Block PowerShell
```

Room:

```text
Block Discord
```

Lesson:

```text
Allow VS Code
Allow Chrome
```

Temporary:

```text
Allow Blender 20 minutes
```

Effective:

рассчитывается deterministically.

---

## 72. Policy rollback

При `Finish Lesson`:

не просто:

```text
remove everything
```

а:

```text
Lesson Policy removed
↓
recalculate EffectivePolicy
↓
Base + Branch + Room remains
```

---

## 73. Focus Mode

Focus Mode — не отдельный low-level mechanism.

Это:

```text
Temporary Policy Overlay
```

Например:

```text
Focus:
allow current lesson applications
block everything else
browser restricted
```

Выключение Focus:

```text
remove overlay
↓
restore effective Lesson Policy
```

---

## 74. Lock Screen

Для MVP Lock может быть реализован Session Host overlay:

```text
full-screen topmost overlay
```

с сообщением:

```text
Экран временно заблокирован преподавателем
```

Но overlay **не является security boundary**.

Настоящие app restrictions обеспечиваются Policy Engine.

---

## 75. Browser policies

Browser filtering реализуем через официальные enterprise policy mechanisms Chrome/Edge.

Product abstraction:

```text
BrowserPolicy

allowUrls
blockUrls
allowIncognito
allowDownloads
extensions
```

Не начинаем с MITM/proxy interception.

---

## 76. Network enforcement

MVP:

```text
browser policy
+
Windows Firewall where useful
```

Future advanced filtering:

## Windows Filtering Platform

Но WFP требует существенно более сложной разработки и не нужен для первого PMF.

---

## 77. Software Manager

Module:

```text
software-manager
```

Основные операции:

```text
detect
install
uninstall // admin only
repair
version
```

---

## 78. WinGet

Используем Windows Package Manager там, где package доступен.

Cloud ApplicationDefinition:

```text
id: python

wingetId:
Python.Python.3.13

approvedVersion:
...
```

---

## 79. WinGet Configuration

Windows WinGet Configuration позволяет декларативно описывать packages, dependencies и machine settings в YAML и использует PowerShell DSC для приведения системы к desired state.

Позже Software Profile может компилироваться в WinGet Configuration.

Но MVP:

> простые package operations через WinGet.

---

## 80. Software Profile

```text
Python Classroom v1
```

содержит:

```text
Python
VS Code
Git
Chrome

optional:
VS Code Python extension
course files
```

---

## 81. Desired State

Для каждого Room:

```text
desiredProfileId
```

Для device рассчитываем:

```text
Desired State
vs
Actual State
```

---

## 82. Drift example

```text
PC-07

Python
required 3.13.x
actual missing

VS Code
required installed
actual installed

Git
required installed
actual installed
```

State:

```text
DRIFTED
```

---

## 83. Repair

Admin нажимает:

```text
Repair PC-07
```

Agent получает:

```text
RepairDesiredState
```

и выполняет:

```text
install Python
verify
report
```

---

## 84. Package execution security

Не принимать arbitrary WinGet query от teacher.

Все installations идут через approved package catalog.

Иначе Teacher Console превращается в remote code execution system.

---

## 85. Windows Service resilience

SCM должен автоматически восстанавливать service.

Windows Service Control Manager поддерживает failure actions, включая restart service, через `SERVICE_FAILURE_ACTIONS`/`ChangeServiceConfig2`.

Recommended:

```text
Failure 1:
restart after 5 sec

Failure 2:
restart after 15 sec

Failure 3:
restart after 60 sec
```

---

## 86. Watchdog

Не создавать второй privileged watchdog без необходимости.

Сначала использовать:

```text
Windows SCM recovery
```

Если этого окажется недостаточно — отдельный updater/watchdog можно добавить позже.

---

## 87. Service boot lifecycle

```text
Windows boot
↓
SCM starts ClassOS Service
↓
load device identity
↓
load cached policies
↓
initialize network
↓
initialize cloud sync
↓
detect sessions
↓
start Session Host
↓
announce device
↓
ONLINE
```

---

## 88. Network failure lifecycle

```text
CONNECTED
↓
network lost
↓
LOCAL_ONLY
↓
retry cloud
↓
local classroom remains available
↓
network restored
↓
sync queues
↓
CONNECTED
```

---

## 89. Teacher disconnect

Remote control:

```text
Teacher connection lost
↓
stop remote input
↓
clear remote owner
↓
student indicator removed
```

Не должно оставаться:

```text
ghost teacher session
```

---

## 90. Session Host crash

```text
Session Host crash
↓
Service detects pipe disconnect
↓
wait/backoff
↓
restart Session Host
```

Screen streaming временно прекращается.

Policy enforcement остаётся, потому что оно принадлежит Service/Windows.

---

## 91. Teacher Console architecture

Tauri frontend:

```text
React
TypeScript
```

Rust Tauri backend:

```text
device discovery
secure transport
screen decoding
local caching
native networking
```

Frontend не должен напрямую заниматься binary screen protocol.

---

## 92. Teacher application state

Core entities:

```text
Organization
Branch
Room

Device
DeviceConnection

ClassroomSession

RemoteSession

Policy
LessonProfile
```

---

## 93. Teacher screen grid architecture

Frontend не хранит giant base64 strings.

Pipeline:

```text
network frame
↓
Tauri native
↓
decode
↓
native/shared buffer
↓
UI rendering
```

Реализация может эволюционировать, но:

> нельзя гонять 20 JPEG через JSON bridge как base64.

---

## 94. Adaptive thumbnail scheduling

Teacher Console сообщает Agent:

```text
Visible
Hidden
Selected
```

Если classroom tab hidden:

```text
thumbnail rate ↓
```

Если PC selected:

```text
selected stream ↑
```

Это сильно экономит ресурсы.

---

## 95. Connection manager

Teacher Console поддерживает:

```text
deviceId → connection state
```

States:

```text
Discovered
Connecting
Authenticating
Connected
Degraded
Disconnected
Unauthorized
UpgradeRequired
```

---

## 96. Bulk commands

Teacher нажимает:

```text
Lock All
```

Teacher Console **не должен выполнять команды строго последовательно**.

Используем concurrent fanout:

```text
12 devices
↓
parallel command dispatch
↓
collect results
```

UI:

```text
11 Success
1 Failed
```

---

## 97. Partial failure philosophy

Никаких:

> «Операция выполнена».

если 4 компьютера не ответили.

Всегда показывать:

```text
18 / 20 successful

PC-07 offline
PC-12 policy failed
```

---

## 98. Cloud architecture v0

```text
API
│
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

Один modular monolith.

Не microservices.

---

## 99. Cloud stack

```text
Bun
TypeScript

PostgreSQL

HTTP API
WebSocket only where needed
```

Redis:

не использовать пока реальная нагрузка не потребует.

---

## 100. Database core

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

lesson_profiles

enrollment_tokens

audit_events

agent_versions
```

---

## 101. Device entity

```text
Device

id
organizationId
branchId
roomId

hostname

osVersion
architecture

agentVersion

createdAt
lastSeenAt

status
```

---

## 102. Device secrets

PostgreSQL никогда не хранит:

```text
device private key
```

Cloud знает только:

```text
public identity
certificate metadata
```

---

## 103. Update architecture

Обновление agent — P0 infrastructure после первого пилота.

Нельзя вручную обновлять 100 endpoints.

---

## 104. Update channel

Versions:

```text
stable
beta
canary
```

Room/branch может быть:

```text
stable
```

Design-partner room:

```text
beta
```

---

## 105. Update manifest

Cloud:

```text
version
url
sha256
signature
minimumSupportedVersion
releaseChannel
```

Agent:

```text
download
↓
verify hash
↓
verify signature
↓
stage
↓
install
↓
health check
```

---

## 106. Code signing

Все Windows executables:

```text
classos-service.exe
classos-session.exe
classos-installer.exe
classos-updater.exe
```

должны быть Authenticode signed для production.

Windows предоставляет стандартную Authenticode/WinVerifyTrust инфраструктуру для проверки подписанного executable content.

---

## 107. Self-update problem

Service не должен заменять работающий executable сам.

Используем:

```text
ClassOS Service
↓
downloads update
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

---

## 108. Rollback

Update считается successful только если:

```text
Service running
+
version matches
+
Session Host starts
+
self-test passes
```

Failure:

```text
rollback previous bundle
```

---

## 109. Installer

Installer responsibilities:

```text
verify Windows version

install binaries

register ClassOSAgent service

configure recovery

configure firewall rules

enrollment

start service
```

---

## 110. MSI vs bootstrapper

MVP может использовать собственный signed bootstrapper.

Production желательно иметь:

```text
MSI
```

или другой enterprise-friendly package format.

Причина:

* массовый deployment;
* Intune;
* GPO;
* SCCM;
* RMM.

---

## 111. Zero-touch installation target

В будущем:

```text
classos-installer.exe --token ABC...
```

После запуска:

```text
device enrolled
service installed
agent online
```

Без ручной настройки.

---

## 112. Security threat model

ClassOS обладает очень мощными возможностями:

```text
screen access
input injection
application launch
policy modification
software installation
restart/shutdown
```

Следовательно, compromise ClassOS может дать attacker серьёзный контроль над школой.

Security — не secondary concern.

---

## 113. Threat: rogue teacher

Teacher account compromised.

Mitigation:

```text
RBAC
room scoping
short-lived tokens
audit
no arbitrary command execution
```

---

## 114. Threat: rogue LAN client

Атакующий находится в Wi-Fi школы.

Mitigation:

```text
discovery unauthenticated
BUT

all control connections authenticated

device certificates
teacher authorization lease
encrypted transport
```

---

## 115. Threat: student attacks Agent

Student has local standard account.

Mitigation:

```text
Service LocalSystem

Session IPC strict ACL

Service binaries protected

student no admin

private key unavailable to user

signed updates
```

---

## 116. Threat: malicious Session Host message

Session Host считается:

## partially untrusted

Потому что он работает в user session.

Service никогда не должен принимать от Session Host:

```text
"install arbitrary MSI"
"disable policy"
"run command as SYSTEM"
```

без строгой validation.

---

## 117. Privilege boundary

```text
USER SESSION
ClassOS Session Host
      │
      │ restricted IPC
      ▼
SECURITY BOUNDARY
      │
      ▼
ClassOS Service
LocalSystem
```

Любое сообщение, пересекающее boundary, рассматривается как untrusted input.

---

## 118. Threat: update supply chain

Mitigation:

```text
signed manifests
signed binaries
hash verification
TLS
rollback
restricted update origin
```

Нельзя доверять URL только потому, что он пришёл из API.

---

## 119. Threat: policy lockout

Policy случайно блокирует Windows/ClassOS.

Mitigation:

```text
compile
test
snapshot
always-allow ClassOS
apply
verify
rollback
break-glass
```

---

## 120. Threat: surveillance abuse

Product rule:

не реализуем:

```text
keylogging
hidden recording
microphone spying
camera spying
after-hours monitoring
```

Classroom monitoring существует только в рамках разрешённых use cases.

---

## 121. Privacy

Screen frames:

```text
RAM
↓
network
↓
Teacher screen
↓
discard
```

По умолчанию:

```text
NO persistent recording
```

---

## 122. AI future boundary

Когда появится AI Vision:

```text
telemetry trigger
↓
capture single frame
↓
redaction where applicable
↓
analysis
↓
discard raw frame
```

Raw frame storage:

opt-in enterprise feature, если вообще понадобится.

---

## 123. Telemetry architecture future

Не связывать core transport непосредственно с AI.

Agent emits structured events:

```text
ApplicationStarted
ApplicationStopped

ForegroundChanged

IdleChanged

CompilationFailed

ProjectChanged

TeacherIntervention
```

Event Bus:

```text
Agent
↓
ClassOS Events
↓
Rules
↓
AI Supervisor
```

---

## 124. Lesson Engine future integration

Core Device Agent не должен знать AlfaCRM.

Cloud Lesson Engine говорит:

```text
LessonSession Started

Room:
A

Profile:
Python

Students:
...
```

Policy/Device layer получает только:

```text
Apply Lesson Profile
```

---

## 125. AlfaCRM boundary

```text
AlfaCRM Adapter
↓
ClassOS canonical domain
↓
Lesson Engine
↓
Device orchestration
```

Никаких AlfaCRM entity IDs внутри Windows Agent protocol.

---

## 126. Domain separation

### Device domain

```text
Device
Room
Policy
Software
Health
```

### Education domain

```text
Student
Teacher
Course
Lesson
Group
```

### Classroom domain

```text
ClassroomSession
LessonProfile
DeviceSession
```

Это позволит позже подключить не только AlfaCRM.

---

## 127. Failure taxonomy

Все ошибки должны иметь machine-readable codes.

Пример:

```text
DEVICE_OFFLINE

AUTH_FAILED

PROTOCOL_UNSUPPORTED

SCREEN_CAPTURE_FAILED

REMOTE_INPUT_DENIED

APPLICATION_NOT_FOUND

POLICY_VALIDATION_FAILED

POLICY_APPLY_FAILED

PACKAGE_INSTALL_FAILED

UPDATE_SIGNATURE_INVALID
```

Не строить логику на строках ошибок.

---

## 128. Observability

Cloud:

```text
agent online rate

command success rate

policy failure rate

update failure rate

screen startup failure rate
```

Не отправлять giant debug logs постоянно.

Agent загружает:

```text
health metrics
errors
important diagnostics
```

---

## 129. Support bundle

Admin может нажать:

```text
Generate diagnostics
```

Agent создаёт:

```text
agent version
Windows version
service status
policy state
recent errors
network diagnostics
```

Без:

```text
screenshots
user documents
browser history
```

---

## 130. Testing strategy

Нельзя тестировать ClassOS только unit tests.

Нужно минимум четыре уровня.

---

## 131. Unit tests

Особенно:

```text
protocol
policy compiler
authorization
state machines
version negotiation
```

---

## 132. Windows integration tests

Отдельные Windows runners/VM.

Тестируем:

```text
Service install/start

Named Pipe ACL

Session Host launch

policy apply/rollback

reboot persistence
```

---

## 133. Multi-VM classroom tests

CI/lab:

```text
Teacher VM
+
Student VM 1
+
Student VM 2
```

Автоматический smoke test:

```text
discover
connect
screen
command
policy
restart
reconnect
```

---

## 134. Real hardware tests

DXGI/remote screen нельзя полностью доверить VM tests.

Минимальная test matrix:

```text
Intel integrated GPU
AMD
NVIDIA

1080p
4K

single display
dual display

Windows 10
Windows 11
```

---

## 135. Chaos testing

Проверить:

```text
pull network cable
kill Session Host
kill Teacher Console
restart Windows
sleep/wake
switch user
lock/unlock
cloud offline
```

ClassOS должен восстанавливаться автоматически.

---

## 136. Performance targets MVP

### Agent Service

Idle CPU:

```text
<1%
```

Idle RAM:

```text
target <100 MB
```

---

## 137. Thumbnail mode

На Student:

```text
CPU target low enough
to be invisible during lesson
```

Главная проверка:

> Roblox/Unity/VS Code не должны заметно тормозить из-за ClassOS.

---

## 138. Network target

20-PC classroom:

Thumbnail traffic должен быть достаточно низким для обычной school LAN/Wi-Fi.

Нельзя проектировать продукт под:

```text
20 × 10 Mbps
```

thumbnail stream.

---

## 139. Latency

Remote-control selected device:

target LAN input-to-screen:

```text
<150 ms
```

Хорошо:

```text
<80 ms
```

Это не hard SLA MVP, а performance target.

---

## 140. Startup

Agent:

после Windows boot:

```text
online <30 sec
```

Teacher Console:

device discovery:

```text
first devices <5 sec
```

на нормальном LAN.

---

## 141. Compatibility policy

Начальный supported baseline:

```text
Windows 11 Pro
Windows 11 Education
Windows 11 Enterprise
```

Windows 10 можно поддержать, если design-partner hardware требует.

Не обещать:

```text
every Windows since Windows 7.
```

---

## 142. Architecture Decision Records

Каждое серьёзное решение отдельно фиксируется:

```text
docs/adr/
```

Пример:

```text
0001-rust-agent.md

0002-service-session-separation.md

0003-dxgi-screen-capture.md

0004-protobuf-protocol.md

0005-local-first-control.md

0006-policy-engine.md
```

---

## 143. ADR-0001

### Rust for Windows Agent

Decision:

```text
Rust
```

Reasons:

* native binary;
* good Win32 bindings;
* memory safety;
* low overhead;
* reusable native code Teacher/Tauri;
* strong networking ecosystem.

---

## 144. ADR-0002

### Service + Session Host

Decision:

разделить privileged и interactive code.

Reason:

Windows Session 0 architecture + security boundaries. Microsoft прямо не рекомендует interactive service design на современных Windows.

---

## 145. ADR-0003

### DXGI Desktop Duplication

Decision:

основной screen backend.

Reason:

Windows remote/collaboration use case, GPU surfaces и change metadata.

---

## 146. ADR-0004

### Named Pipe for local IPC

Decision:

Service ↔ Session Host.

Reason:

native local duplex IPC + Windows ACL.

Explicit DACL mandatory.

---

## 147. ADR-0005

### Local-first classroom

Decision:

Teacher ↔ Agent communication работает без Cloud.

Reason:

интернет не может быть SPOF реального урока.

---

## 148. ADR-0006

### Product Policy abstraction

Decision:

UI/API никогда не оперируют registry/GPO напрямую.

Reason:

Windows enforcement mechanisms могут меняться независимо от product-level model.

---

## 149. MVP implementation order

Не писать всё одновременно.

---

### Milestone T0

#### Windows skeleton

```text
ClassOS Service
+
Session Host
+
Named Pipe
```

DoD:

Service запускает Session Host.

Они обмениваются heartbeat.

---

## 150. Milestone T1

### Device connection

```text
Teacher Console
↓
discover device
↓
authenticate
↓
DeviceHello
```

DoD:

Teacher видит online machine.

---

## 151. Milestone T2

### Screenshot

```text
DXGI
↓
JPEG
↓
Teacher
```

DoD:

кнопка:

```text
Take Screenshot
```

показывает desktop.

---

## 152. Milestone T3

### Continuous thumbnails

```text
1 FPS
```

DoD:

4–10 PCs одновременно.

---

## 153. Milestone T4

### Remote control

```text
mouse
keyboard
```

DoD:

полностью управляем Student PC.

---

## 154. Milestone T5

### Classroom commands

```text
lock
message
open app
open URL
restart
shutdown
```

---

## 155. Milestone T6

### Policy Engine

Первый policy:

```text
block specific app
```

Apply + rollback.

---

## 156. Milestone T7

### Focus Mode

Teacher:

```text
Allow:
VS Code

Focus ON
```

Student не может запустить тестовое запрещённое приложение.

---

## 157. Milestone T8

### Device health

```text
disk
RAM
Windows
software
```

---

## 158. Milestone T9

### Installer + updater

До подключения второго филиала.

---

## 159. Definition of Technical MVP

Technical MVP считается готовым, когда:

```text
10 Windows student PCs
+
1 Teacher PC
```

могут:

1. автоматически обнаружиться;
2. безопасно соединиться;
3. показать screen thumbnails;
4. открыть live screen;
5. дать remote control;
6. выполнить classroom command;
7. применить Focus Mode;
8. пережить reboot;
9. автоматически reconnect;
10. обновиться без ручного обхода кабинета.

---

## 160. Hard architectural invariants

Эти правила нельзя нарушать ради скорости.

### I

```text
LocalSystem Service
≠
Interactive UI
```

---

### II

```text
Discovery
≠
Trust
```

---

### III

```text
Student session
≠
Privileged authority
```

---

### IV

```text
Policy
must always have rollback
```

---

### V

```text
Cloud outage
must not stop lesson
```

---

### VI

```text
Remote control
must be authenticated + audited
```

---

### VII

```text
Screens
are ephemeral by default
```

---

### VIII

```text
Teacher
cannot execute arbitrary SYSTEM commands
```

---

### IX

```text
Updates
must be signed
```

---

### X

```text
Product API
must not expose raw Windows implementation
```

---

## 161. What Codex should build first

Первый engineering task должен быть исключительно:

## `T0 — Service / Session Host skeleton`

Не screen streaming.

Не Tauri.

Не cloud.

Не policies.

---

## 162. T0 exact scope

Создать Rust workspace:

```text
crates/
├── agent-service
├── agent-session
├── protocol
├── windows-platform
└── common
```

`agent-service`:

```text
runs as console during development
+
supports Windows Service mode later
```

`agent-session`:

```text
normal desktop process
```

---

## 163. T0 protocol

Только:

```text
Hello
Heartbeat
Ping
Pong
GetSessionInfo
SessionInfo
```

---

## 164. T0 IPC

Named Pipe:

```text
\\.\pipe\classos\session-{id}
```

Service:

```text
server
```

Session Host:

```text
client
```

---

## 165. T0 success output

Запускаем:

```text
classos-service
```

Service:

```text
Interactive session detected: 1
Starting session host...
Session host connected.
User: student
Heartbeat OK.
```

Session Host:

```text
Connected to ClassOS Service.
Session: 1
```

---

## 166. T1

После этого только:

## `Teacher ↔ Agent network protocol`

---

## 167. T2

После стабильного protocol:

## `DXGI screenshot`

---

## 168. Why this order matters

Самый плохой вариант развития:

```text
React dashboard
↓
AI UI
↓
fake device cards
↓
months later
↓
Windows agent doesn't work reliably
```

Самый правильный:

```text
Windows primitive
↓
reliability
↓
protocol
↓
Teacher UX
↓
product
```

ClassOS живёт или умирает на качестве Agent.

---

## 169. Architecture after PMF

После подтверждения продукта:

```text
             ClassOS Platform

Device Plane
├── Windows
└── Linux

Control Plane
├── Policies
├── Software
└── Updates

Classroom Plane
├── Teacher
├── Lesson Sessions
└── Student Identity

Integration Plane
├── AlfaCRM
├── LMS
└── SSO

Intelligence Plane
├── Telemetry
├── AI Supervisor
├── AI Tutor
└── Analytics
```

Но **сейчас существует только Device Plane + Teacher Control Plane.**

---

## 170. Final architecture principle

ClassOS должен оставаться системой, где:

```text
Windows
делает низкоуровневую работу

Agent
переводит ClassOS intentions
в Windows actions

Teacher Console
управляет классом

Cloud
связывает организацию

Lesson Engine
понимает учебный контекст

AI
понимает, где нужна помощь
```

Именно такое разделение позволит пройти путь:

```text
Veyon Replacement
↓
Classroom Control
↓
Device Management
↓
Lesson Orchestration
↓
AI Classroom
↓
Enterprise Education Platform
```

без необходимости переписывать фундамент на каждом новом этапе.

---

## 171. Immediate engineering objective

Следующий шаг после принятия RFC:

```text
T0
```

Создать:

```text
Rust workspace

ClassOSAgent Windows Service skeleton

ClassOS Session Host

secure local Named Pipe IPC

session discovery

Session Host lifecycle

heartbeat/restart
```

Никакого другого функционала до выполнения T0.

После T0:

```text
T1 Network
↓
T2 DXGI
↓
T3 Streaming
↓
T4 Remote Control
```

И только после этого начинается настоящий Teacher Console.

---

## 172. Technical North Star

Не количество функций.

Не количество Windows API.

Не красота architecture diagrams.

Главная техническая характеристика ClassOS:

> **Преподаватель включает компьютерный класс и ClassOS просто работает.**

Без:

* ручного reconnect;
* шаманства с firewall;
* перезапуска service;
* постоянного reinstall;
* плясок с Windows user sessions;
* настройки каждого ПК;
* случайно пропавших устройств.

Для classroom infrastructure:

## reliability is the feature

Если ClassOS менее надёжен, чем Veyon, всё остальное не имеет значения.
