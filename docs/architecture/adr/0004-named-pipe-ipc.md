# 0004 — Named Pipe для локального Service ↔ Session Host IPC

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

Service и Session Host — два процесса на одной машине, требующие быстрого duplex-канала с возможностью ограничить доступ строго до пары «LocalSystem + конкретная сессия пользователя».

## Рассмотренные варианты

1. **Localhost TCP** — прост, но по умолчанию виден любому локальному процессу/пользователю без дополнительной изоляции; требует свой auth handshake поверх.
2. **Shared memory** — быстро, но сложна синхронизация и нет встроенного access control per-connection.
3. **Windows Named Pipe** — нативный duplex IPC, securable object с explicit DACL, не требует сетевого порта.

## Решение

Named Pipe (`\\.\pipe\classos\session-{sessionId}-{instanceId}`) с явно построенным security descriptor: полный доступ только LocalSystem и SID конкретной сессии пользователя; никакого `Everyone`/`Anonymous`/network-доступа (`architecture/01_TECHNICAL_ARCHITECTURE.md` §11–13, `specs/T0_*` §44–48).

## Последствия

- ACL строится динамически из SID пользователя при каждом запуске Session Host — нельзя хардкодить SID.
- Pipe предназначен исключительно для локального IPC этой пары процессов — это НЕ транспорт Teacher ↔ Agent (тот описан отдельно, ADR понадобится при проектировании T1).
- Framing протокола — length-prefixed protobuf, а не JSON (см. `specs/T0_*` §49–51) ради версионирования и совместимости.
