# ClassOS — T1 Implementation Specification

**Файл:** `docs/specs/T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md`
**Статус:** Spec-ready (не implementation-started)
**Milestone:** T1
**Предпосылка:** T0 завершён и стабилен (см. `T0_SERVICE_SESSION_HOST_SPEC.md` §176 gate)

---

## 1. Цель T1

T1 не показывает экран. T1 не делает remote control. T1 не трогает policies.

T1 должен доказать: **Teacher Console видит реальный Student PC по сети, устанавливает с ним аутентифицированное защищённое соединение и получает живой статус.**

После T1 ClassOS умеет:

1. объявлять устройство в локальной сети (unauthenticated discovery);
2. генерировать и хранить криптографическую identity устройства;
3. проходить enrollment (one-time код → device certificate);
4. устанавливать TLS-соединение Teacher ↔ Agent;
5. проходить protocol version negotiation;
6. обмениваться `DeviceHello`/`DeviceStatus`/`Heartbeat`;
7. показывать в Teacher Console список online/offline устройств;
8. переживать разрыв сети и переподключаться автоматически;
9. отличать «устройство существует в сети» от «устройству можно доверять».

---

## 2. Definition of Done

На стенде из 1 Teacher PC + 2–3 Student PC (уже прошедших T0):

```text
Запустить Teacher Console
↓
увидеть anounced-but-untrusted candidate devices
↓
пройти enrollment каждого устройства (one-time код)
↓
устройства переходят в CONNECTED
↓
Teacher видит hostname, статус online, agent version
↓
выключить Wi-Fi на одном Student PC
↓
Teacher Console показывает его OFFLINE в течение объявленного timeout
↓
включить обратно
↓
устройство автоматически возвращается в CONNECTED без переустановки
```

Дополнительно: другой ноутбук в той же сети, не проходивший enrollment, не может подключиться к control-каналу устройства (только видит discovery-объявление).

---

## 3. Non-goals (сознательно не входит в T1)

```text
screen capture / DXGI
remote input
classroom commands (lock/launch/restart/...)
policy engine
software management
cloud backend (реальный, не mock)
AlfaCRM
AI
QUIC (допустим TLS/TCP)
```

Если возникает соблазн «раз уж делаем сеть, давайте и скриншот отправим» — нет. Это T2.

---

## 4. Архитектурные решения, уже принятые (не переоткрывать без ADR)

- Discovery ≠ Trust — ADR-0005 логика распространяется и сюда: обнаружение не равно авторизации (`01_TECHNICAL_ARCHITECTURE.md` §29–31).
- Local-first — Teacher ↔ Agent работает без облака (ADR-0005).
- Protocol — schema-first, Protocol Buffers, versioned envelope (`01_TECHNICAL_ARCHITECTURE.md` §35–39).
- Транспорт T1 — TLS/TCP, UDP multicast discovery и отдельный крейт
  `transport` (ADR-0009).

Если T1 потребует отступить от одного из этих решений — сначала новый ADR, потом код.

---

## 5. Discovery

### 5.1 Механизм

Простой local multicast/broadcast. Agent периодически (например каждые 3–5 сек, с jitter) рассылает `ClassOSDeviceAnnouncement`:

```text
protocolVersion
deviceId          (публичный, не секрет)
hostname
roomHint          (опционально, если уже сконфигурирован)
agentVersion
ip
controlPort
```

Никаких credentials/tokens/имён учеников в announcement (`01_TECHNICAL_ARCHITECTURE.md` §29).

### 5.2 Trust model

Announcement — **UNTRUSTED** до криптографической аутентификации. Любой в LAN потенциально может подделать announcement — это допустимо и не является уязвимостью, пока авторизация не выдаёт прав на его основании (§31).

### 5.3 Teacher-side discovery lifecycle

```text
join multicast group
↓
собрать announcements за N секунд
↓
показать candidate list (untrusted)
↓
teacher инициирует enrollment ИЛИ подключается к уже enrolled device
↓
offline = lastSeen > timeout (предложение: 15 сек)
```

---

## 6. Device Identity & Enrollment

### 6.1 Ключевая пара устройства

При первом запуске (после T0 device_id уже существует как случайный UUID — см. `T0_*` §120) Agent генерирует криптографическую key pair для identity. Предпочтительно: несимвольный ключ через Windows CNG/machine key store, non-exportable, где возможно (`01_TECHNICAL_ARCHITECTURE.md` §45). TPM-backed key — будущее улучшение, не блокер T1.

Для T1 закрытый PKCS#8 защищается Windows DPAPI в machine scope; ограничения и
обязательное повторное рассмотрение CNG перед production зафиксированы в
ADR-0010.

Приватный ключ **никогда**:

- не сериализуется в IPC;
- не логируется;
- не хранится в открытом виде в `ProgramData` как plain file, если есть более безопасная альтернатива на конкретной машине.

