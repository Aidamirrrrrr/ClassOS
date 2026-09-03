# ClassOS — T4 Implementation Specification

**Файл:** `docs/specs/T4_REMOTE_CONTROL_SPEC.md`
**Статус:** Spec-ready
**Milestone:** T4
**Предпосылка:** T3 завершён (selected-mode live stream стабилен)

---

## 1. Цель T4

Teacher может взять полный контроль над мышью и клавиатурой выбранного Student PC поверх уже работающего selected-mode стрима (T3).

После T4 закрывается P0 backlog из `03_EXECUTION_PLAN_90_DAYS.md` §73 (Veyon feature parity foundation) — это последний технический кирпич перед CHECKPOINT #1 demo.

---

## 2. Definition of Done

```text
Teacher открывает Student PC (fullscreen, T3)
↓
[Take Control]
↓
двигает мышь на своей машине → курсор двигается на Student PC
↓
кликает / печатает → действие происходит на Student PC
↓
на Student PC виден явный индикатор "Teacher connected"
↓
[Stop Control] или закрытие соединения
↓
индикатор исчезает, input больше не передаётся
```

Проверить дополнительно: попытка Teacher управлять elevated/admin окном на Student PC — управление не должно проходить (ограничение UIPI, см. §6).

---

## 3. Non-goals

```text
clipboard sync (явно future, см. §26 арх. RFC)
file transfer
множественный одновременный remote control одного устройства несколькими teacher (см. §7 — exclusive control)
запись сессии remote control
```

---

## 4. Реализация — где выполняется input injection

`SendInput` вызывается **в Session Host**, не в Service (Session Host работает в security-контексте пользователя — тот же процесс, что уже делает DXGI capture с T2). Именно поэтому UIPI-ограничение (§6) работает нам на пользу автоматически, без дополнительного кода.

```text
Teacher
↓ RemoteInput (T1 network channel)
Service
↓ Named Pipe (T0 IPC, новое сообщение)
Session Host
↓ SendInput (Win32)
Windows
```

---

## 5. Новые сообщения протокола

### 5.1 Network (`classos_network.proto`)

```protobuf
message RemoteControlStart {
  string device_id = 1;
}

message RemoteControlStarted {
  string device_id = 1;
  string session_id = 2;   // remote control session id, для audit
}

message RemoteControlStop {
  string device_id = 1;
}

message RemoteControlStopped {
  string device_id = 1;
  string reason = 2;       // "teacher_stopped" | "disconnected" | "denied"
}

message RemoteInputEvent {
  string device_id = 1;

  oneof event {
    MouseMove mouse_move = 10;
    MouseButton mouse_button = 11;
    MouseWheel mouse_wheel = 12;
    KeyEvent key_event = 13;
  }
}

message MouseMove {
  float x = 1;   // normalized 0.0..1.0, НЕ абсолютные пиксели (§6.2)
  float y = 2;
}

message MouseButton {
  enum Button { LEFT = 0; RIGHT = 1; MIDDLE = 2; }
  Button button = 1;
  bool is_down = 2;
  float x = 3;
  float y = 4;
}

message MouseWheel {
  int32 delta = 1;
}

message KeyEvent {
  uint32 virtual_key_code = 1;
  bool is_down = 2;
}
```

### 5.2 Local IPC (`local_ipc.proto`, расширение)

Аналогичный набор сообщений (`RemoteInputEvent` и т.п.) передаётся Service → Session Host по уже существующему Named Pipe.

### 5.2.1 Координаты

Mouse-координаты передаются **normalized (0.0 → 1.0)**, не абсолютными пикселями — так изменение масштаба/разрешения стрима на Teacher-стороне не ломает input mapping (`01_TECHNICAL_ARCHITECTURE.md` §26). Session Host переводит normalized-координаты в абсолютные пиксели целевого дисплея непосредственно перед `SendInput`.

---

## 6. `SendInput` и UIPI

`SendInput` умеет синтезировать mouse/keyboard input, но UIPI (User Interface Privilege Isolation) не позволяет процессу инжектить input в окна с более высоким integrity level, чем у самого процесса (`01_TECHNICAL_ARCHITECTURE.md` §25). Т.к. Session Host работает как standard user (ADR-0002), это автоматически означает:

> Teacher через ClassOS **не может** управлять elevated/admin-приложениями на Student PC.

Это желаемое security-свойство, а не баг, который нужно обходить. Не пытаться повышать integrity level Session Host ради «полноты» remote control.

---

