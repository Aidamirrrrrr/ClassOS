# ClassOS T5 — classroom-команды

Статус по `docs/specs/BACKLOG.md`: **код завершён и проходит автоматические
проверки; проверка на реальных Windows-устройствах не проводилась.**

## Реализовано

- единый idempotent command envelope с дедлайном и закешированным результатом;
- типизированный маршрут Teacher → Service → Session Host;
- Lock/Unlock overlay, сообщение, URL, приложение только из локального каталога;
- строго типизированные Restart/Shutdown в Service без произвольных команд;
- параллельный fanout на выбранные устройства и частичные результаты в Teacher Console;
- unit-тесты дедлайна, idempotency, allowlist и protobuf round-trip.

## Ограничения и реальная приёмка

Lock overlay T5 — не security boundary: его enforcement заменит Policy Engine в T6.
Нужно проверить на выделенном Windows-стенде все действия, offline partial failure,
повтор после reconnect и физические Restart/Shutdown. До этого T5 не является
подтверждённым на целевой среде.
