# 0007 — Локальная enrollment-заглушка в T1, замена на Cloud issuer в T8

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

`architecture/01_TECHNICAL_ARCHITECTURE.md` §44 описывает enrollment как процесс с выпуском device certificate облаком. Но реальный Cloud backend появляется только в T8 (`specs/T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md`), а сетевой стек (T1) нужен раньше. Нужно решить, как T1–T7 проходят enrollment без облака, не блокируя всю сетевую спецификацию до T8.

## Рассмотренные варианты

1. **Отложить весь T1 до T8** — архитектурно чище (enrollment сразу «правильный»), но противоречит инкрементальному порядку T-milestone'ов и продуктовому принципу «не строить полгода платформу» (`product/03_EXECUTION_PLAN_90_DAYS.md` §3).
2. **Локальная enrollment-заглушка через Teacher Console как временный admin authority** (`specs/T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md` §6.2) — Teacher Console временно выполняет роль issuer'а one-time кода и подтверждения enrollment, с протокольным контрактом (`EnrollmentRequest`/`EnrollmentResult`), спроектированным сразу совместимым с будущим Cloud issuer.

## Решение

Вариант 2. T1 реализует enrollment локально; протокольные сообщения `EnrollmentRequest`/`EnrollmentResult` фиксируются в T1 и **не меняются** при переезде issuance authority в Cloud на T8 (`specs/T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md` §6).

## Последствия

- Между T1 и T8 enrollment security модель слабее целевой (issuer — сам Teacher Console, а не независимый Cloud CA) — приемлемо только для design-partner пилотов, не для реального security-review перед платным rollout (см. security checklist в T8 §10).
- Если при реализации T1 схема `EnrollmentRequest`/`EnrollmentResult` будет спроектирована иначе, чем описано в spec — это уже расхождение с этим ADR, и переезд на Cloud issuer в T8 потребует либо миграции схемы (новый ADR, superseding этот), либо исправления реализации T1 под контракт.
- Приватный ключ устройства (ADR через `specs/T1_*` §6.1) не меняет модель хранения при переходе issuer'а — это осталось внешним по отношению к решению здесь.