### 6.2 Enrollment workflow (T1 — упрощённый, без реального cloud backend)

Т.к. реального Cloud ещё нет (Cloud v0 — часть T8), T1 реализует enrollment локально через сам Teacher Console как временный "admin authority". Это оформленное архитектурное решение, не спонтанное упрощение — см. `architecture/adr/0007-t1-local-enrollment-stub.md`.

```text
Admin (Teacher Console, admin role)
↓
генерирует enrollment code (short-lived, one-time)
↓
вводит/показывает код оператору на Student PC (или передаёт через тот же LAN discovery канал)
↓
Agent предъявляет code при первом подключении
↓
Teacher Console подтверждает enrollment
↓
Agent сохраняет: assigned deviceId, issuer public key, локально выпущенный "device certificate" (упрощённая структура на T1)
```

Важно зафиксировать явно: **это временная, локальная реализация enrollment для T1.** Когда появится Cloud v0 (T8), enrollment authority переезжает в облако, а протокол сообщения `EnrollmentRequest`/`EnrollmentResult` должен остаться совместимым (не менять схему дважды — продумать её сразу с учётом будущего cloud issuer, см. §7).

> **ADR-0018 дополняет этот поток.** Как описано выше, устройство отдаёт код тому, кто первым подключился, ничего о нём не зная, и принимает от него ключ издателя навсегда. ADR-0018 добавляет в `EnrollmentRequest`/`EnrollmentResult` nonce и доказательство знания кода, а также путь перевыпуска credential уже зарегистрированному устройству — без него истёкший credential и потерянный ключ издателя означают обход всех машин класса руками.

### 6.3 Формат enrollment code

```text
one-time
expires (например 10 минут)
attached to: organization/branch context (на T1 можно заглушку "default")
```

---

## 7. Protocol (Teacher ↔ Agent, network layer)

Отдельная proto-схема от локального IPC T0 (`local_ipc.proto`). Предложение: `proto/classos_network.proto`, `package classos.network.v1`.

### 7.1 Envelope

Аналогично T0 IPC, но с полями идентификации устройства/сессии teacher'а:

```protobuf
message Envelope {
  uint32 protocol_version = 1;
  string message_id = 2;
  int64 timestamp_ms = 3;

  oneof payload {
    DeviceHello device_hello = 10;
    TeacherHello teacher_hello = 11;
    EnrollmentRequest enrollment_request = 12;
    EnrollmentResult enrollment_result = 13;
    DeviceStatus device_status = 14;
    Heartbeat heartbeat = 15;
    UpgradeRequired upgrade_required = 16;
  }
}
```

### 7.2 Ключевые сообщения (минимум T1)

```text
DeviceHello        — deviceId, hostname, agentVersion, osVersion, capabilities[]
TeacherHello        — teacherSessionId, minProtocol, maxProtocol
EnrollmentRequest    — enrollmentCode, devicePublicKey
EnrollmentResult     — success/fail, issuedCredential (T1: упрощённый локальный "certificate")
DeviceStatus         — health placeholder (T1: online/offline + agentVersion; полноценный health — T7)
Heartbeat            — sequence, sentAtUnixMs (аналог T0 Ping/Pong, но по сети)
UpgradeRequired      — minProtocol/maxProtocol сервера/клиента при несовпадении
```

`capabilities[]` уже вводится в T1 (даже если пусто), чтобы feature detection был архитектурным паттерном с самого начала, а не добавлялся задним числом (`01_TECHNICAL_ARCHITECTURE.md` §37).

### 7.3 Version negotiation

Копирует паттерн T0 (§38 архитектурного RFC): Teacher объявляет `[minProtocol, maxProtocol]`, Agent — своё, выбирается наибольшее пересечение; при отсутствии пересечения — `UpgradeRequired`, соединение не устанавливается.

---

## 8. Transport

### 8.1 Выбор T1

TLS поверх TCP. QUIC — явно отложен (`01_TECHNICAL_ARCHITECTURE.md` §32: "MVP допускает TLS TCP, если это заметно ускоряет прототип"). Протокол должен быть спроектирован через trait `DeviceTransport`, не завязан на конкретный transport — переход на QUIC в будущем не должен требовать переписывать протокольный слой.

```rust
trait DeviceTransport {
    async fn connect(&self, addr: SocketAddr) -> Result<Connection>;
    async fn send(&self, msg: Envelope) -> Result<()>;
    async fn recv(&self) -> Result<Envelope>;
}
```

### 8.2 TLS

Самоподписанные/локально выпущенные сертификаты на T1 приемлемы (нет ещё настоящего Cloud CA). Обязательно: сертификат привязан к device identity (§6.1), а не голый TLS без проверки peer identity — иначе TLS даёт шифрование, но не аутентификацию, что противоречит инварианту "Discovery ≠ Trust".

### 8.3 Порт

