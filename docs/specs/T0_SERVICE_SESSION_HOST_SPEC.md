# ClassOS — T0 Implementation Specification

**Файл:** `docs/specs/T0_SERVICE_SESSION_HOST_SPEC.md`
**Статус:** Implementation-ready
**Milestone:** T0
**Цель:** запустить надёжную пару `ClassOS Service ↔ ClassOS Session Host` на Windows и подготовить фундамент для дальнейших T1–T4.

---

## 1. Цель T0

T0 не должен показывать экран.

T0 не должен иметь Teacher Console.

T0 не должен иметь Cloud.

T0 должен доказать фундаментальную архитектуру ClassOS:

```text
Windows Service
LocalSystem
      │
      │ secure IPC
      ▼
Session Host
interactive user session
```

После T0 ClassOS должен уметь:

1. работать как настоящий Windows Service;
2. определять активную пользовательскую session;
3. запускать `classos-session.exe` именно внутри этой session;
4. создавать защищённый локальный IPC;
5. проходить handshake между Service и Session Host;
6. обмениваться heartbeat;
7. обнаруживать crash Session Host;
8. автоматически его перезапускать;
9. корректно обрабатывать logon/logout/lock/unlock;
10. переживать reboot.

---

## 2. Definition of Done

T0 считается завершённым, если на Windows-машине можно выполнить:

```text
Install ClassOSAgent service
↓
Restart Windows
↓
Login as standard user
↓
classos-session.exe starts automatically
↓
Session Host connects to Service
↓
Heartbeat works
```

После:

```text
taskkill /IM classos-session.exe /F
```

Service обнаруживает disconnect и автоматически восстанавливает Session Host.

После logout:

```text
Session Host terminates
```

После login другого пользователя:

```text
new Session Host launches
```

После:

```text
sc stop ClassOSAgent
```

Service корректно завершает:

* IPC;
* supervision;
* Session Host;
* logging;
* runtime.

---

## 3. Что НЕ входит в T0

Запрещено добавлять в этот milestone:

```text
DXGI
screenshots
screen streaming

Teacher Console
Tauri

network discovery

remote mouse
remote keyboard

policies

AppLocker

WinGet

AlfaCRM

Cloud

AI
```

Если во время реализации появляется желание:

> «Заодно давайте добавим…»

ответ:

> нет.

---

## 4. Target platform

Основная development platform:

```text
Windows 11 x64
```

Target:

```text
x86_64-pc-windows-msvc
```

Минимальная поддержка первого engineering prototype:

```text
Windows 11 Pro
```

После T0/T1 проверить:

* Windows 11 Education;
* Windows 11 Enterprise.

Windows 10 пока не является обязательной целью.

---

## 5. Toolchain

Использовать:

```text
Rust stable
MSVC toolchain
Cargo workspace
```

Rust target:

```bash
rustup target add x86_64-pc-windows-msvc
```

---

## 6. Основные Rust dependencies

На момент написания документа актуальный `windows` crate находится на версии `0.62.2`.

Рекомендуемый baseline:

```toml
windows = "0.62"
windows-service = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time", "signal"] }
prost = "0.14"
prost-build = "0.14"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
```

`windows-service 0.8.1` предоставляет Rust abstraction для реализации и управления Windows Services, включая service control handling.

Все зависимости после создания проекта закрепляются:

```text
Cargo.lock
```

Не использовать:

```text
*
latest
git main branch
```

в production workspace.

---

## 7. Почему `windows-service`

Для SCM boilerplate не нужно писать весь FFI вручную.

Используем:

```text
windows-service
```

для:

* service dispatcher;
* service registration;
* status handling;
* stop handling;
* session change events;
* service installation helper.

Raw `windows` API используется там, где нужен непосредственно Win32:

```text
WTS
CreateProcessAsUser
Named Pipes
Security descriptors
tokens
handles
```

---

## 8. Repository structure

После T0:

```text
classos/
│
├── Cargo.toml
├── Cargo.lock
│
├── crates/
│   │
│   ├── agent-service/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── service.rs
│   │       ├── runtime.rs
│   │       └── supervisor.rs
│   │
│   ├── agent-session/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── runtime.rs
│   │       └── ipc_client.rs
│   │
│   ├── agent-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs
│   │       └── error.rs
│   │
│   ├── protocol/
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   └── src/
│   │       ├── lib.rs
│   │       └── framing.rs
│   │
│   └── windows-platform/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── handles.rs
│           ├── sessions.rs
│           ├── process.rs
│           ├── security.rs
│           └── pipes.rs
│
├── proto/
│   └── local_ipc.proto
│
├── scripts/
│   ├── install-service.ps1
│   ├── uninstall-service.ps1
│   └── status.ps1
│
└── docs/
    ├── 01_ROADMAP.md
    ├── 02_PRODUCT_ANALYSIS.md
    ├── 03_EXECUTION_PLAN_90_DAYS.md
    ├── 01_TECHNICAL_ARCHITECTURE.md
    └── T0_SERVICE_SESSION_HOST_SPEC.md
```

---

## 9. Workspace Cargo.toml

Root:

```toml
[workspace]
resolver = "2"

members = [
    "crates/agent-service",
    "crates/agent-session",
    "crates/agent-core",
    "crates/protocol",
    "crates/windows-platform",
]

[workspace.package]
edition = "2024"

[workspace.dependencies]
windows = "0.62"
windows-service = "0.8"
tokio = { version = "1", features = [
    "rt-multi-thread",
    "macros",
    "sync",
    "time",
    "signal"
] }
prost = "0.14"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1", features = ["v4"] }
```

Конкретные patch versions фиксируются через `Cargo.lock`.

---

## 10. Windows API feature flags

`windows-platform` не должен включать весь Win32.

Минимально потребуются namespaces уровня:

```text
Win32_Foundation

Win32_Security

Win32_Security_Authorization

Win32_System_Pipes

Win32_System_RemoteDesktop

Win32_System_Threading

Win32_System_Environment

Win32_System_WindowsProgramming

Win32_System_Memory

Win32_UI_WindowsAndMessaging
```

Список уточняется компилятором.

Не включать огромный набор Windows APIs «на всякий случай».

---

## 11. Binary layout

T0 создаёт два executable.

### Service

```text
classos-service.exe
```

Назначение:

```text
privileged orchestration
```

---

### Session Host

