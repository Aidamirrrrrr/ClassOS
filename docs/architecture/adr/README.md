# Architecture Decision Records

ADR фиксирует **необратимое или дорогое в развороте** архитектурное решение: контекст, альтернативы, выбор, последствия. Не каждое решение достойно ADR — только те, что перечислены в `CLAUDE.md` («Когда обязателен новый ADR»), либо любое другое решение, которое сложно будет отменить через полгода.

## Шаблон

```markdown
# NNNN — Заголовок решения

**Статус:** Accepted | Superseded by NNNN | Deprecated
**Дата:** YYYY-MM-DD

## Контекст
Какая проблема решается, какие силы/ограничения на неё давят.

## Рассмотренные варианты
1. Вариант A — плюсы/минусы
2. Вариант B — плюсы/минусы

## Решение
Что выбрано и почему именно это.

## Последствия
Что это даёт, чем платим, что теперь нельзя делать иначе без нового ADR.
```

## Индекс

| # | Заголовок | Статус |
| --- | --- | --- |
| [0001](0001-rust-agent.md) | Rust для Windows-агента | Accepted |
| [0002](0002-service-session-separation.md) | Разделение Service / Session Host | Accepted |
| [0003](0003-dxgi-screen-capture.md) | DXGI Desktop Duplication как основной screen backend | Accepted |
| [0004](0004-named-pipe-ipc.md) | Named Pipe для локального Service ↔ Session Host IPC | Accepted |
| [0005](0005-local-first-control.md) | Local-first classroom control | Accepted |
| [0006](0006-policy-engine-abstraction.md) | Product-level Policy abstraction поверх Windows enforcement | Accepted |
| [0007](0007-t1-local-enrollment-stub.md) | T1 local enrollment stub, заменяется Cloud issuer в T8 | Accepted |
| [0008](0008-monorepo-tooling.md) | Монорепозиторий: pnpm workspaces + Cargo workspace, без Turborepo на старте | Accepted |
| [0009](0009-t1-network-transport.md) | Сетевой транспорт и discovery для T1 | Accepted |
| [0010](0010-t1-device-key-storage.md) | Хранение закрытого ключа устройства в T1 | Accepted |
| [0011](0011-t3-parameterized-local-capture.md) | Параметризованный захват кадра в локальном IPC T3 | Accepted |
| [0012](0012-t4-remote-input-session-boundary.md) | Remote input выполняется только в Session Host | Accepted |
| [0013](0013-t5-command-privilege-routing.md) | Classroom-команды проходят через границу привилегий | Accepted |
| [0014](0014-t6-windows-enforcement-providers.md) | Выбор Windows enforcement-технологии для T6 | Accepted |
| [0015](0015-t8-agent-update-channel.md) | Канал получения обновлений агентом | Accepted |
| [0016](0016-t8-lease-authorization-on-device.md) | Проверка прав преподавателя на устройстве по classroom lease | Accepted |
| [0017](0017-t6-applocker-rule-conditions.md) | Условия правил AppLocker: где именно разрешён исполняемый файл | Accepted |
| [0018](0018-enrollment-window-and-credential-reissue.md) | Аутентификация окна enrollment и перевыпуск device credential | Accepted |

Следующий свободный номер: **0019**.
