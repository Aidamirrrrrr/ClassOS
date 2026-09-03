---
name: classos-write-spec
description: Написать новый implementation-ready spec для следующего milestone ClassOS (T-серии или будущей бизнес-фазы) в установленном формате, согласованном с CLAUDE.md и существующими спеками T0-T8.
---

# ClassOS — Write Spec

Используется, когда нужно превратить roadmap-уровневое описание (из `product/01_ROADMAP.md` или `docs/specs/FUTURE_PHASES_OVERVIEW.md`) в implementation-ready spec, по которому реально можно писать код — по образцу `docs/specs/T0_SERVICE_SESSION_HOST_SPEC.md` .. `T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md`.

## Перед началом

1. Прочитать `docs/README.md` и `CLAUDE.md` целиком.
2. Прочитать **предыдущий** spec по номеру (например, перед написанием T9 — обязательно перечитать T8) — новый spec обязан явно указывать свою предпосылку (какой milestone должен быть завершён) и не дублировать/не противоречить уже принятым ADR.
3. Прочитать `docs/architecture/adr/` целиком — если новый spec подразумевает решение, которое требует ADR (см. список в `CLAUDE.md`), сначала предложить ADR, потом писать spec.
4. Свериться с `docs/ISSUES_AND_INCONSISTENCIES.md` — не наследовать уже отмеченные нестыковки в новый документ.

## Обязательная структура выходного файла

Файл: `docs/specs/NN_<КОРОТКОЕ_ИМЯ>_SPEC.md` (или в `docs/specs/` подходящего домена, если milestone относится к будущей бизнес-фазе — уточнить у пользователя расположение, если неочевидно).

```markdown
# ClassOS — <Milestone> Implementation Specification

**Файл:** `docs/specs/NN_..._SPEC.md`
**Статус:** Spec-ready | Draft
**Milestone:** <T-номер или Phase-буква>
**Предпосылка:** <какой предыдущий milestone должен быть завершён>

# 1. Цель
# 2. Definition of Done
# 3. Non-goals (что сознательно не входит)
# 4. Архитектурные решения, уже принятые (не переоткрывать без ADR)
# 5+. Технические разделы (протокол/сообщения, ключевые структуры, workflow)
# N. Security invariants <milestone>
# N+1. Тесты (Unit / Integration / Acceptance)
# N+2. Acceptance criteria
# N+3. Что дальше
```

## Правила содержания

- Каждый non-goal должен быть с явной причиной (почему не сейчас), а не просто списком.
- Каждое новое protobuf-сообщение — с комментарием, почему оно нужно именно в этой форме (units, normalized coordinates и т.п., по аналогии с существующими спеками).
- Security invariants — не общие фразы, а проверяемые тестом утверждения.
- Acceptance criteria должны быть измеримы (не «работает хорошо», а конкретные сценарии/пороги).
- В конце — ссылка на следующий spec и явное указание gate/checkpoint, если он есть перед следующим шагом (см. `product/03_EXECUTION_PLAN_90_DAYS.md` как источник чекпоинтов).

## После написания

1. Обновить `docs/specs/BACKLOG.md` — добавить строку с новым milestone, статус spec → `spec-ready`.
2. Если spec вводит новое существенное архитектурное решение — создать ADR по skill `classos-adr` **до**, не после, финализации spec.
3. Не писать код в рамках этого skill — только документ.