```text
classos-session.exe
```

Назначение:

```text
interactive-session functionality
```

---

## 12. Development mode

Очень важно не требовать установки Windows Service для каждого запуска во время разработки.

`classos-service.exe` должен поддерживать:

```text
classos-service.exe run
```

и:

```text
classos-service.exe service
```

---

## 13. `run` mode

Запускается как обычный process из terminal.

Не LocalSystem.

Используется для:

* business logic;
* IPC;
* protocol;
* unit tests;
* debugging.

Но:

`WTSQueryUserToken` в таком режиме работать не обязан, потому что Microsoft требует для него контекст LocalSystem и соответствующую привилегию.

Поэтому development mode может запускать Session Host непосредственно как child process текущего пользователя.

---

## 14. `service` mode

Запуск осуществляется SCM.

```text
SCM
↓
classos-service.exe service
```

Здесь работает настоящий:

```text
LocalSystem
```

flow.

---

## 15. Commands

Service binary:

```text
classos-service.exe run

classos-service.exe service

classos-service.exe install

classos-service.exe uninstall

classos-service.exe status
```

`install/uninstall` можно сначала оставить в PowerShell script, если это заметно сокращает T0.

Но архитектура CLI должна допускать встроенную реализацию позднее.

---

## 16. Windows Service properties

Service name:

```text
ClassOSAgent
```

Display name:

```text
ClassOS Agent
```

Description:

```text
ClassOS classroom management agent.
```

Account:

```text
LocalSystem
```

Startup:

```text
Automatic
```

Service type:

```text
SERVICE_WIN32_OWN_PROCESS
```

---

## 17. Service states

Обязательные:

```text
START_PENDING
RUNNING
STOP_PENDING
STOPPED
```

Service должен корректно уведомлять SCM.

---

## 18. Accepted controls

Минимум:

```text
STOP

SHUTDOWN

SESSIONCHANGE
```

Service control handler должен поддерживать extended controls.

Windows предоставляет `RegisterServiceCtrlHandlerEx` для extended service controls.

---

## 19. Session change

Когда Service получает:

```text
SERVICE_CONTROL_SESSIONCHANGE
```

event data содержит `WTSSESSION_NOTIFICATION`, включая session ID.

Нас интересуют события:

```text
WTS_SESSION_LOGON

WTS_SESSION_LOGOFF

WTS_SESSION_LOCK

WTS_SESSION_UNLOCK

WTS_CONSOLE_CONNECT

WTS_CONSOLE_DISCONNECT
```

---

## 20. Event handler rule

SCM callback должен быть очень быстрым.

Нельзя внутри handler:

```text
launch processes

wait

open long IPC

perform network calls
```

Handler только отправляет event во внутренний channel:

```text
SCM callback
↓
mpsc::Sender<ServiceEvent>
↓
async runtime
```

---

## 21. Internal ServiceEvent

```rust
enum ServiceEvent {
    Stop,
    Shutdown,

    SessionLogon(u32),
    SessionLogoff(u32),

    SessionLock(u32),
    SessionUnlock(u32),

    ConsoleConnect(u32),
    ConsoleDisconnect(u32),
}
```

---

## 22. Runtime architecture

```text
SCM Thread
     │
     ▼
ServiceEvent channel
     │
     ▼
Tokio Runtime
     │
     ├── SessionSupervisor
     ├── IpcServer
     ├── HeartbeatMonitor
     └── ShutdownCoordinator
```

---

## 23. SessionSupervisor

Основной T0 component:

```rust
struct SessionSupervisor {
    state: SupervisorState,
}
```

---

## 24. Supervisor states

```rust
enum SupervisorState {
    NoInteractiveSession,

    Starting {
        session_id: u32,
    },

    WaitingForIpc {
        session_id: u32,
        pid: u32,
    },

    Running {
        session_id: u32,
        pid: u32,
    },

    Stopping {
        session_id: u32,
        pid: u32,
    },

    Backoff,
}
```

---

## 25. Session selection

Для MVP ClassOS обслуживает:

## физическую console session

Первичная функция:

```text
WTSGetActiveConsoleSessionId()
```

Microsoft определяет её как session, подключённую к физической console. Если console session временно отсутствует, возвращается `0xFFFFFFFF`.

---

## 26. Selection algorithm

```text
sessionId = WTSGetActiveConsoleSessionId()

if sessionId == 0xFFFFFFFF:
    NoInteractiveSession

else:
    inspect session state
```

При необходимости проверяем session через:

```text
WTSQuerySessionInformation
```

или `WTSEnumerateSessionsW`.

---

## 27. Why not blindly enumerate first

ClassOS изначально предназначен для физического ученического ПК.

Нам не нужно автоматически обслуживать:

```text
RDP admin session
disconnected session
background session
```

Главная session:

```text
physical console
```

---

## 28. Future multi-session

Архитектура не должна использовать global:

```text
Option<SessionHost>
```

внутри всех модулей.

Лучше:

```text
HashMap<SessionId, SessionHostState>
```

или abstraction, допускающую это.

Но T0 runtime фактически контролирует только одну active console session.

---

## 29. Получение user token

Для запуска interactive process используем:

```text
WTSQueryUserToken(session_id)
```

Microsoft сообщает, что эта функция возвращает primary token залогиненного пользователя и требует, чтобы caller работал как LocalSystem с `SE_TCB_NAME`.

Это подходит нашей Service architecture.

---

## 30. Token lifecycle

Обязательный RAII wrapper:

```rust
struct OwnedHandle(HANDLE);
```

`Drop`:

```text
CloseHandle()
```

Нельзя оставлять raw `HANDLE` разбросанными по business logic.

Microsoft отдельно предупреждает, что token handles от `WTSQueryUserToken` нужно закрывать после использования.

---

## 31. `OwnedHandle`

В `windows-platform/handles.rs`:

```rust
pub struct OwnedHandle {
    handle: HANDLE,
}
```

API:

```rust
impl OwnedHandle {
    pub fn raw(&self) -> HANDLE;
}
```

Implement:

```text
Drop
```

Без:

```text
Clone
```

если duplication handle не сделан явно.

---

## 32. User environment

Session Host должен получать нормальное окружение пользователя:

```text
USERPROFILE
APPDATA
LOCALAPPDATA
TEMP
PATH
...
```

Используем:

```text
CreateEnvironmentBlock
```

