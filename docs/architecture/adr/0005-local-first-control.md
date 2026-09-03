# 0005 — Local-first classroom control

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

Занятие в классе не должно зависеть от качества интернета школы. Это одновременно продуктовое требование (`product/01_ROADMAP.md` §4) и техническое (`architecture/01_TECHNICAL_ARCHITECTURE.md` §5).

## Рассмотренные варианты

1. **Cloud-first**: Teacher Console управляет устройствами через облако как relay. Проще для multi-branch аналитики, но делает урок зависимым от внешнего интернета — неприемлемо для core value proposition («убрать Veyon»).
2. **Local-first с cloud-опциональной синхронизацией**: Teacher Console общается с устройствами напрямую по локальной сети; облако нужно для аккаунтов, конфигурации, аналитики, но не для core classroom actions.

## Решение

Local-first. Основные classroom-функции (discovery, screen, remote control, Focus Mode, launch/lock/restart) работают без облака. Авторизация преподавателя офлайн обеспечивается через signed classroom authorization lease, выданный заранее (`architecture/01_TECHNICAL_ARCHITECTURE.md` §46).

## Последствия

- Нельзя проектировать ни одну core-функцию так, чтобы она требовала round-trip в cloud в реальном времени.
- Нужен механизм offline-валидации подписанных lease на устройстве (криптографическая проверка локально, без сетевого запроса).
- Audit/telemetry буферизуются локально при отсутствии сети и синхронизируются после восстановления связи (§50–51).