`controlPort`, объявленный в discovery announcement — конкретное значение выбрать при реализации и задокументировать в `docs/specs/README-T1.md` (не хардкодить магическое число без объяснения, не конфликтовать с уже занятыми Windows-портами).

---

## 9. Connection & Device state machine (Teacher Console)

```rust
enum ConnectionState {
    Discovered,
    Connecting,
    Authenticating,
    Connected,
    Degraded,
    Disconnected,
    Unauthorized,
    UpgradeRequired,
}
```

Совпадает с `01_TECHNICAL_ARCHITECTURE.md` §95 — не изобретать другой набор состояний.

### 9.1 Offline detection

`lastSeen + heartbeat timeout`. Предложение: heartbeat каждые 5 сек (как в T0 IPC), offline после 15–20 сек отсутствия ответа — держать константы конфигурируемыми, не «магическими» внутри кода.

### 9.2 Reconnect

Автоматический, с exponential backoff (тот же паттерн, что supervisor в T0: 1s → 2s → 5s → 10s → 30s → 60s max). Reconnect не должен требовать повторного enrollment, если credential ещё валиден.

---

## 10. Teacher Console (T1 scope only)

Минимальный экран:

```text
Room (пока один, без реальной модели Organization/Branch/Room — та появится в T8 Cloud v0)

PC-01   CONNECTED    v0.1.3
PC-02   CONNECTED    v0.1.3
PC-03   OFFLINE
```

Действие:

```text
[Enroll new device]
```

Никаких screen thumbnails, никаких commands — тех кнопок в T1 быть не должно (это T2+).

---

## 11. Security invariants T1

1. Enrollment code — one-time, ограниченный TTL, не подходит для повторного использования.
2. Приватный ключ устройства никогда не покидает устройство.
3. TLS-сертификат привязан к device identity — Teacher Console должен отвергать соединение с устройством, чей сертификат не совпадает с ранее enrolled identity (защита от подмены устройства после enrollment).
4. Discovery-канал остаётся unauthenticated и не должен становиться каналом передачи чего-либо чувствительного.
5. Protocol version mismatch — явный отказ (`UpgradeRequired`), никогда silent fallback на «предположим, что старое поведение сработает».
6. Никаких arbitrary команд в T1 — сообщений, дающих исполнение кода/команд, в этом protocol-наборе не существует вовсе (это осознанно оставлено для T5 с явным RBAC).
7. **Дополнено ADR-0018.** Поток §6.2 в исходном виде («Agent предъявляет code при первом подключении») не проверяет, кому именно устройство предъявляет код: собеседник ничем себя не подтверждает, поэтому первый подключившийся становится издателем устройства. ADR-0018 вводит взаимное доказательство знания кода и перевыпуск credential; до его реализации инвариант 1 защищает только от повторного использования кода, но не от перехвата окна enrollment.

---

## 12. Failure handling

```text
Discovery: не видит устройство → показать "no devices found", не падать
Enrollment: неверный/просроченный код → явная ошибка EnrollmentResult.success=false + причина
TLS handshake failed → Unauthorized/Degraded, retry с backoff
Protocol mismatch → UpgradeRequired, соединение не поднимается
Heartbeat timeout → Disconnected → автоматический reconnect
```

Ошибки — machine-readable коды (паттерн §127 архитектурного RFC), не строки для сравнения в коде.

---

## 13. Тесты

### Unit

```text
envelope encode/decode
version negotiation (match / no overlap / one-sided)
enrollment code expiry logic
connection state machine transitions
backoff calculation (переиспользовать/повторно протестировать логику из T0, если крейт общий)
```

### Integration (минимум 2 реальных Windows-машины + 1 Teacher Console)

```text
discovery: teacher видит announcement
enrollment: happy path
enrollment: expired code → отказ
enrollment: reused code → отказ
TLS: valid identity → connected
TLS: cert mismatch после re-enroll другого устройства с тем же IP → rejected
network drop → offline detection → reconnect
teacher restart → device state восстанавливается корректно (re-discover + re-connect, без повторного enrollment)
```

---

## 14. Acceptance criteria

T1 считается завершённым, если:

1. Discovery работает в реальной локальной сети (не только на loopback).
2. Enrollment проходит end-to-end с одноразовым кодом.
3. TLS-соединение устанавливается и привязано к device identity, а не только к IP/hostname.
4. Teacher Console показывает верный online/offline статус в реальном времени.
5. Отключение сети одного устройства не влияет на остальные (изоляция соединений).
6. Reconnect после сетевого сбоя происходит автоматически без вмешательства оператора.
7. Неаутентифицированный клиент не может пройти дальше discovery-объявления.
8. Version mismatch корректно блокирует соединение вместо undefined behaviour.

---

## 15. Что дальше

После стабильного T1 — `T2_SCREEN_CAPTURE_DXGI_SPEC.md`: DXGI screenshot поверх уже работающего сетевого канала. Screen-протокол — новые сообщения в том же `classos_network.proto` (расширение envelope, не новый транспорт).