Microsoft прямо указывает, что полученный environment block можно передать в `CreateProcessAsUser`.

---

## 33. Environment RAII

Создать:

```rust
struct EnvironmentBlock {
    ptr: *mut c_void,
}
```

`Drop`:

```text
DestroyEnvironmentBlock
```

---

## 34. Session Host launch

Функция:

```rust
pub fn launch_in_session(
    session_id: u32,
    executable: &Path,
    args: &[OsString],
) -> Result<LaunchedProcess>
```

---

## 35. Launch pipeline

```text
Session ID
↓
WTSQueryUserToken
↓
CreateEnvironmentBlock
↓
build STARTUPINFO
↓
CreateProcessAsUserW
↓
close thread handle
↓
retain process handle / PID
```

---

## 36. CreateProcessAsUser

Microsoft определяет `CreateProcessAsUser` как создание процесса в security context пользователя, представленного primary token. Обычно caller требует `SE_INCREASE_QUOTA_NAME` и иногда `SE_ASSIGNPRIMARYTOKEN_NAME`; LocalSystem services имеют необходимые для такого сценария privileges.

---

## 37. Desktop

В `STARTUPINFO`:

```text
lpDesktop = "winsta0\\default"
```

чтобы процесс появился в interactive desktop пользователя.

---

## 38. Creation flags

Минимально:

```text
CREATE_UNICODE_ENVIRONMENT
```

Можно также использовать:

```text
CREATE_NEW_PROCESS_GROUP
```

если это пригодится для lifecycle.

Не добавлять flags без причины.

---

## 39. Session Host arguments

Service запускает:

```text
classos-session.exe
    --session-id 1
    --pipe \\.\pipe\classos-session-1-...
```

Но секреты не передавать через command line.

Command line можно прочитать из Process Explorer.

---

## 40. IPC authentication secret

В T0 предпочтительнее:

> не использовать отдельный shared secret.

Identity клиента подтверждается через Windows security:

* Named Pipe ACL;
* session SID;
* client process inspection при необходимости.

Позже можно добавить channel nonce.

---

## 41. Named Pipe naming

Не использовать только:

```text
\\.\pipe\classos-session-1
```

Лучше:

```text
\\.\pipe\classos\session-{sessionId}-{instanceId}
```

Например:

```text
\\.\pipe\classos\session-1-a32f...
```

`instanceId` генерируется Service при запуске Session Host.

---

## 42. Почему random instance ID

Снижает:

* collisions;
* stale connection bugs;
* spoofing convenience;
* confusion при rapid restart.

---

## 43. Pipe ownership

Service:

```text
SERVER
```

Session Host:

```text
CLIENT
```

---

## 44. Security descriptor

Это критично.

Microsoft указывает, что Named Pipe — securable object и при подключении проверяет access token клиента против ACL pipe.

Default ACL использовать нельзя.

---

## 45. Desired ACL

Full access:

```text
SYSTEM
```

Read/write:

```text
конкретный interactive user/session
```

Никакого:

```text
Everyone
Authenticated Users
Users
ANONYMOUS
NETWORK
```

---

## 46. ACL construction

Алгоритм:

```text
1. Получить SID user token.

2. Получить LocalSystem SID.

3. Построить explicit security descriptor.

4. Создать Named Pipe с SECURITY_ATTRIBUTES.

5. Передать descriptor в CreateNamedPipeW.
```

---

## 47. SDDL

Допускается использовать SDDL и:

```text
ConvertStringSecurityDescriptorToSecurityDescriptorW
```

для преобразования строки security descriptor в native descriptor. Microsoft предоставляет эту функцию именно для такого преобразования.

Но SID пользователя должен формироваться динамически.

Не hardcode:

```text
S-1-5-21-...
```

---

## 48. Network pipe access

Pipe предназначен только для локального Service ↔ Session Host.

Никакой remote Named Pipe functionality.

Это не Teacher transport.

---

## 49. IPC framing

Используем:

## length-prefixed protobuf

Frame:

```text
4-byte unsigned length
+
protobuf payload
```

Integer encoding:

```text
little-endian u32
```

Maximum T0 message:

```text
64 KiB
```

Messages больше лимита:

```text
ProtocolError::FrameTooLarge
```

---

## 50. Почему не JSON

T0 очень простой, но protocol потом будет расширяться.

Protobuf даёт:

* schema;
* generated types;
* compatibility;
* binary format;
* enum versioning.

---

## 51. Proto file

`proto/local_ipc.proto`

Пример:

```protobuf
syntax = "proto3";

package classos.local.v1;

message Envelope {
  string message_id = 1;

  oneof payload {
    SessionHello session_hello = 10;
    ServiceHello service_hello = 11;

    Ping ping = 12;
    Pong pong = 13;

    GetSessionInfo get_session_info = 14;
    SessionInfo session_info = 15;

    Shutdown shutdown = 16;
  }
}
```

---

## 52. SessionHello

```protobuf
message SessionHello {
  uint32 protocol_version = 1;
  uint32 session_id = 2;
  uint32 pid = 3;
}
```

---

## 53. ServiceHello

```protobuf
message ServiceHello {
  uint32 protocol_version = 1;
  string service_instance_id = 2;
}
```

---

## 54. Ping

```protobuf
message Ping {
  uint64 sequence = 1;
  int64 sent_at_unix_ms = 2;
}
```

---

## 55. Pong

```protobuf
message Pong {
  uint64 sequence = 1;
}
```

---

## 56. SessionInfo

```protobuf
message SessionInfo {
  uint32 session_id = 1;
  uint32 pid = 2;

  string username = 3;

  bool is_locked = 4;
}
```

Не добавлять в T0:

```text
email
student
organization
device
lesson
```

---

## 57. Protocol version

Constant:

```rust
pub const LOCAL_PROTOCOL_VERSION: u32 = 1;
```

Handshake должен fail-fast при mismatch.

---

## 58. IPC handshake

Flow:

```text
Session Host
↓
connect pipe

Session Host
→ SessionHello

Service validates:
- session id
- expected instance
- process/session relationship

Service
→ ServiceHello

Connection becomes:
AUTHENTICATED
```

---

## 59. Validation

Service не доверяет данным `SessionHello`.

Session Host говорит:

```text
pid = 4200
session = 1
```

Service должен проверить самостоятельно:

