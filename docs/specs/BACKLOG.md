# ClassOS — Spec & Implementation Backlog

Единственный источник правды о том, **что специфицировано, реализовано и проверено на целевой среде**. Эти три состояния ведутся независимо: готовый код не выдаётся за проверенный на реальной Windows-машине.

## Легенда

- **Spec:** `not started` / `draft` / `spec-ready` — готов ли implementation-ready документ.
- **Impl:** `not started` / `in progress` / `done` — реализован ли код и пройдены ли автоматические проверки.
- **Runtime validation:** `not started` / `CI only` / `partial` / `passed` — насколько реализация проверена запуском на целевой Windows-среде.

| Milestone | Spec | Impl | Runtime validation | Файл |
| --- | --- | --- | --- | --- |
| T0 — Service / Session Host | spec-ready | done | CI only | [T0_SERVICE_SESSION_HOST_SPEC.md](T0_SERVICE_SESSION_HOST_SPEC.md) |
| T1 — Network & Device Discovery | spec-ready | done | CI only | [T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md](T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md) |
| T2 — Screen Capture (DXGI) | spec-ready | done | not started | [T2_SCREEN_CAPTURE_DXGI_SPEC.md](T2_SCREEN_CAPTURE_DXGI_SPEC.md) |
| T3 — Continuous Streaming | spec-ready | done | not started | [T3_CONTINUOUS_STREAMING_SPEC.md](T3_CONTINUOUS_STREAMING_SPEC.md) |
| T4 — Remote Control | spec-ready | done | not started | [T4_REMOTE_CONTROL_SPEC.md](T4_REMOTE_CONTROL_SPEC.md) |
| — CHECKPOINT #1 demo (реальному преподавателю) | — | — | — | `product/03_EXECUTION_PLAN_90_DAYS.md` §15 |
| T5 — Classroom Commands + Bulk Actions | spec-ready | done | not started | [T5_CLASSROOM_COMMANDS_SPEC.md](T5_CLASSROOM_COMMANDS_SPEC.md) |
| T6 — Policy Engine + Focus Mode | spec-ready | done | not started | [T6_POLICY_ENGINE_FOCUS_MODE_SPEC.md](T6_POLICY_ENGINE_FOCUS_MODE_SPEC.md) |
| — CHECKPOINT #2 demo | — | — | — | `product/03_EXECUTION_PLAN_90_DAYS.md` §22 |
| T7 — Device Health & Software Management | spec-ready | done | not started | [T7_DEVICE_HEALTH_SOFTWARE_SPEC.md](T7_DEVICE_HEALTH_SOFTWARE_SPEC.md) |
| T8 — Installer, Updater, Cloud v0 | spec-ready | done | not started | [T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md](T8_INSTALLER_UPDATER_CLOUD_V0_SPEC.md) |
| — Technical MVP complete / Go-No-Go #2 | — | — | — | `product/03_EXECUTION_PLAN_90_DAYS.md` §66–68 |
| Phase A — Lesson Engine | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase B — AlfaCRM Integration | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase C — AI Supervisor | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase D — AI Tutor | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase E — Analytics & Parent Reports | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase F — Retention Engine | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |
| Phase G — Enterprise / Multi-branch | not started (overview only) | not started | not started | [FUTURE_PHASES_OVERVIEW.md](FUTURE_PHASES_OVERVIEW.md) |

## Сквозная ревизия 2026-09-04

Проведена в два прохода: сначала поиск кода без вызывающих, затем сверка
каждого milestone с acceptance-критериями его спеки. Ни один пункт не меняет
`Runtime validation` — исправленный и проверенный автоматикой код по-прежнему
ни разу не запускался в классе.

### Проход 1: код, написанный и никем не вызываемый

Существовал как библиотека с тестами, а не как функция продукта.

| Что было | Состояние сейчас |
| --- | --- |
| `PriorityQueue` без вызывающих; кадры писались прямо в сокет | Очередь в реальном пути отправки, чтение не ждёт записи (README-T3) |
| `transport::lease::authorize` без вызывающих | Право проверяется на каждой операции (ADR-0016) |
| Крейт `updater` без вызывающих; установщик его не копировал | Служба проверяет обновления и запускает updater (ADR-0015) |
| Манифест обновления подписывался только в тестах | `sign-manifest.ts` + кросс-языковой вектор |
| Cloud только in-memory, `schema.sql` никем не выполнялась | PostgreSQL-адаптер, CI поднимает настоящую базу |
| Cloud ни с чем не соединён | Консоль входит в Cloud, получает lease, регистрирует устройства |
| Обнаружение по одному устройству на нажатие | Непрерывное обнаружение, список пополняется сам |

