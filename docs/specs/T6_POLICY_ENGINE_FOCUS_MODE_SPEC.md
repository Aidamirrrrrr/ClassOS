# ClassOS — T6 Implementation Specification

**Файл:** `docs/specs/T6_POLICY_ENGINE_FOCUS_MODE_SPEC.md`
**Статус:** Spec-ready
**Milestone:** T6
**Предпосылка:** T5 завершён (classroom commands, Veyon parity)

---

## 1. Цель T6

Это milestone, после которого ClassOS перестаёт быть «просто Veyon» (`02_MARKET_AND_INVESTMENT_ANALYSIS.md` §13). Реализовать реальную security boundary — Policy Engine — и Focus Mode как первый продуктовый layer поверх неё.

---

## 2. Definition of Done

```text
Teacher: [Focus Mode] → Allow: VS Code → [Enable]
↓
Student пытается запустить заблокированное приложение (например тестовое запрещённое приложение)
↓
не запускается — блокировка на уровне Windows enforcement, а не overlay
↓
[Disable Focus]
↓
recalculate EffectivePolicy → возврат к Base+Branch+Room, без Focus
```

Тестируется под **standard Windows user** (не admin) — так и должен работать student-аккаунт (`03_EXECUTION_PLAN_90_DAYS.md` §19).

---

## 3. Non-goals

```text
software installation/deployment (T7)
browser URL filtering сложнее базового enterprise-policy allowlist (сам allowlist — да, MITM/proxy-фильтрация — нет, см. §75-76 арх. RFC)
network-level filtering через WFP (явно отложено, будущее v1+)
Lesson Profiles как полноценная бизнес-сущность со scheduling (это уже Lesson Engine, отдельная будущая фаза — T6 даёт только применение статичного профиля по кнопке)
```

---

## 4. Архитектурное решение (ADR-0006, не переоткрывать)

Продуктовый слой `LessonPolicy`/`ApplicationDefinition` никогда не содержит registry keys напрямую. Policy Compiler транслирует в конкретные механизмы:

```text
ClassOS Policy (YAML-подобная модель)
      ↓
Policy Compiler
      ↓
├── Assigned Access
├── AppLocker
├── registry/GPO/CSP
└── Browser policies (Chrome/Edge enterprise policy)
```

---

## 5. `PolicyProvider` trait

```rust
trait PolicyProvider {
    fn check_support(&self) -> Capability;
    fn current_state(&self) -> Result<State>;
    fn apply(&self, policy: &Policy) -> Result<ApplyResult>;
    fn rollback(&self, snapshot: &PolicySnapshot) -> Result<()>;
}
```

Крейт `policy-engine`, изолирован от Windows-специфики так же, как `windows-platform` изолирует Win32 (тот же паттерн, что уже применён в T0 — не изобретать новый).

---

## 6. Safe rollout workflow (обязательная последовательность, не сокращать)

```text
Compile
↓
Validate (например Test-AppLockerPolicy или эквивалент)
↓
Check required ClassOS components (allow rules для собственных бинарников — см. §8)
↓
Snapshot current state
↓
Apply
↓
Verify
↓
Commit
```

При Failure на любом шаге после Snapshot — **Rollback snapshot**, не частично применённое состояние (`01_TECHNICAL_ARCHITECTURE.md` §67). Это инвариант IV из `CLAUDE.md`: «Policy обязана иметь rollback» — не опция, а обязательное условие для мержа T6.

---

## 7. Policy model (продуктовый уровень)

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

Первый набор ограничений (T6 минимум, `03_EXECUTION_PLAN_90_DAYS.md` §20):

```text
application restrictions
Settings restrictions
PowerShell restriction
cmd restriction
Microsoft Store restriction
personalization restriction
```

---

## 8. Never block ClassOS (обязательный auto-allow)

Policy Compiler автоматически добавляет allow-правила для собственных ClassOS-бинарников (`classos-service.exe`, `classos-session.exe`, будущий `classos-updater.exe`) при каждой компиляции политики, **до** её применения. Пропуск этого шага — критический баг: одна ошибочная политика не должна заблокировать сам management layer, лишив teacher/admin возможности исправить ситуацию удалённо (`01_TECHNICAL_ARCHITECTURE.md` §68).

---

## 9. Break-glass

Локальный администратор должен иметь emergency-механизм («ClassOS Recovery»), который отключает active Lesson Policy и восстанавливает базовое состояние, доступный **только** локально (не через сеть/Teacher Console) — на случай бага в Policy Compiler (`01_TECHNICAL_ARCHITECTURE.md` §69). Без этого механизма T6 не может считаться production-ready даже для пилота.

---

## 10. Policy layering

```text
BASE DEVICE POLICY
       +
BRANCH POLICY
       +
ROOM POLICY
       +
LESSON POLICY
       +
TEMPORARY TEACHER OVERRIDE (= Focus Mode)
```