```text
ProcessIdToSessionId(pid)
```

и убедиться:

```text
actual session == expected session
```

---

## 60. Optional pipe client PID validation

Если используемый Windows API позволяет получить client process ID для named pipe:

```text
GetNamedPipeClientProcessId
```

использовать его вместо доверия payload.

Это preferred implementation.

---

## 61. IPC connection states

```rust
enum IpcState {
    Waiting,
    Connected,
    Handshaking,
    Ready,
    Closing,
}
```

---

## 62. Heartbeat

После успешного handshake:

Service отправляет:

```text
Ping
```

каждые:

```text
5 seconds
```

Session Host отвечает:

```text
Pong
```

---

## 63. Dead timeout

Если нет корректного Pong:

```text
15 seconds
```

connection считается unhealthy.

Но прежде чем kill/restart, проверить:

```text
process alive?
pipe state?
```

---

## 64. Why heartbeat

Pipe disconnect обычно обнаружит crash.

Но heartbeat нужен также для:

* hung Session Host;
* event loop stall;
* future health measurements.

---

## 65. Session Host heartbeat handling

Session Host не создаёт собственный independent heartbeat.

Service является authority.

```text
Service → Ping
Session → Pong
```

---

## 66. Session Supervisor loop

Conceptually:

```rust
loop {
    match state {
        NoInteractiveSession => ...
        Starting => ...
        WaitingForIpc => ...
        Running => ...
        Stopping => ...
        Backoff => ...
    }
}
```

Не распределять lifecycle по десяткам callback.

---

## 67. Desired state model

Supervisor всегда рассчитывает:

```text
desired state
```

из текущей Windows session.

Например:

```text
console user exists
→ Session Host SHOULD RUN
```

```text
no console user
→ Session Host SHOULD NOT RUN
```

Это лучше imperative:

```text
on event A do X
on event B do Y
```

---

## 68. Reconciliation loop

Даже если Windows event потерян:

каждые:

```text
10 seconds
```

Supervisor выполняет reconcile.

```text
determine active session
↓
compare desired vs actual
↓
repair
```

---

## 69. Why reconciliation

Система должна self-heal.

Если:

* Session Host неожиданно умер;
* service missed event;
* session changed;
* process orphaned;

следующий reconcile исправляет состояние.

---

## 70. Backoff

Если Session Host постоянно падает:

не restart loop:

```text
100 times/sec
```

Использовать exponential backoff:

```text
1s
2s
5s
10s
30s
60s max
```

После стабильной работы, например:

```text
60 seconds
```

backoff reset.

---

## 71. Crash storm

После, например:

```text
5 crashes / 2 minutes
```

log:

```text
SESSION_HOST_CRASH_LOOP
```

Supervisor продолжает recovery с большим backoff.

В T0 не выключать сервис навсегда.

---

## 72. Duplicate Session Host

Перед запуском нового host проверить, нет ли уже managed instance.

Нельзя просто искать:

```text
process name == classos-session.exe
```

потому что могут существовать:

* debug process;
* другая session;
* stale process.

Использовать:

```text
tracked PID
+
session ID
+
IPC instance ID
```

---

## 73. Child process handle

Service хранит process handle запущенного Session Host.

```rust
struct SessionProcess {
    session_id: u32,
    pid: u32,
    process_handle: OwnedHandle,
}
```

Это позволяет:

```text
wait for exit
query exit
terminate only managed process
```

---

## 74. Normal stop

Service shutdown:

```text
Service
→ Shutdown IPC

wait 3 sec

if Session Host exits:
    OK

else:
    terminate managed process
```

---

## 75. Session logout

При logout:

```text
desired state = NoInteractiveSession
```

Session Host может уже завершиться автоматически вместе с session.

Service:

* closes pipe;
* releases process handle;
* waits for next active session.

---

## 76. Lock behavior

На:

```text
WTS_SESSION_LOCK
```

Session Host продолжает существовать.

T0:

```text
is_locked = true
```

Никаких shutdown.

Позже screen capture policy можно изменить отдельно.

---

## 77. Unlock

```text
WTS_SESSION_UNLOCK
```

Session Host сообщает:

```text
is_locked = false
```

или Service обновляет session state.

---

## 78. RDP

T0 не гарантирует работу ClassOS в RDP session.

Если администратор подключился по RDP:

ClassOS продолжает ориентироваться на:

```text
physical console session
```

Нужно обязательно протестировать, чтобы RDP admin случайно не заменил Student Session Host.

---

## 79. Logging stack

Использовать:

```text
tracing
tracing-subscriber
```

В development:

```text
stdout
```

В service mode:

```text
file
```

---

## 80. Log directory

```text
C:\ProgramData\ClassOS\logs\
```

Файлы:

```text
service.log
session-{sessionId}.log
```

---

## 81. Log rotation

T0 достаточно:

```text
daily
```

или size-based rotation.

Не позволять логам расти бесконечно.

Target:

```text
max ~100 MB total
```

для T0 можно сделать простой retention.

---

## 82. Structured fields

Пример:

```text
level=INFO
component=session_supervisor
event=session_host_started
session_id=1
pid=4182
```

---

## 83. Required log events

Service startup:

```text
SERVICE_STARTING
SERVICE_RUNNING
SERVICE_STOPPING
SERVICE_STOPPED
```

Session:

```text
SESSION_DISCOVERED
SESSION_CHANGED
SESSION_HOST_STARTING
SESSION_HOST_STARTED
SESSION_HOST_CONNECTED
SESSION_HOST_DISCONNECTED
SESSION_HOST_EXITED
SESSION_HOST_RESTARTING
```

IPC:

```text
IPC_LISTENING
IPC_CONNECTED
IPC_HANDSHAKE_OK
IPC_HANDSHAKE_FAILED
IPC_HEARTBEAT_TIMEOUT
```

---

## 84. Error architecture

Core error:

```rust
#[derive(thiserror::Error, Debug)]
pub enum AgentError {
    ...
}
```

Не возвращать по проекту:

```text
String
```

как error type.

---

## 85. Error categories

```text
WindowsApi

SessionNotFound

UserTokenFailed

EnvironmentCreationFailed

ProcessLaunchFailed

PipeCreateFailed

PipeSecurityFailed

Protocol

HandshakeFailed

HeartbeatTimeout

Shutdown
```

---

## 86. Windows error context

При Win32 failure всегда сохранять:

```text
API
GetLastError / windows::core::Error
context
```

Пример:

```text
CreateProcessAsUserW failed
session_id=1
win32=1314
```

---

## 87. No panic rule

Service runtime не должен panic из-за:

```text
malformed IPC frame
missing session
failed process launch
```

Panic допустим только для invariant violation в development.

Production boundaries должны возвращать errors.

---

## 88. Unsafe rule

Любой Win32 FFI требует `unsafe`, но:

> unsafe должен быть изолирован внутри `windows-platform`.

Business crates:

```text
agent-service
agent-core
protocol
```

не должны содержать raw unsafe Win32 code без крайней необходимости.

---

## 89. Safe wrappers

Нужно написать wrappers для:

```text
HANDLE

environment block

WTS allocated memory

security descriptor memory

process/thread handles
```

Каждый автоматически освобождает resource через `Drop`.

---

## 90. Memory ownership

Для каждого Win32 API Codex должен проверить:

> кто обязан освобождать возвращённую память?

Например нельзя механически вызывать:

```text
free()
```

на buffer, который требует:

```text
WTSFreeMemory
```

или:

```text
LocalFree
```

---

## 91. Service install script

`scripts/install-service.ps1`

Должен:

1. проверить admin;
2. найти release binary;
3. создать `C:\Program Files\ClassOS`;
4. скопировать service/session binaries;
5. создать service;
6. установить automatic start;
7. запустить service;
8. показать status.

---

## 92. Installation directory

```text
C:\Program Files\ClassOS\
```

Binaries:

```text
classos-service.exe
classos-session.exe
```

---

## 93. Runtime directory

```text
C:\ProgramData\ClassOS\
```

Никогда не писать runtime data в:

```text
Program Files
```

---

## 94. Uninstall script

Должен:

```text
stop service
delete service
remove binaries
```

Но T0 по умолчанию можно оставить:

```text
ProgramData logs
```

если не указан:

```text
-Purge
```

---

## 95. Service recovery

После стабильного T0 настроить SCM failure recovery.

`windows-service` имеет поддержку service failure actions; актуальная версия содержит соответствующие service structures/examples.

Target:

```text
first failure:
restart 5 sec

second:
restart 15 sec

third+:
restart 60 sec
```

---

## 96. Dev testing command

Должна быть возможность:

Terminal A:

```bash
cargo run -p agent-service -- run
```

Terminal B:

```bash
cargo run -p agent-session -- --dev
```

В dev mode pipe может использовать ACL только текущего user.

---

## 97. Production testing command

```powershell
cargo build --release
.\scripts\install-service.ps1

Restart-Computer
```

После login:

```powershell
Get-Service ClassOSAgent
```

ожидается:

```text
Running
```

---

## 98. Unit tests

Обязательные T0 unit tests:

```text
protobuf encode/decode

frame encode/decode

frame too large

partial frame reads

multiple sequential frames

protocol version mismatch

heartbeat sequence handling

supervisor state transitions

backoff calculation
```

---

## 99. Framing tests

Named Pipe/TCP semantics могут разбивать write на части.

`FramedReader` обязан корректно обработать:

```text
2 bytes length
↓
pause
↓
2 bytes length
↓
half payload
↓
remaining payload
```

Нельзя предполагать:

```text
one read == one message
```

---

## 100. Integration test — IPC

Запустить fake server/client:

```text
Service Pipe
↔
Session Client
```

Проверить:

```text
handshake
ping
pong
shutdown
```

---

## 101. Integration test — ACL

Под настоящей Windows:

```text
User A
```

может открыть pipe своей session.

Другой test user:

```text
User B
```

не должен получить доступ.

Этот test может быть manual/VM automation, но должен существовать до production.

---

## 102. Integration test — launch

Настоящий installed Service:

```text
LocalSystem
```

после user login:

```text
launches Session Host
```

Проверить:

```text
Session Host user == logged-on user
```

и:

```text
Session Host session ID == console session
```

---

## 103. Integration test — environment

Session Host должен видеть корректные:

```text
USERPROFILE
LOCALAPPDATA
APPDATA
TEMP
```

Это проверяет, что `CreateEnvironmentBlock` используется корректно.

---

## 104. Integration test — restart Session Host

```text
kill classos-session.exe
```

Expected:

```text
Service detects exit
↓
backoff
↓
launches new instance
↓
IPC reconnect
```

---

## 105. Integration test — user logout/login

Scenario:

```text
Login User A

verify host A
↓
Logout

verify host stopped
↓
Login User B

verify host B
```

---

## 106. Integration test — lock/unlock

```text
Win + L
```

Service remains Running.

Session Host remains managed.

Unlock:

system returns Ready state.

---

## 107. Integration test — service restart

```text
Restart-Service ClassOSAgent
```

Expected:

```text
old managed Session Host exits
↓
Service restarts
↓
new Session Host launches
↓
new IPC instance established
```

---

## 108. Integration test — reboot

Главный T0 test:

```text
Restart Windows
↓
Login
↓
do nothing manually
```

Expected logs:

```text
SERVICE_RUNNING
SESSION_DISCOVERED
SESSION_HOST_STARTED
IPC_HANDSHAKE_OK
```

---

## 109. Acceptance Test A

### Fresh machine

Precondition:

```text
Windows 11
ClassOS absent
```

Steps:

```text
install
reboot
login
```

Pass:

```text
service Running
session host Running
IPC Ready
```

---

## 110. Acceptance Test B

### Session Host crash

```text
taskkill /F /PID <session-host>
```

Pass:

new Session Host:

```text
<30 sec
```

И желательно:

```text
<10 sec
```

---

## 111. Acceptance Test C

### Service crash

Принудительно убить Service.

SCM failure recovery должен вернуть его.

Затем Service должен снова reconcile interactive session.

---

## 112. Acceptance Test D

### Different user

User A logout.

User B login.

ClassOS Session Host должен работать как:

```text
User B
```

а не под token старого пользователя.

---

## 113. Acceptance Test E

### Pipe security

Непривилегированный другой пользователь не может подключиться к pipe.

---

## 114. Acceptance Test F

### 8 hours

Оставить машину на:

```text
8 hours
```

Heartbeat:

```text
stable
```

RAM service/session:

```text
not continuously increasing
```

No restart loop.

---

## 115. Memory target T0

Без screen capture:

