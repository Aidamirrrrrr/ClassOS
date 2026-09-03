# ClassOS T1 — сеть и обнаружение устройств

Статус по `docs/specs/BACKLOG.md`: **реализация в процессе; runtime-проверка не начата.**

## Принятые границы реализации

- Сетевой protobuf-контракт остаётся в существующем крейте `protocol`.
- Переиспользуемые discovery и transport-механизмы находятся в отдельном
  крейте `transport`; он не содержит продуктовой логики и не принимает решений
  о доверии к устройству.
- UDP discovery всегда считается недоверенным. Доступ к control-каналу будет
  выдаваться только после enrollment и проверки identity через TLS.

## Порты и адреса

- IPv4 multicast-группа discovery: `239.255.67.79`.
- UDP discovery-порт: `45900`.
- TCP control-порт: `45901`.
- Multicast TTL равен `1`, поэтому объявления не должны покидать локальную сеть.

На 2026-09-04 порты `45900–45901` не зарегистрированы в реестре Service Name
and Transport Protocol Port Number Registry IANA. Это не исключает локальный
конфликт: ошибка bind должна быть явной. Все значения объявлены именованными
константами и могут быть заменены конфигурацией; в продуктовой логике магические
номера не используются. Решение целиком зафиксировано в ADR-0009.

## Реализовано

- отдельная схема `classos.network.v1`;
- version negotiation и protobuf round-trip тесты;
- публичное `ClassOSDeviceAnnouncement` без credentials и пользовательских данных;
- UDP broadcaster с интервалом 3–5 секунд и multicast TTL `1`;
- UDP listener, структурная валидация и сохранение фактического адреса источника.
- одноразовые enrollment-коды с TTL и привязкой к контексту;
- connection state machine и offline detection;
- генерация ECDSA device identity и SHA-256 fingerprint сертификата;
- защищённое DPAPI machine-scope хранение приватного ключа на Windows.
- TLS/TCP listener и клиент с length-prefixed protobuf framing;
- pinning SHA-256 fingerprint и отказ при подмене сертификата;
- отдельный bootstrap-режим, допустимый только для enrollment.

Broadcaster ещё не подключён к Windows Service: по ADR-0009 Service не должен
рекламировать control-порт до успешного запуска TLS listener.

## Пока не реализовано

- выпуск локального credential после проверки enrollment-кода;
- application gate и mutual authentication control-канала после enrollment;
- heartbeat, offline detection и reconnect;
- Teacher Console.

## Runtime validation

Discovery пока не проверен между двумя реальными Windows-компьютерами в одной
локальной сети. Unit-тест сокетного пути использует loopback и не заменяет такую
проверку.