## 7. Remote session state machine

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
teacherDeviceId  (или teacher session identity из T1 auth)
startedAt
sessionId
```

**Инвариант:** только один active remote-control owner на устройство одновременно. Другие teacher могут смотреть selected-mode stream (T3), но control — эксклюзивен (`01_TECHNICAL_ARCHITECTURE.md` §27). Второй `RemoteControlStart` от другого teacher, пока сессия Active — явный отказ, не queue и не silent override.

---

## 8. Student indication — обязательное, не опциональное

Во время remote control Session Host обязан показывать индикатор:

```text
┌───────────────────────────┐
│ Teacher connected         │
└───────────────────────────┘
```

Это прямое продолжение UI-долга, отмеченного в T3 §9. Скрытый remote control запрещён продуктовым правилом (`01_TECHNICAL_ARCHITECTURE.md` §28, §120) — это не UX-опция, а инвариант, зафиксированный в `CLAUDE.md` (пункт 6/7 инвариантов).

---

## 9. Disconnect handling

### 9.1 Teacher disconnect

```text
Teacher connection lost
↓
stop remote input
↓
clear remote owner
↓
student indicator removed
```

Не должно оставаться «ghost teacher session» (`01_TECHNICAL_ARCHITECTURE.md` §89) — таймаут на явное подтверждение отсутствия соединения должен быть коротким (секунды, не минуты).

### 9.2 Session Host crash во время remote control

Наследуется поведение T0 supervisor (§90 арх. RFC): screen streaming и remote control временно прекращаются, Session Host перезапускается через backoff, Teacher Console переводит устройство в `Degraded`/`Disconnected` до восстановления IPC.

---

## 10. Audit (минимальный для T4)

Каждое начало/конец remote control сессии обязано попадать в audit-запись (полноценный AuditLog — цель более поздних этапов, но T4 не должен создавать remote control functionality без хотя бы structured log-события):

```text
timestamp
teacherId
deviceId
action: REMOTE_CONTROL_STARTED | REMOTE_CONTROL_STOPPED
sessionId
result: SUCCESS | DENIED | ERROR
```

Полноценный persistent append-only AuditLog с локальной буферизацией при офлайне — это уже описано на уровне архитектуры (`01_TECHNICAL_ARCHITECTURE.md` §49–50) и не обязательно к полной реализации в T4, но структура события должна быть заложена сразу в этом формате, чтобы не переписывать её при подключении к реальному Audit-хранилищу позже.

---

## 11. Security invariants T4

1. Remote control разрешён только после `authorized teacher + explicit session` (§27 арх. RFC) — никакого implicit control через сам факт стрима.
2. Координаты/ввод не доверяются напрямую — Session Host обязан проверять, что control-сессия действительно Active перед каждым применением `SendInput` (защита от race: Stop пришёл, но одно "хвостовое" событие ещё в очереди).
3. UIPI-ограничение не обходится намеренно ни в каком виде (например попыткой поднять integrity level Session Host) — см. §6.
4. Один exclusive owner на устройство — второй teacher не может тихо перехватить контроль.

---

## 12. Тесты

### Unit

```text
RemoteControlStart/Stop/Started/Stopped encode-decode
RemoteInputEvent encode-decode (mouse/keyboard варианты)
normalized-координаты → пиксели конкретного дисплея (граничные случаи: 0.0, 1.0, разные разрешения)
state machine: Idle→Requesting→Active→Stopping→Idle, включая отказ второму teacher при Active
```

### Integration

```text
полный цикл: Start → mouse move → click → keyboard input → Stop, проверка результата на экране
попытка управлять elevated-окном (например запущенным от администратора приложением) — input не проходит
второй teacher пытается Start во время чужой Active сессии — явный отказ
disconnect teacher посреди сессии → индикатор исчезает, input прекращается
kill Session Host во время Active remote control → корректное восстановление после supervisor restart, без "залипшего" состояния Active на стороне Teacher Console
```

---

## 13. Acceptance criteria

1. Полный remote control цикл работает надёжно на LAN.
2. UIPI-ограничение подтверждено тестом (elevated-окно не управляется).
3. Эксклюзивность control подтверждена тестом.
4. Индикатор "Teacher connected" появляется/исчезает синхронно с реальным состоянием control-сессии, без ложных срабатываний и без пропусков.
5. Disconnect/crash сценарии не оставляют устройство в неконсистентном "callback-состоянии".
6. Каждый Start/Stop оставляет audit-совместимую log-запись.

---

## 14. Что дальше

С T4 закрыт технический фундамент CHECKPOINT #1 (`03_EXECUTION_PLAN_90_DAYS.md` §15) — самое время для первого демо реальному преподавателю **до** написания T5. Далее — `T5_CLASSROOM_COMMANDS_SPEC.md`: lock/message/launch app/open URL/restart/shutdown + bulk actions.
