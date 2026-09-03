# ClassOS — T5 Implementation Specification

**Файл:** `docs/specs/T5_CLASSROOM_COMMANDS_SPEC.md`
**Статус:** Spec-ready
**Milestone:** T5
**Предпосылка:** T4 завершён, CHECKPOINT #1 demo проведено (`03_EXECUTION_PLAN_90_DAYS.md` §15)

---

## 1. Цель T5

Достичь Veyon feature parity по discrete-командам и закрыть P0 backlog (`03_EXECUTION_PLAN_90_DAYS.md` §73):

```text
Lock / Unlock
Message
Launch Application (из catalog, не arbitrary path)
Open URL
Restart
Shutdown
```

Плюс — **bulk actions** поверх нескольких устройств одновременно (`01_TECHNICAL_ARCHITECTURE.md` §96).

---

## 2. Definition of Done

```text
Teacher выбирает несколько устройств (checkbox / Select All)
↓
[Lock] / [Open App] / ...
↓
команды выполняются параллельно (не последовательно)
↓
UI показывает "18/20 successful, PC-07 offline, PC-12 policy failed" — а не единый success/fail
```

Самые частые действия выполняются за 1–2 клика (`03_EXECUTION_PLAN_90_DAYS.md` §16).

---

## 3. Non-goals

```text
Policy Engine / Focus Mode (T6)
произвольный исполняемый путь/PowerShell для teacher (запрещено продуктовым правилом, см. §5)
software installation (T7)
```

---

## 4. Command envelope (расширение `classos_network.proto`)

Общий паттерн команды/ответа, единый для всех командных типов (`01_TECHNICAL_ARCHITECTURE.md` §39–42):

```protobuf
message Command {
  string command_id = 1;      // UUID, для idempotency
  int64 expires_at_unix_ms = 2;

  oneof body {
    LockDevice lock_device = 10;
    UnlockDevice unlock_device = 11;
    ShowMessage show_message = 12;
    LaunchApplication launch_application = 13;
    OpenUrl open_url = 14;
    RestartDevice restart_device = 15;
    ShutdownDevice shutdown_device = 16;
  }
}

message CommandResult {
  string command_id = 1;
  bool success = 2;
  string error_code = 3;   // machine-readable, пусто если success
  string message = 4;
}
```

### 4.1 Idempotency

`command_id` — UUID, сгенерированный Teacher Console. Agent хранит короткий кэш выполненных `command_id` (`01_TECHNICAL_ARCHITECTURE.md` §41) — повтор после reconnect не выполняется заново. Это особенно важно для `RestartDevice`/`ShutdownDevice`.

### 4.2 Deadlines

Каждая команда имеет `expires_at_unix_ms`. Команда, отправленная 10 минут назад и доставленная только после reconnect, — устарела и должна быть отклонена Agent'ом с кодом `COMMAND_EXPIRED`, а не выполнена вслепую (§42 арх. RFC).

---

## 5. `LaunchApplication` — через catalog, не произвольный путь

Teacher никогда не отправляет `C:\whatever\program.exe` (`01_TECHNICAL_ARCHITECTURE.md` §60–61). Вместо этого:

```protobuf
message LaunchApplication {
  string application_id = 1;   // например "vscode", разрешается через Application Catalog
}
```

Agent разрешает `application_id → путь` через локально закешированный Application Catalog (полноценный cloud-catalog — часть T7/T8, на T5 допустим статический локальный список известных ClassOS-приложений: VS Code, Chrome, Python и т.п.). Arbitrary executable/PowerShell — **не существует как команда вообще**, и только Admin role (не Teacher) в принципе может когда-либо получить более широкие возможности, причём это явно не входит в T5 (§61 арх. RFC: "лучше вообще не включать в Teacher Console").

---

## 6. `LockDevice` — T5 реализация против будущей T6

На T5 Lock реализуется как Session Host full-screen topmost overlay (`01_TECHNICAL_ARCHITECTURE.md` §74):

```text
┌───────────────────────────┐
│ Экран временно             │
│ заблокирован преподавателем │
└───────────────────────────┘
```

**Явно зафиксировать в README-T5.md:** этот overlay — **не security boundary**. Настоящие app restrictions — это Policy Engine (T6). Lock в T5 не мешает технически подкованному ученику завершить процесс overlay — и это допустимо для T5, потому что реальная защита придёт с T6, а T5 закрывает Veyon-parity UX, а не security guarantee.

---

## 7. Bulk dispatch

```text
12 devices
↓
параллельный fanout команды каждому устройству (не последовательно, не await в цикле)
↓
собрать все CommandResult
↓
UI: "11 Success / 1 Failed (PC-07 offline)"
```

Партиальный отказ обязателен к показу явно (наследуется паттерн из T3 §8, применяется здесь к командам как исходно и задумано в `01_TECHNICAL_ARCHITECTURE.md` §96–97).

---

## 8. Teacher UI (минимум T5)

```text
Actions:
Lock
Unlock
Message
Open app
Open URL
Restart
Shutdown

Select:
☑ PC-01  ☑ PC-02  ☑ PC-03
[Select All]
```

---

## 9. Security invariants T5

1. `LaunchApplication` работает только через Application Catalog — никакого произвольного пути (§5).
2. Нет команды, эквивалентной "run arbitrary command as SYSTEM" — такое сообщение не должно существовать в схеме вообще (это уже зафиксировано в `CLAUDE.md`, инвариант 8, и в T0 §133 — «Session Host не получает privileged generic command»; здесь распространяется на весь Teacher-facing протокол).
3. `RestartDevice`/`ShutdownDevice` обязаны быть идемпотентны через `command_id` (§4.1) — двойной restart из-за реордеринга сети не должен быть возможен как отдельный неучтённый эффект.
4. Все команды и их результаты обязаны быть audit-совместимы (тот же формат события, что заложен в T4 §10).

---

## 10. Тесты

### Unit

```text
Command/CommandResult encode-decode для каждого типа
idempotency cache: повтор command_id не выполняется дважды
expired command отклоняется (COMMAND_EXPIRED)
application_id, отсутствующий в catalog → явная ошибка, не silent no-op
```

### Integration

```text
bulk Lock на 10+ устройств — параллельно, не последовательно (замерить время: не должно расти линейно с числом устройств)
одно из 10 устройств offline — итоговый отчёт показывает частичный успех
Open URL / Launch VS Code — приложение реально открывается на Student PC
Restart/Shutdown — устройство физически перезагружается/выключается (проверка на выделенном тестовом стенде, не на проде)
повторная отправка того же command_id после reconnect — не выполняется повторно
```

---

## 11. Acceptance criteria

1. Все команды из списка §1 работают end-to-end на реальных устройствах.
2. Bulk actions выполняются параллельно с корректным partial-failure отчётом.
3. Launch/Open URL проходят исключительно через catalog/allowlist — нет пути для произвольного исполняемого кода.
4. Idempotency и deadlines подтверждены тестами, не только описаны в спеке.
5. Самые частые действия (Lock, Message, Open App) доступны за 1–2 клика в UI.

---

## 12. Что дальше

`T6_POLICY_ENGINE_FOCUS_MODE_SPEC.md` — реальная security boundary (AppLocker/Assigned Access) вместо overlay, плюс Focus Mode и Lesson Profiles. Именно с T6 продукт перестаёт быть «просто Veyon» (`02_MARKET_AND_INVESTMENT_ANALYSIS.md` §13, §68 pivot-предупреждение).