Service:

```text
<50–70 MB target
```

Session Host:

```text
<30–50 MB target
```

Это не hard limit, но если пустой Session Host использует 300 MB — нужно разбираться.

---

## 116. CPU target

Idle:

```text
~0%
```

Оба процесса большую часть времени должны ждать:

* IPC;
* timer;
* Windows events.

Никаких busy polling loops.

---

## 117. Reconciliation frequency

Default:

```text
10 sec
```

Не:

```text
10 ms
```

Session events обеспечивают fast response.

Reconcile — safety net.

---

## 118. Heartbeat frequency

```text
5 sec
```

Timeout:

```text
15 sec
```

Config constants вынести:

```rust
const HEARTBEAT_INTERVAL: Duration = ...
const HEARTBEAT_TIMEOUT: Duration = ...
```

---

## 119. Config

T0 configuration:

```text
C:\ProgramData\ClassOS\config.toml
```

Но хранить там минимум.

Например:

```toml
log_level = "info"
```

Не делать большой config framework.

---

## 120. Device ID

T0 не требует Cloud enrollment.

Но Service создаёт persistent:

```text
device_id
```

при первом запуске.

UUID v4.

Хранить:

```text
C:\ProgramData\ClassOS\state\device-id
```

Позже заменим/свяжем с cryptographic device identity.

---

## 121. Service Instance ID

Каждый запуск Service:

```text
service_instance_id = random UUID
```

Не persistent.

Используется для:

* IPC;
* logs;
* stale-session detection.

---

## 122. Session Instance

Каждый Session Host launch:

```text
session_instance_id
```

генерируется Service.

Позволяет отличить:

```text
старый process
```

от:

```text
нового managed process.
```

---

## 123. Threading

Service должен иметь один Tokio runtime.

Не создавать отдельный OS thread под каждую мелочь.

Однако Windows SCM callback работает по правилам `windows-service` crate.

Передавать события в Tokio через thread-safe channel.

---

## 124. Cancellation

Использовать общий cancellation mechanism.

Например:

```text
tokio_util::sync::CancellationToken
```

или собственный shutdown broadcast.

При stop:

```text
cancel
↓
tasks stop
↓
Session Host shutdown
↓
flush logs
↓
SCM STOPPED
```

---

## 125. Graceful shutdown deadline

Service:

```text
~10 sec maximum target
```

SCM не должен бесконечно ждать.

---

## 126. Session Host runtime

Очень простой:

```text
initialize logging
↓
parse args
↓
connect Named Pipe
↓
handshake
↓
event loop
```

---

## 127. Session Host event loop

Обрабатывает:

```text
Ping
GetSessionInfo
Shutdown
```

Unknown message:

```text
protocol error/log
```

Не panic.

---

## 128. Parent death

Если Service исчез:

Named Pipe disconnect.

Session Host:

```text
wait short grace period
↓
exit
```

Не оставлять orphan Session Host навсегда.

---

## 129. Reconnect policy

Session Host T0:

не reconnect indefinitely к старому pipe.

Если Service connection lost:

```text
exit
```

Service после restart создаст новый Session Host.

Это значительно проще и надёжнее.

---

## 130. Session Host uniqueness

Один Service instance запускает:

```text
at most one Session Host
```

для selected console session.

---

## 131. Security invariant №1

Session Host никогда не получает:

```text
LocalSystem token
```

---

## 132. Security invariant №2

Service никогда не доверяет:

```text
sessionId
pid
username
```

только потому, что Session Host прислал их.

Проверяет Windows independently.

---

## 133. Security invariant №3

Session Host не получает privileged generic command:

```text
RunAsSystem(string)
```

Такого protocol message вообще не должно существовать.

---

## 134. Security invariant №4

Named Pipe ACL создаётся явно.

Default descriptor запрещён.

---

## 135. Security invariant №5

Все Win32 token/process handles должны иметь deterministic lifetime.

---

## 136. Security invariant №6

Command line Session Host не содержит:

```text
password
token
API key
private key
```

---

## 137. Security invariant №7

Session Host process создаётся только из trusted ClassOS installation directory:

```text
C:\Program Files\ClassOS\classos-session.exe
```

Не из writable temp/user directory.

---

## 138. File ACL

`Program Files\ClassOS` должен быть writable только privileged identities.

Standard student user:

```text
Read & Execute
```

не:

```text
Modify
```

---

## 139. ProgramData ACL

Стандартный пользователь не должен менять:

```text
Service config
device state
```

Session Host logs при необходимости пишутся:

через Service IPC или в user-writable log location.

Лучше T0:

Service owns privileged logs.

---

## 140. Rust coding rules

Запрещено:

```text
unwrap()
expect()
```

в production path за исключением compile-time/programmer invariants.

---

## 141. Naming

Rust modules:

```text
snake_case
```

Types:

```text
PascalCase
```

Errors:

```text
ThingFailed
ThingUnavailable
```

---

## 142. API design

Windows module:

```rust
pub trait SessionProvider {
    fn active_console_session(&self) -> Result<Option<Session>>;
}
```

---

## 143. Process launcher trait

```rust
pub trait SessionProcessLauncher {
    fn launch(
        &self,
        session: &Session,
        spec: &ProcessSpec,
    ) -> Result<ManagedProcess>;
}
```

---

## 144. IPC abstraction

```rust
pub trait LocalIpcServer {
    async fn accept(&self) -> Result<IpcConnection>;
}
```

Это позволит unit-test Supervisor без real Win32.

---

## 145. Mock implementations

Обязательно:

```text
MockSessionProvider
MockProcessLauncher
MockIpcServer
```

для state-machine tests.

---

## 146. Why dependency inversion matters

Иначе любой test:

```text
must run as LocalSystem on Windows
```

и разработка станет адом.

Business lifecycle должен тестироваться без настоящего Win32.

---

## 147. T0 module graph

```text
agent-service
     │
     ├── agent-core
     │
     ├── protocol
     │
     └── windows-platform

agent-session
     │
     ├── agent-core
     ├── protocol
     └── windows-platform

protocol
     │
     └── prost

windows-platform
     │
     └── windows
```

---

## 148. `windows-platform` purity

`windows-platform` не должен импортировать:

```text
Teacher
Lesson
Student
AI
Organization
```

Никакого product domain.

Только Windows primitives.

---

## 149. Build profiles