### Проход 2: дефекты, найденные сверкой со спеками

Первые четыре означали, что функция не сработала бы **ни разу** на реальной
машине; сборка и тесты их пропускали, потому что ломалось взаимодействие
частей, а не части по отдельности.

| Дефект | Последствие | Где |
| --- | --- | --- |
| `interval` выдаёт первый тик немедленно: агент слал heartbeat и health-отчёт раньше ответа на запрос | Снимок экрана, старт remote control, classroom-команды и запрос состояния всегда падали с «неожиданным ответом» | T2, T4, T5, T7 |
| `RepairResult` приходит перед `CommandResult`, консоль читала одно сообщение | «Привести к профилю» всегда выглядело как сбой связи | T7 §8 |
| Allow-правила AppLocker по голому имени файла | Focus Mode обходится переименованием под standard-user — DoD §2 не выполнялся | T6, ADR-0017 |
| `primary` — первый выход DXGI, а не основной монитор | На двух мониторах преподаватель видел не тот экран | T2 §13.2 |
| `winget::is_available()` не вызывался | Машина без winget отчитывалась как машина без единой нужной программы | T7 §4.2 |
| `CaptureError` в потоке игнорировался консолью | Сбой захвата выглядел как застывшая картинка | T2 §13.5 |
| `PRIMARY KEY` с выражением, `citext` без `CREATE EXTENSION`, jsonb возвращается строкой | Схема не применялась ни на одной базе; аудит расходился с in-memory контрактом | T8 §4.1 |
| Второй crypto-провайдер rustls от нового HTTP-клиента | TLS-транспорт агента падал бы в рантайме | T1 |

Три последние строки таблицы найдены не чтением, а исполнением: джобой CI с
настоящим PostgreSQL и прогоном workspace-тестов. Это аргумент за то, чтобы
проверок исполнением становилось больше, а не меньше.

### Чем закрыты дыры в проверках

- CI поднимает настоящий PostgreSQL и **падает**, если его тесты пропущены;
- CI собирает фронтенд production-сборкой и запускает тесты backend консоли —
  раньше приложение не собиралось нигде;
- конвейер проверки обновлений исполняется в тестах против локального
  HTTP-сервера, а не только компилируется;
- три кросс-языковых контракта (lease, поля манифеста, подпись манифеста)
  закреплены векторами с обеих сторон.

## Как снимается `Runtime validation`

Прогон ведётся по [RUNTIME_ACCEPTANCE.md](RUNTIME_ACCEPTANCE.md): пункт
отмечается пройденным только по наблюдаемому результату, частично пройденный
блок отмечается частично. Округление в пользу «работает» здесь дороже
честного отказа — следующий, кто откроет эту таблицу, будет строить планы на
её отметках.

## Правила обновления

1. Milestone не начинается в коде, пока его строка не `spec-ready`.
2. Impl переходит в `done`, когда код, автоматические тесты, lint, Windows cross-check и cross-build завершены. Непройденная проверка на реальной машине отражается отдельно и явно в `Runtime validation`.
3. Допускается начинать следующий технический milestone при `Impl: done` и `Runtime validation: CI only`, если непроверенные сценарии перечислены в README соответствующего milestone. Это не разрешает называть их проверенными.
   - **Исключение, действующее сейчас:** T7 и T8 начаты по явному разрешению владельца проекта при `Runtime validation: not started` у T6. Правило не отменено: долг по реальной приёмке T0–T8 накапливается и остаётся обязательным перед пилотом. Каждый milestone, начатый по этому исключению, отмечает его в своём README.
4. Обязательные реальные контрольные точки: T0; связка T2–T4; полный прогон перед пилотом. Накопление кода не отменяет более ранние acceptance tests.
5. Gate-проверки CHECKPOINT #1/#2 и Go/No-Go остаются обязательными продуктовыми точками паузы.
6. При создании нового spec для Phase A–G — добавить отдельную строку с реальным именем файла (`NN_..._SPEC.md`), а не оставлять ссылку на `FUTURE_PHASES_OVERVIEW.md`.
