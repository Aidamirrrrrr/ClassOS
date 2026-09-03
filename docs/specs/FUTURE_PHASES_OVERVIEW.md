# ClassOS — Future Phases Overview

**Файл:** `docs/specs/FUTURE_PHASES_OVERVIEW.md`
**Статус:** ROADMAP-LEVEL — **не implementation-ready**. Это карта того, что нужно превратить в отдельные `NN_..._SPEC.md` после Go-решения по T0–T8, а не спецификация для кодирования.

---

## Зачем этот файл

`product/01_ROADMAP.md` уже описывает Phase 4–10 (Lesson Engine → AlfaCRM → AI Supervisor → AI Tutor → Analytics → Retention → Enterprise) на продуктовом уровне. Этот файл — мост между тем roadmap'ом и будущими implementation-ready спеками того же качества, что T0–T8. **Не начинать кодировать что-либо отсюда напрямую** — сначала пишется отдельный `NN_..._SPEC.md` по образцу T0–T8 (DoD, non-goals, протокол, security invariants, тесты, acceptance criteria).

---

## Gate перед стартом этого блока

Согласно `03_EXECUTION_PLAN_90_DAYS.md` §46 и §66–68: не начинать Lesson Engine/AlfaCRM/AI, пока не пройден go/no-go по гипотезе «ClassOS заменяет Veyon» (T0–T8). Триггер для AlfaCRM конкретно — «минимум 2 школы регулярно проводят занятия через ClassOS» (§46). Триггер для AI — «1000+ real classroom hours» (§49).

---

## Phase A — Lesson Engine (следующий implementation-ready spec: `15_LESSON_ENGINE_SPEC.md`)

Источник: `01_ROADMAP.md` §24–26, `01_TECHNICAL_ARCHITECTURE.md` §124–126.

Ключевая абстракция — `LessonSession`:

```text
lessonId, branchId, roomId, teacherId, courseId, groupId
startTime, endTime
students[], devices[]
softwareProfile, policyProfile, webProfile
status: Scheduled → Preparing → Ready → Running → Finishing → Completed
```

Что должен решить будущий spec:

- Как `Start Lesson`/`Finish Lesson` workflow (§25–26 roadmap) переиспользует уже существующие Command-примитивы из T5/T6 (Apply Policy, Launch Application) вместо изобретения нового механизма.
- Domain separation уже зафиксирован архитектурно (`01_TECHNICAL_ARCHITECTURE.md` §126): Device domain / Education domain / Classroom domain — Core Device Agent **не должен знать** про AlfaCRM или Lesson напрямую, только про `Apply Lesson Profile`.
- Student identity: `Device + Student + Lesson` (§23 roadmap) — выбор ученика + PIN, список приходит из внешнего источника (пока вручную, затем AlfaCRM).

---

## Phase B — AlfaCRM Integration (следующий spec: `16_ALFACRM_INTEGRATION_SPEC.md`)

Источник: `01_ROADMAP.md` §21–22, §47–48; `01_TECHNICAL_ARCHITECTURE.md` §125.

Ключевые архитектурные решения, уже зафиксированные и обязательные к соблюдению в будущем spec:

- **Один централизованный AlfaCRM Connector** — не разрешать каждому worker обращаться к AlfaCRM самостоятельно. Единый rate limiter (AlfaCRM API ограничен 5 RPS), кеш, sync, retries (§22 roadmap).
- **Webhook > polling** там, где webhook доступен (AlfaCRM поддерживает triggers по событиям клиентов/групп/уроков/посещаемости/платежей).
- **AlfaCRM boundary** (`01_TECHNICAL_ARCHITECTURE.md` §125): `AlfaCRM Adapter → ClassOS canonical domain → Lesson Engine → Device orchestration`. Никаких AlfaCRM entity ID внутри Windows Agent protocol — это прямое продолжение domain separation из Phase A.
- Направление синхронизации: inbound (students/teachers/groups/lessons/schedule/attendance), outbound (attendance, lesson result, derived comments) — не пытаться сделать ClassOS источником истины для CRM-сущностей (`01_ROADMAP.md` §21).

