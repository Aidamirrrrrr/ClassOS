# ClassOS — инструкции для ИИ-агентов

ClassOS — система управления компьютерными классами для IT-школ (замена Veyon + Windows classroom control + со временем lesson-aware orchestration layer). Полный контекст — в `docs/README.md`.

**Статус проекта: pre-code.** Спецификации milestone'ов T0–T8 готовы в `docs/specs/`. Кода ещё нет. Не начинай писать implementation, пока не прочитал spec нужного milestone и `docs/architecture/01_TECHNICAL_ARCHITECTURE.md`.

## Обязательное чтение перед любой инженерной задачей

1. `docs/README.md` — карта документации и кто что читает.
2. `docs/architecture/01_TECHNICAL_ARCHITECTURE.md` — как всё устроено.
3. `docs/specs/BACKLOG.md` — какой milestone сейчас в работе.
4. Spec конкретного milestone из `docs/specs/`.
5. `docs/architecture/adr/` — уже принятые архитектурные решения (не переоткрывать их без нового ADR).

## Жёсткие архитектурные инварианты

Эти правила нельзя нарушать ради скорости (источник: `01_TECHNICAL_ARCHITECTURE.md` §160):

1. `LocalSystem Service ≠ Interactive UI` — привилегированный код и UI пользователя разделены (Service / Session Host).
2. `Discovery ≠ Trust` — обнаружение устройства в сети не даёт доступа; нужна отдельная аутентификация.
3. `Student session ≠ Privileged authority` — Session Host никогда не получает LocalSystem token и не является доверенной стороной для Service.
4. Любая Policy обязана иметь rollback.
5. Отключение Cloud не должно останавливать урок (local-first).
6. Remote control обязан быть аутентифицирован и залогирован в audit.
7. Экраны/скриншоты эфемерны по умолчанию — без постоянного хранения.
8. Teacher не может выполнять произвольные SYSTEM-команды.
9. Обновления агента только подписанные (Authenticode + hash + manifest signature).
10. Product API/UI никогда не показывает сырые Windows-механизмы (GPO/AppLocker/SID/registry) напрямую пользователю.

## Правило одного milestone

Реализуется **только текущий** milestone из `docs/specs/BACKLOG.md`. Если во время работы появляется желание «заодно добавить» что-то из следующего T — не делать. Каждый spec явно перечисляет Non-goals/Out of scope — это обязательная часть контракта, а не пожелание.

## Когда обязателен новый ADR

Перед любым решением, которое меняет:

- транспорт/протокол (proto-схему, framing, versioning);
- разделение привилегий Service/Session Host;
- модель аутентификации/enrollment устройства;
- выбор технологии enforcement (AppLocker vs Assigned Access vs что-то ещё).

Создавать ADR через skill `classos-adr` (см. `.claude/skills/classos-adr/`), нумерация продолжается от последнего файла в `docs/architecture/adr/`.

## Продуктовые принципы (коротко)

- Не переписывать Windows — оркестрировать её (AppLocker/Assigned Access/GPO/WinGet/DXGI/SendInput), не изобретать свои security-механизмы.
- Zero bullshit UX: преподаватель никогда не видит GPO/CSP/AppLocker/SID/WMI — только `[Python] [Roblox] [Focus] [Lock Class]`.
- Не начинать с AI. AI — после подтверждённого PMF на classroom-control (см. `product/03_EXECUTION_PLAN_90_DAYS.md`, §49).
- Приватность by design: без скрытого мониторинга, keylogging, постоянной записи экрана/микрофона/камеры.

## Стек (зафиксирован, не менять без ADR)

| Компонент | Технология |
| --- | --- |
| Student Agent (Service + Session Host) | Rust, `windows-rs`, Tokio |
| Teacher Console | Tauri 2 + React + TypeScript |
| Cloud | Bun + TypeScript + PostgreSQL (Redis — только когда действительно понадобится) |
| Protocol | Protocol Buffers, versioned envelope, schema-first |
| Monorepo tooling | pnpm workspaces (JS) + Cargo workspace (Rust), без Turborepo/Nx на старте — см. `architecture/adr/0008-monorepo-tooling.md` |

## Что не делать в первой версии

Полный список — `product/01_ROADMAP.md` §45. Коротко: свой Explorer/ОС/антивирус/kernel driver/package manager/браузер/сложный WFP filter, постоянный AI video analysis, parent app, CRM, billing, full LMS, churn ML, macOS/Linux.

## Если чего-то нет в spec'ах

Не додумывать архитектурное решение на месте. Если задача требует решения, не покрытого текущими доками — остановиться и предложить: (а) короткий ADR, либо (б) вопрос пользователю. Не документировать решение только в коде/комментариях — spec в `docs/` первичен.

## Commit messages

Conventional Commits: `<type>: <описание>` (`docs`, `feat`, `fix`, `chore`, `refactor`, `test`). Subject — короткий и в повелительном наклонении, тело коммита — при необходимости пояснить «почему». Пока в репозитории только документация — используется `docs:`; после старта T0 добавятся `feat:`/`fix:`/`chore:` для соответствующих изменений.
