# ClassOS — Spec & Implementation Backlog

Единственный источник правды о том, **что специфицировано, реализовано и проверено на целевой среде**. Эти три состояния ведутся независимо: готовый код не выдаётся за проверенный на реальной Windows-машине.

## Легенда

- **Spec:** `not started` / `draft` / `spec-ready` — готов ли implementation-ready документ.
- **Impl:** `not started` / `in progress` / `done` — реализован ли код и пройдены ли автоматические проверки.
- **Runtime validation:** `not started` / `CI only` / `partial` / `passed` — насколько реализация проверена запуском на целевой Windows-среде.

| Milestone | Spec | Impl | Runtime validation | Файл |
| --- | --- | --- | --- | --- |
| T0 — Service / Session Host | spec-ready | in progress | CI only | [T0_SERVICE_SESSION_HOST_SPEC.md](T0_SERVICE_SESSION_HOST_SPEC.md) |
| T1 — Network & Device Discovery | spec-ready | not started | not started | [T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md](T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md) |
| T2 — Screen Capture (DXGI) | spec-ready | not started | not started | [T2_SCREEN_CAPTURE_DXGI_SPEC.md](T2_SCREEN_CAPTURE_DXGI_SPEC.md) |
| T3 — Continuous Streaming | spec-ready | not started | not started | [T3_CONTINUOUS_STREAMING_SPEC.md](T3_CONTINUOUS_STREAMING_SPEC.md) |
| T4 — Remote Control | spec-ready | not started | not started | [T4_REMOTE_CONTROL_SPEC.md](T4_REMOTE_CONTROL_SPEC.md) |
| — CHECKPOINT #1 demo (реальному преподавателю) | — | — | — | `product/03_EXECUTION_PLAN_90_DAYS.md` §15 |
| T5 — Classroom Commands + Bulk Actions | spec-ready | not started | not started | [T5_CLASSROOM_COMMANDS_SPEC.md](T5_CLASSROOM_COMMANDS_SPEC.md) |
| T6 — Policy Engine + Focus Mode | spec-ready | not started | not started | [T6_POLICY_ENGINE_FOCUS_MODE_SPEC.md](T6_POLICY_ENGINE_FOCUS_MODE_SPEC.md) |
| — CHECKPOINT #2 demo | — | — | — | `product/03_EXECUTION_PLAN_90_DAYS.md` §22 |
| T7 — Device Health & Software Management | spec-ready | not started | not started | [T7_DEVICE_HEALTH_SOFTWARE_SPEC.md](T7_DEVICE_HEALTH_SOFTWARE_SPEC.md) |
| T8 — Installer, Updater, Cloud v0 | spec-ready | not started | not started | [T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md](T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md) |
| — Technical MVP complete / Go-No-Go #2 | — | — | — | `product/03_EXECUTION_PLAN_90_DAYS.md` §66–68 |
| Phase A — Lesson Engine | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase B — AlfaCRM Integration | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase C — AI Supervisor | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase D — AI Tutor | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase E — Analytics & Parent Reports | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase F — Retention Engine | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase G — Enterprise / Multi-branch | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |

## Правила обновления

1. Milestone не начинается в коде, пока его строка не `spec-ready`.
2. Impl переходит в `done`, когда код, автоматические тесты, lint, Windows cross-check и cross-build завершены. Непройденная проверка на реальной машине отражается отдельно и явно в `Runtime validation`.
3. Допускается начинать следующий технический milestone при `Impl: done` и `Runtime validation: CI only`, если непроверенные сценарии перечислены в README соответствующего milestone. Это не разрешает называть их проверенными.
4. Обязательные реальные контрольные точки: T0; связка T2–T4; полный прогон перед пилотом. Накопление кода не отменяет более ранние acceptance tests.
5. Gate-проверки CHECKPOINT #1/#2 и Go/No-Go остаются обязательными продуктовыми точками паузы.
6. При создании нового spec для Phase A–G — добавить отдельную строку с реальным именем файла (`NN_..._SPEC.md`), а не оставлять ссылку на `FUTURE_PHASES_OVERVIEW.md`.