---

## Phase C — AI Supervisor (следующий spec: `17_AI_SUPERVISOR_SPEC.md`)

Источник: `01_ROADMAP.md` §27–30, §49–54 execution plan; `01_TECHNICAL_ARCHITECTURE.md` §122–123.

Обязательная последовательность, зафиксированная во всех трёх продуктовых документах одинаково — **не пропускать шаги ради того, чтобы сразу сделать AI красиво**:

```text
AI Phase 1 — Rule-based stuck detection (без LLM вообще)
  active lesson + IDE open + project unchanged N min + not idle + same app → "Possibly stuck"

AI Phase 2 — обогащение rule engine
  + compiler errors, terminal output, repeated commands

AI Phase 3 — LLM/VLM только по событию (не continuous video analysis)
  telemetry trigger → single screenshot → analysis → discard raw frame
```

Архитектурное требование (`01_TECHNICAL_ARCHITECTURE.md` §123): Agent эмитит структурированные события (`ApplicationStarted`, `ForegroundChanged`, `IdleChanged`, `CompilationFailed`, `ProjectChanged`, `TeacherIntervention`) через Event Bus → Rules → AI Supervisor. **Core transport не должен быть напрямую связан с AI** — это отдельный consumer событий, не встроенная часть протокола T0–T8.

Privacy-инвариант (наследуется из T2 §9, `01_TECHNICAL_ARCHITECTURE.md` §122): raw screenshot для AI — capture → analysis → **discard**, персистентное хранение — opt-in enterprise feature, не default.

---

## Phase D — AI Tutor (следующий spec: `18_AI_TUTOR_SPEC.md`)

Источник: `01_ROADMAP.md` §31.

Ключевое продуктовое ограничение, обязательное к сохранению в spec: политика подсказок — не выдавать решение сразу (`подсказка → вопрос → ещё подсказка → пример → решение только если разрешено`), с teacher-контролируемыми режимами `AI OFF / Hints only / Normal / Free`.

---

## Phase E — Learning Analytics & Parent Reports (следующий spec: `19_ANALYTICS_AND_REPORTS_SPEC.md`)

Источник: `01_ROADMAP.md` §33, §47–48.

Включает Parent Report (перевод телеметрии в понятный родителю язык) и Curriculum/Teacher Intelligence (`02_MARKET_AND_INVESTMENT_ANALYSIS.md` §47–48) — но это уже расширения поверх накопленных данных, а не новая инфраструктура.

---

## Phase F — Retention Engine (следующий spec: `20_RETENTION_ENGINE_SPEC.md`)

Источник: `01_ROADMAP.md` §34, `02_MARKET_AND_INVESTMENT_ANALYSIS.md` §46.

Явное продуктовое требование: **не начинать с ML-модели**. Сначала `rules + statistics`, ML — только когда накопятся реальные данные (`01_ROADMAP.md` §34, повторено в §9 «Retention Intelligence» market analysis).

---

## Phase G — Enterprise / Multi-branch (следующий spec: `21_ENTERPRISE_MULTI_BRANCH_SPEC.md`)

Источник: `01_ROADMAP.md` §38, §22.

Ключевое: **policy inheritance** (HQ → Region → Branch → Room → Lesson) с запретом на понижение ограничений вниз по иерархии — «HQ запрещает Steam, филиал не может это отменить, но может дополнительно запретить Discord» (§38). T8 Cloud v0 даёт только плоскую Organization→Branch→Room модель — эта фаза добавляет полноценную иерархию, RBAC на уровне организации, SSO, bulk enrollment, self-hosted опцию.

---

## Как использовать этот файл

Когда наступает время конкретной фазы: взять соответствующий раздел здесь + первоисточники в `product/01_ROADMAP.md`, написать полноценный `NN_..._SPEC.md` по формату T0–T8 (см. `.claude/skills/classos-write-spec/`), обновить `specs/BACKLOG.md`, и только после этого — код.
