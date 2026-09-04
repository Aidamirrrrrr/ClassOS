# 0012 — Remote input выполняется только в Session Host

**Статус:** Accepted
**Дата:** 2026-09-04

## Контекст

T4 добавляет mouse и keyboard input поверх уже аутентифицированного
Teacher ↔ Agent control-канала. Service работает в Session 0 как LocalSystem,
поэтому не имеет права и не должен пытаться взаимодействовать с desktop
ученика. Кроме того, ввод должен автоматически оставаться ограниченным UIPI.

## Рассмотренные варианты

1. Вызывать `SendInput` из Service — нарушает разделение Service / Session
   Host и не привязывает ввод к interactive desktop пользователя.
2. Передавать локально абсолютные пиксельные координаты — масштаб UI Teacher
   и разрешение student-дисплея будут расходиться.
3. Service только авторизует и маршрутизирует stateful control-сессию, а
   Session Host валидирует active session и вызывает `SendInput` с
   нормализованными координатами.

## Решение

Выбран вариант 3. Network и Named Pipe несут те же строго типизированные
remote-control сообщения; mouse-координаты задаются в диапазоне `0.0..=1.0`.
Service назначает единственного owner для устройства. Session Host принимает
input только пока получил подтверждённое состояние Active и показывает
неотключаемый индикатор control-сессии.

## Последствия

- Remote input не является generic command execution и не даёт Teacher
  возможностей LocalSystem.
- UIPI не обходится: elevated окна ученика остаются недоступны для control.
- При потере TLS-соединения Service обязан завершить сессию и передать stop в
  Session Host до очистки owner.