`EffectivePolicy` рассчитывается детерминированно из этих слоёв (`01_TECHNICAL_ARCHITECTURE.md` §70–71). На T6 допустимо, что Branch/Room policy — статичная локальная заглушка (реальная multi-branch иерархия — Cloud v0, T8+), но **модель слоёв обязана быть реализована сразу в этом виде**, а не как плоский единственный policy-объект, который потом придётся переписывать.

---

## 11. Focus Mode = Temporary Policy Overlay

Focus Mode — не отдельный низкоуровневый механизм, а частный случай Temporary Override поверх Effective Policy (`01_TECHNICAL_ARCHITECTURE.md` §73):

```text
Focus:
  allow: [текущие lesson-приложения]
  block: everything else
  browser: restricted
```

Выключение:

```text
remove overlay
↓
recalculate EffectivePolicy
↓
Base + Branch + Room остаётся, Lesson (если был активен отдельно) тоже остаётся
```

---

## 12. Browser policies

Реализуется через официальные enterprise-policy механизмы Chrome/Edge (`01_TECHNICAL_ARCHITECTURE.md` §75):

```text
BrowserPolicy
  allowUrls
  blockUrls
  allowIncognito
  allowDownloads
  extensions
```

Явно **не** реализуется через MITM/proxy interception на T6 — это осознанно отложено (§76, WFP — будущее v1+, не нужен для PMF).

---

## 13. Новые сообщения протокола

```protobuf
message ApplyPolicy {
  string policy_id = 1;
  bytes compiled_policy = 2;   // сериализованный Policy, формат — на усмотрение реализации, но версионированный
}

message PolicyResult {
  string policy_id = 1;
  bool success = 2;
  string error_code = 3;
  string message = 4;
}

message RollbackPolicy {
  string snapshot_id = 1;
}

message FocusModeEnable {
  repeated string allowed_application_ids = 1;
}

message FocusModeDisable {}
```

`ApplyPolicy`/`RollbackPolicy` следуют тому же `Command`/`CommandResult` идемпотентному паттерну, что и T5 (переиспользовать, не изобретать параллельный механизм).

---

## 14. Teacher UI (Zero bullshit UX)

Teacher никогда не видит GPO/CSP/AppLocker/SID/registry (`CLAUDE.md`, продуктовый принцип; `03_EXECUTION_PLAN_90_DAYS.md` §7.4). Видит только:

```text
[Python]
[Roblox]
[Design]
[Focus Mode]
```

Любая реализация T6, которая протекает Windows-деталями в Teacher Console UI (даже в виде "advanced" вкладки по умолчанию видимой) — нарушение инварианта X.

---

## 15. Security invariants T6

1. Policy без rollback не мержится (см. §6).
2. Auto-allow для ClassOS-бинарников обязателен при каждой компиляции (§8).
3. Break-glass доступен только локально, не через сеть (§9).
4. Product API/Teacher UI не оперируют Windows-примитивами напрямую (§14, инвариант X `CLAUDE.md`).
5. Compiled policy применяется только после успешной Validate — никогда "apply blindly" (§6).

---

## 16. Тесты

### Unit

```text
Policy Compiler: YAML/модель → корректный набор правил для каждого enforcement provider
EffectivePolicy расчёт: layering нескольких уровней даёт предсказуемый детерминированный результат
auto-allow для ClassOS-бинарников присутствует в каждом скомпилированном policy без исключений
```

### Integration

```text
Apply → Verify → Commit — happy path под standard user
Apply с намеренно некорректным правилом → Validate ловит это до Apply
Apply, вызывающий сбой на этапе Verify → Rollback возвращает предыдущее рабочее состояние
Focus Mode enable/disable несколько раз подряд — корректный возврат к базовому состоянию каждый раз
Break-glass локально восстанавливает базовое состояние даже при "испорченной" Lesson Policy
попытка запустить заблокированное Focus-политикой приложение — блокируется на уровне Windows, а не только визуально
```

---

## 17. Acceptance criteria

1. На ученическом standard-user аккаунте невозможно запустить приложение, заблокированное активной политикой (`01_ROADMAP.md`, Phase 2 DoD).
2. Focus Mode включается/выключается по одной команде для всей группы устройств.
3. Rollback гарантированно восстанавливает рабочее состояние после сбоя Apply — проверено тестом, не только предположением.
4. Break-glass работает локально без сети и без зависимости от Teacher Console.
5. Teacher UI не показывает ни одной Windows-специфичной детали.

---

## 18. Что дальше

`T7_DEVICE_HEALTH_SOFTWARE_SPEC.md` — hardware/software inventory, health dashboard, WinGet-based install/repair. Это следующий продающий блок для владельца филиала (`02_MARKET_AND_INVESTMENT_ANALYSIS.md` §17 «Repair Classroom»).