Development:

```text
debug symbols
```

Production:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
panic = "abort"
```

`panic = abort` обсуждается после того, как все resource cleanup paths безопасны.

Для раннего T0 можно оставить default unwind ради debugging.

---

## 150. CI

GitHub Actions или другой CI:

```text
windows-latest
```

Steps:

```text
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

---

## 151. Formatting

```text
rustfmt
```

обязателен.

---

## 152. Clippy

CI:

```text
-D warnings
```

Но не писать:

```text
#[allow(...)]
```

на весь crate ради обхода проблем.

---

## 153. Documentation

Public architecture traits должны иметь rustdoc.

Low-level obvious wrappers не требуют романов.

---

## 154. No premature async everywhere

Win32 synchronous call:

```text
WTSGetActiveConsoleSessionId
```

не нужно превращать в async.

Async использовать для:

* lifecycle;
* timers;
* IPC;
* cancellation.

---

## 155. Blocking operations

Потенциально долгие blocking операции не должны блокировать Tokio executor.

В T0 таких мало.

Позже:

* installers;
* policies;
* WinGet;

используют `spawn_blocking`/worker.

---

## 156. T0 implementation sequence

Codex должен работать строго в этом порядке.

---

## Step 1 — Workspace

Создать:

```text
Cargo workspace
5 crates
proto generation
CI
```

Success:

```text
cargo test --workspace
```

---

## Step 2 — Protocol

Реализовать:

```text
Envelope
Hello
Ping
Pong
SessionInfo
Shutdown
```

Плюс:

```text
length framing
```

Unit tests.

---

## Step 3 — Dev IPC

Сделать Named Pipe Service/Session Host в обычном user context.

Без Session launch.

Запуск вручную:

```text
service run
session --dev
```

Handshake + heartbeat.

---

## Step 4 — Service skeleton

Добавить настоящий:

```text
Windows Service
```

с:

```text
start
stop
session change
```

---

## Step 5 — Session discovery

Добавить:

```text
WTSGetActiveConsoleSessionId
```

плюс session metadata abstraction.

---

## Step 6 — User token

Добавить:

```text
WTSQueryUserToken
```

RAII wrapper.

---

## Step 7 — Process launch

Добавить:

```text
CreateEnvironmentBlock
CreateProcessAsUserW
```

Session Host запускается автоматически.

---

## Step 8 — Secure pipe

Добавить explicit ACL с SID interactive пользователя.

Не делать это последним перед production.

---

## Step 9 — Supervisor

Добавить:

```text
desired state
reconcile
restart
backoff
```

---

## Step 10 — Installer

Настоящий installed service + reboot tests.

---

## 157. Git commits

Желательно небольшие независимые commits:

```text
chore: initialize Rust workspace

feat(protocol): add local IPC messages

feat(protocol): add length-prefixed framing

feat(ipc): add local named-pipe transport

feat(service): add Windows service runtime

feat(windows): add active console session discovery

feat(windows): add session user token handling

feat(windows): launch process in user session

feat(ipc): restrict pipe ACL to session user

feat(service): add session supervisor

feat(service): add host heartbeat and recovery

chore(installer): add service install scripts

test: add T0 Windows integration tests
```

---

## 158. Codex must not optimize yet

Не делать в T0:

```text
zero-copy protocol

shared memory

IOCP optimization

custom allocator

lock-free queues

QUIC

GPU

SIMD
```

Reliability > cleverness.

---

## 159. Manual smoke checklist

Перед merge T0:

```text
[ ] clean build

[ ] install service

[ ] service starts

[ ] login creates session host

[ ] correct user

[ ] correct session ID

[ ] pipe handshake works

[ ] heartbeat works

[ ] lock/unlock works

[ ] logout removes host

[ ] login starts new host

[ ] killing host causes restart

[ ] restarting service works

[ ] reboot works

[ ] other user cannot open pipe

[ ] no runaway CPU

[ ] no obvious handle leak
```

---

## 160. Handle leak test

Во время 100 Session Host restart cycles:

```text
Service handle count
```

не должен постоянно расти.

Особенно проверить:

* user token;
* process handle;
* thread handle;
* Named Pipe handle;
* environment block.

---

## 161. Process leak test

После многократного:

```text
login/logout
```

не остаются orphan:

```text
classos-session.exe
```

---

## 162. IPC fuzz-ish tests

Отправить:

```text
zero length
oversized frame
invalid protobuf
unknown field
abrupt disconnect
partial message
```

Service не падает.

---

## 163. Service recovery scenario

Если Session Host binary отсутствует:

Service:

```text
logs error
backoff
continues running
```

Не crash-loop самого Service.

---

## 164. Broken user token

Если `WTSQueryUserToken` failed:

```text
log
backoff
reconcile later
```

Service остаётся alive.

---

## 165. Temporary no-console state

`WTSGetActiveConsoleSessionId()` может вернуть:

```text
0xFFFFFFFF
```

при переходном состоянии console session. Microsoft прямо документирует такой сценарий.

Это не fatal error.

State:

```text
NoInteractiveSession
```

и повторный reconcile.

---

## 166. No logged-in user

После boot до login:

Expected:

```text
Service Running
Session Host absent
```

Это нормальное состояние.

---

## 167. First user login

Expected:

```text
SessionLogon
↓
reconcile
↓
launch
```

Target:

Session Host ready:

```text
<10 sec after desktop available
```

---

## 168. Startup race

Windows session может существовать, но desktop ещё не полностью готов.

Если `CreateProcessAsUser` временно fails:

```text
retry with backoff
```

Не считать устройство broken.

---

## 169. Service permissions

Не вручную включать дополнительные privileges без реальной необходимости.

Начать с standard LocalSystem service token.

Если API требует privilege:

* проверить документ;
* включать явно только нужный privilege;
* документировать.

---

## 170. No token leakage

Никогда:

```text
serialize HANDLE
send user token through IPC
store token
log token
```

Token существует только внутри Service process и используется кратковременно для process launch.

---

## 171. Session Host trust model

Session Host позже будет взаимодействовать с данными пользователя и потенциально может быть manipulated этим пользователем.

Поэтому:

> Session Host никогда не является trusted authority.

Service проверяет все privileged requests.

---

## 172. Future IPC direction

Позже Service → Session Host:

```text
capture screen
inject input
show overlay
```

Session Host → Service:

