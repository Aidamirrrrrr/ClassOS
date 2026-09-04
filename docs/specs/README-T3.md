# ClassOS T3 — непрерывный screen stream

Статус по `docs/specs/BACKLOG.md`: **код завершён и проходит автоматические
проверки; end-to-end поток и нагрузочная LAN-приёмка не проводились.**

## Реализовано

- `StreamSubscribe` и `StreamUnsubscribe` в сетевом протоколе;
- режимы `Thumbnail` и `Selected` в `ScreenFrame`;
- `StreamVisibility` и `negotiate_schedule` в `agent-core`;
- ограничения Agent: thumbnail 1–2 FPS/до 640 px, selected до 15 FPS/3840 px;
- скрытая вкладка (`Hidden`) полностью прекращает отправку кадров;
- unit-тесты negotiation и protobuf round-trip.

## Далее

- adaptive loop capture → encode → отправка кадров;
- приоритет control-сообщений над screen queue;
- подписка нескольких устройств в Teacher Console;
- reconnect/unsubscribe и нагрузочные тесты 10–20 устройств.
