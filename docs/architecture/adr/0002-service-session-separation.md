# 0002 — Разделение Service / Session Host

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

Windows с Vista изолирует службы в Session 0; служба не должна напрямую взаимодействовать с интерактивным desktop пользователя (`architecture/01_TECHNICAL_ARCHITECTURE.md` §6). Нужен способ делать screen capture, remote input, показывать overlay — то есть работать внутри пользовательской сессии — не давая этому коду привилегии LocalSystem.

## Рассмотренные варианты

1. **Один процесс `agent.exe`, работающий как interactive service** — устаревший паттерн (не поддерживается по факту с Vista/7), небезопасен: весь функционал получает LocalSystem без необходимости.
2. **Servic e + Session Host, разделённые IPC** — служба (LocalSystem) отвечает за privileged operations, отдельный процесс в сессии пользователя — за UI/capture/input.

## Решение

Два runtime-компонента: `ClassOS Service` (LocalSystem, Session 0) и `ClassOS Session Host` (standard user, интерактивная сессия), связанные через локальный аутентифицированный IPC (см. ADR-0004).

## Последствия

- Session Host по определению **partially untrusted** — Service не доверяет данным от него без независимой проверки через Windows API (`ProcessIdToSessionId`, `GetNamedPipeClientProcessId`) — см. §116, §59–60 T0 spec.
- Компрометация Session Host (student пытается на него влиять) не даёт атакующему LocalSystem.
- Усложняет lifecycle: нужен supervisor с desired-state reconciliation (см. `specs/T0_*` §67–71), а не просто «запустить один exe».