```text
frame data
foreground app
idle info
```

Но privileged:

```text
install software
apply policy
restart computer
```

выполняются только Service.

---

## 173. T0 output artifacts

После завершения должны существовать:

```text
target/release/classos-service.exe

target/release/classos-session.exe

scripts/install-service.ps1

scripts/uninstall-service.ps1
```

---

## 174. T0 documentation output

Codex должен добавить:

```text
README-T0.md
```

с:

* prerequisites;
* build;
* development run;
* service install;
* uninstall;
* smoke tests;
* known limitations.

---

## 175. Known limitations T0

Зафиксировать:

```text
Windows 11 x64 only tested

single physical console session

no RDP support guarantee

no Teacher Console

no external network

no screen capture

no cloud auth

no auto-update
```

Это не bugs.

Это scope.

---

## 176. Gate before T1

Нельзя начинать:

## T1 Teacher ↔ Agent Network

пока T0 не проходит:

```text
reboot
login/logout
host crash
service restart
pipe ACL
8h stability
```

---

## 177. Почему это важно

Если сейчас сделать хрупкую основу:

```text
Service
↔
Session Host
```

то позже на ней окажутся:

```text
DXGI
remote control
policies
AI
software deployment
```

и любой lifecycle bug будет выглядеть как:

> «ClassOS иногда просто не видит компьютер».

А для такого продукта это смертельно.

---

## 178. Exact T0 outcome

После T0 архитектура выглядит:

```text
Windows
│
├── Session 0
│
│   ClassOSAgent
│   LocalSystem
│
│       │
│       │ Named Pipe
│       │ authenticated by Windows ACL
│       │
│       ▼
│
└── Session 1
    Student
        │
        └── ClassOSSession
            Standard User
```

Service знает:

```text
there is active session #1
```

Session Host знает:

```text
I belong to Service instance X
```

Оба:

```text
heartbeat healthy
```

---

## 179. T1 preview

Следующий milestone после T0:

## Teacher ↔ Agent Network

Добавятся:

```text
device identity
local discovery
TLS
Teacher authentication
network protocol
DeviceHello
heartbeat
```

Но Session Host architecture остаётся прежней.

---

## 180. T2 preview

После T1:

## DXGI Screenshot

```text
Teacher
↓
Service
↓
Session Host
↓
DXGI
↓
Session Host
↓
Service
↓
Teacher
```

---

## 181. T3 preview

После screenshot:

```text
continuous thumbnail streams
```

---

## 182. T4 preview

После stable streaming:

```text
remote control
```

через Session Host + `SendInput`.

---

## 183. Final T0 rule

Codex должен принимать решения исходя из:

```text
correctness
security
recoverability
testability
```

а не:

```text
minimum number of files
```

Но при этом не создавать unnecessary abstractions, которые пока ничем не оправданы.

---

## 184. Codex task

Ниже можно использовать как непосредственную стартовую задачу.

---

### Task: Implement ClassOS T0

Implement milestone T0 according to:

```text
docs/01_TECHNICAL_ARCHITECTURE.md
docs/T0_SERVICE_SESSION_HOST_SPEC.md
```

#### Goal

Build the foundational Windows architecture:

```text
ClassOS Windows Service
        ↕
secure Named Pipe IPC
        ↕
ClassOS Session Host
```

#### Required implementation

Create the Rust workspace and implement:

1. `agent-service`;
2. `agent-session`;
3. `agent-core`;
4. `protocol`;
5. `windows-platform`.

Implement:

* Windows Service runtime;
* development console runtime;
* physical console session discovery;
* Service Control session-change handling;
* `WTSQueryUserToken`;
* environment creation;
* `CreateProcessAsUserW`;
* managed Session Host process;
* strict Named Pipe ACL;
* protobuf IPC;
* length-prefixed framing;
* Hello handshake;
* Ping/Pong heartbeat;
* SessionInfo;
* graceful Shutdown;
* Session Host supervision;
* exponential restart backoff;
* automatic reconciliation;
* structured logging;
* install/uninstall scripts;
* unit tests;
* Windows integration tests where practical.

#### Security requirements

Do not:

* expose a network-accessible Named Pipe;
* use default pipe ACL;
* trust PID/session values received from Session Host;
* run Session Host as LocalSystem;
* expose arbitrary SYSTEM command execution;
* leak user tokens;
* store sensitive handles;
* put secrets into process arguments.

Use RAII wrappers for Win32 resources.

#### Scope restrictions

Do not implement:

* screen capture;
* remote input;
* Teacher Console;
* network discovery;
* cloud;
* policies;
* software deployment;
* AI.

#### Acceptance criteria

T0 is complete only when:

1. Service automatically starts after reboot.
2. No Session Host runs before interactive login.
3. Login launches Session Host in the correct user/session.
4. Service and Session Host complete IPC handshake.
5. Ping/Pong remains stable.
6. Killing Session Host causes automatic restart.
7. Logout removes the managed Session Host.
8. Login as another user launches a new correct Session Host.
9. Restarting the Service recovers automatically.
10. Unauthorized local users cannot connect to the Session Host pipe.
11. 8-hour idle stability test shows no crash loop or obvious resource leak.

#### Engineering approach

Work incrementally.

Do not mock completion.

After every substantial step:

* compile;
* run relevant tests;
* inspect errors;
* fix root causes.

Prefer small modules and explicit ownership.

Keep raw Win32 `unsafe` code inside `windows-platform`.

At the end, produce:

```text
README-T0.md
```

containing:

* architecture implemented;
* build instructions;
* service installation;
* local development instructions;
* smoke-test procedure;
* known limitations;
* remaining issues before T1.

### Do not start T1

## 185. Final Definition of Done

Мы готовы сказать:

## T0 COMPLETE

только если эта цепочка работает без ручного вмешательства:

```text
Windows boots
↓
ClassOS Service starts
↓
Student logs in
↓
Service discovers session
↓
Service obtains user token
↓
Service launches Session Host
↓
Session Host connects through secured Named Pipe
↓
handshake succeeds
↓
heartbeat remains healthy
↓
Session Host crashes
↓
Service automatically restores it
↓
user logs out
↓
Session Host disappears
↓
another user logs in
↓
correct new Session Host appears
```

Если это стабильно — фундамент ClassOS готов.

Только после этого начинаем:

## `docs/specs/T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md`
