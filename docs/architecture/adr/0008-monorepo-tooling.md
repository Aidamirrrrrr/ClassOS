# 0008 — Инструментарий монорепозитория: pnpm workspaces + Cargo workspace

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

Проект объединяет два независимых языковых стека в одном репозитории: Rust (Student Agent — Service/Session Host, крейты по `architecture/01_TECHNICAL_ARCHITECTURE.md` §14–16) и TypeScript (Teacher Console на Tauri 2 + React, позже Cloud на Bun — `CLAUDE.md`, стек). Нужно решить, как организовать репозиторий так, чтобы:

- оба стека жили в одном репозитории (монорепо), а не в двух отдельных;
- JS-пакеты могли переиспользовать общий код (типы протокола, UI-компоненты) без публикации в npm;
- Rust-часть могла эволюционировать по T-milestone'ам (T0 — 5 крейтов, далее добавляются `screen-capture`, `remote-input`, `policy-engine` и т.д. — см. `docs/ISSUES_AND_INCONSISTENCIES.md` §3) без переезда на другую систему сборки;
- CI и локальная разработка не усложнялись сверх необходимого на раннем этапе (проект ещё pre-code, дороговизна ошибиться в выборе сейчас — низкая, но выбор должен быть осознанным, не first-thing-that-worked).

## Рассмотренные варианты

1. **Два отдельных репозитория** (`classos-agent` на Rust, `classos-console`/`classos-cloud` на TS) — проще для CI изоляции, но затрудняет совместное версионирование protobuf-схемы (`protocol` крейт нужен и агенту, и — потенциально — Teacher Console через generated TS types), усложняет атомарные PR, меняющие протокол сразу с двух сторон.
2. **Монорепо на Nx** — мощная система с кэшированием графа задач и генераторами, но существенно тяжелее по конфигурации и порогу входа, чем нужно проекту на этой стадии (founder + возможно один Windows-инженер, см. `product/02_MARKET_AND_INVESTMENT_ANALYSIS.md` §57).
3. **Монорепо на pnpm workspaces (JS-часть) + отдельный Cargo workspace (Rust-часть), без общего build-оркестратора (Turborepo/Nx) на старте** — простейшая связка, которую поддерживают из коробки и Tauri, и Cargo; не требует изучения дополнительного инструмента; можно добавить Turborepo позже аддитивно (не breaking), когда реально возникнет проблема с временем сборки/кэшированием, а не заранее.

## Решение

Вариант 3. Единый git-репозиторий:

```text
classos/
├── apps/
│   └── teacher/              # Tauri 2 + React + TypeScript, появляется начиная с T1 UI
├── crates/
│   ├── agent-service/
│   ├── agent-session/
│   ├── agent-core/
│   ├── protocol/
│   ├── windows-platform/
│   └── ...                   # screen-capture (T2), remote-input (T4), policy-engine (T6),
│                              # software-manager/device-health (T7), updater (T8) — по мере milestone'ов
├── packages/
│   └── shared/                # общий TS-код (сгенерированные из .proto типы и т.п.), появляется когда реально понадобится
├── services/
│   └── cloud/                 # Bun + TypeScript + PostgreSQL, появляется в T8 (Cloud v0)
├── proto/
│   ├── local_ipc.proto
│   └── classos_network.proto
├── installer/
├── docs/
├── pnpm-workspace.yaml         # объединяет apps/*, packages/*, services/* — создаётся при первом реальном JS-пакете
├── Cargo.toml                  # workspace root для crates/* — создаётся в T0 Step 1
└── package.json                # root, только dev-tooling (markdownlint и т.п.), без рантайм-зависимостей
```

Пакетный менеджер JS-стороны — **pnpm** (не npm/yarn): быстрее, экономнее по диску за счёт content-addressable store, стандартный выбор для Tauri-монорепозиториев.

Turborepo/Nx **не вводятся сейчас**. Это дополняемое решение: пока в JS-части один реальный пакет (`apps/teacher`), оркестратор сборки не даёт выгоды и добавляет конфигурацию, которую некому поддерживать. Триггер для повторного рассмотрения — когда пакетов станет 3+ и локальная пересборка/тесты станут заметно медленными.

Rust workspace (`Cargo.toml` в корне, `crates/*` как members) физически создаётся **в момент старта T0**, согласно `specs/T0_SERVICE_SESSION_HOST_SPEC.md` Step 1 — не раньше, чтобы не размывать «правило одного milestone» (`CLAUDE.md`). JS workspace (`pnpm-workspace.yaml`, `apps/teacher`) создаётся, когда стартует реализация Teacher UI (первое появление — в рамках T1 согласно `specs/T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md` §10, минимальный экран статуса устройств).

## Последствия

- До старта T0 в репозитории физически нет ни `Cargo.toml`, ни `pnpm-workspace.yaml` — это осознанно, а не забыто: эта ADR фиксирует **выбор инструментов**, а не создаёт скелет заранее.
- `protocol` крейт (Rust, из .proto) и будущий generated TS client должны собираться из одного и того же `proto/*.proto` источника — конкретный механизм генерации TS-типов (например через `protoc` + `ts-proto` в `packages/shared` или прямой вызов из `apps/teacher/src-tauri`) не решается этой ADR и должен быть зафиксирован отдельно (в T1-спеке или отдельной ADR), когда Teacher Console реально начнёт декодировать `classos_network.proto`.
- Если позже потребуется Turborepo/Nx — это аддитивное изменение (не меняет layout, только добавляет task-runner поверх), отдельного ADR не требует, но стоит явно отметить в этом файле как `Superseded`-подобное дополнение, если решение примется.
- CI должен будет параллельно поддерживать два тулчейна (Rust + Node/pnpm) в одном пайплайне — это не новая проблема монорепо, а прямое следствие уже принятого стека (ADR-0001), просто здесь явно зафиксировано организационно.
