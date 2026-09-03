# ClassOS Docs — карта документации

Этот файл — единственная точка правды о том, **где что лежит** и **в каком порядке это читать**. Если структура каталогов меняется — обновляется в первую очередь этот файл.

> Внутри старых документов (`product/01_ROADMAP.md` и др.) встречаются собственные схемы репозитория/каталогов — это исторические иллюстрации на момент написания, а не текущее состояние. Актуальная структура — здесь и в `architecture/01_TECHNICAL_ARCHITECTURE.md`.

---

## 1. Структура

```text
docs/
├── README.md                              ← вы здесь
├── ISSUES_AND_INCONSISTENCIES.md          ← найденные нестыковки между документами
│
├── product/                               ← ЗАЧЕМ мы это строим
│   ├── 01_ROADMAP.md                      Product & Technical Roadmap v0.1
│   ├── 02_MARKET_AND_INVESTMENT_ANALYSIS.md  Рынок, конкуренты, инвестиции
│   └── 03_EXECUTION_PLAN_90_DAYS.md       План первых 90 дней, go/no-go
│
├── architecture/                          ← КАК устроена система в целом
│   ├── 01_TECHNICAL_ARCHITECTURE.md       Technical Architecture RFC
│   └── adr/                               Architecture Decision Records
│       ├── README.md                      индекс + шаблон ADR
│       ├── 0001-rust-agent.md
│       ├── 0002-service-session-separation.md
│       ├── 0003-dxgi-screen-capture.md
│       ├── 0004-named-pipe-ipc.md
│       ├── 0005-local-first-control.md
│       ├── 0006-policy-engine-abstraction.md
│       ├── 0007-t1-local-enrollment-stub.md
│       └── 0008-monorepo-tooling.md
│
└── specs/                                 ← ЧТО именно реализовать, milestone за milestone
    ├── BACKLOG.md                         статус всех milestone'ов T0–T8+
    ├── T0_SERVICE_SESSION_HOST_SPEC.md         [SPEC-READY]
    ├── T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md [SPEC-READY]
    ├── T2_SCREEN_CAPTURE_DXGI_SPEC.md          [SPEC-READY]
    ├── T3_CONTINUOUS_STREAMING_SPEC.md         [SPEC-READY]
    ├── T4_REMOTE_CONTROL_SPEC.md                [SPEC-READY]
    ├── T5_CLASSROOM_COMMANDS_SPEC.md            [SPEC-READY]
    ├── T6_POLICY_ENGINE_FOCUS_MODE_SPEC.md      [SPEC-READY]
    ├── T7_DEVICE_HEALTH_SOFTWARE_SPEC.md        [SPEC-READY]
    ├── T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md    [SPEC-READY]
    └── FUTURE_PHASES_OVERVIEW.md                [ROADMAP-LEVEL — не implementation-ready]
```

## 2. Порядок чтения

**Для человека**, впервые открывающего проект:

```text
product/01_ROADMAP.md
        ↓
product/02_MARKET_AND_INVESTMENT_ANALYSIS.md
        ↓
product/03_EXECUTION_PLAN_90_DAYS.md
        ↓
architecture/01_TECHNICAL_ARCHITECTURE.md
        ↓
specs/BACKLOG.md → текущий milestone spec
```

**Для ИИ-агента**, которому поручили конкретную инженерную задачу — читать не всё подряд, а по роли:

| Роль агента | Subagent | Что читать в первую очередь |
| --- | --- | --- |
| Пишет код агента (Rust/Windows) | `classos-windows-engineer` | `CLAUDE.md` → `architecture/01_TECHNICAL_ARCHITECTURE.md` → нужный `specs/T<N>_*_SPEC.md` → `architecture/adr/` |
| Проектирует protocol/IPC изменения | `classos-windows-engineer` | `architecture/01_TECHNICAL_ARCHITECTURE.md` (§30–42) + текущий spec + обязателен новый ADR |
| Работает над Teacher Console (Tauri/React) | `classos-teacher-console-engineer` | `architecture/01_TECHNICAL_ARCHITECTURE.md` (§91–97) + `specs/T4_REMOTE_CONTROL_SPEC.md` |
| Работает над Cloud (Bun/TS/Postgres, T8+) | `classos-cloud-engineer` | `specs/T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md` + `architecture/adr/0008-monorepo-tooling.md` |
| Отвечает на продуктовые/бизнес-вопросы | — | `product/*` |
| Пишет новый spec следующего milestone | — | использовать skill `classos-write-spec`, за основу брать `specs/T0_*` как эталон формата |
| Оформляет архитектурное решение | — | использовать skill `classos-adr` |

## 3. Статус проекта

Сейчас: **pre-code, спецификации T0–T8 готовы**. Код не написан. Следующий шаг — реализация T0 строго по `specs/T0_SERVICE_SESSION_HOST_SPEC.md`, без забегания вперёд (см. `CLAUDE.md`, раздел «Правило одного milestone»).

Актуальный трекер статусов — `specs/BACKLOG.md`.
