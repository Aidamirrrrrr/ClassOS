# 0006 — Product-level Policy abstraction поверх Windows enforcement

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

Windows предоставляет несколько независимых enforcement-механизмов (Assigned Access, AppLocker/App Control, GPO/CSP, Firewall, Chrome/Edge enterprise policies). Ни один из них сам по себе не покрывает весь продуктовый сценарий (динамический Lesson Policy, layered policy inheritance, Focus Mode как временный overlay).

## Рассмотренные варианты

1. **Завязаться напрямую на Assigned Access** — просто для базового kiosk-сценария, но не годится как единственный механизм: разные Windows editions, нет динамического per-lesson профиля, негибко для temporary overrides.
2. **Продуктовый слой (`Policy`) поверх нескольких enforcement providers** — API/UI оперируют абстракцией `LessonPolicy`/`ApplicationDefinition`, а `Policy Compiler` транслирует её в конкретные Assigned Access / AppLocker / registry-GPO / browser policy вызовы.

## Решение

Вариант 2. `policy-engine` крейт с трейтом `PolicyProvider` (`compile → validate → snapshot → apply → verify → commit`, с обязательным rollback), продуктовый YAML-подобный `LessonPolicy` не содержит registry keys напрямую (`architecture/01_TECHNICAL_ARCHITECTURE.md` §62–72).

## Последствия

- UI/API/Teacher никогда не видят и не конфигурируют GPO/AppLocker/SID напрямую (инвариант X, `CLAUDE.md`).
- Policy Compiler обязан автоматически добавлять allow-правила для собственных ClassOS-бинарников — иначе один баг policy заблокирует сам management layer (§68).
- Любая эффективная политика — детерминированный результат layering `Base + Branch + Room + Lesson + Temporary Override` (§70–72), а не императивный набор патчей.
- Требуется emergency break-glass механизм, доступный только локальному администратору (§69).
