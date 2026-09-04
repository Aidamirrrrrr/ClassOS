# ClassOS T1 — сеть и обнаружение устройств

Статус по `docs/specs/BACKLOG.md`: **реализация завершена; CI проходит, runtime-приёмка не проводилась.**

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
- control listener Agent с публикацией discovery только после успешного bind;
- явный `UpgradeRequired` при несовместимых версиях протокола;
- service-side обработчик enrollment и online heartbeat;
- подписанный Teacher credential и проверка application-level TeacherHello;
- минимальный Tauri Teacher Console: discovery, выпуск одноразового кода и enrollment.

## Дополнено позднее

- непрерывное обнаружение (`transport::listen_loop`) вместо одного объявления
  за нажатие кнопки: класс из пятнадцати машин собирается сам, а повторное
  объявление обновляет адрес устройства;
- список устройств в Teacher Console пополняется событиями по мере появления
  объявлений.

## Пока не реализовано

- reconnect и отображение всех состояний connection state machine в UI;
- обновление/отзыв credential и ротация ключа Teacher;
- список устройств не переживает перезапуск консоли: он строится заново из
  discovery, поэтому выключенное устройство исчезает из него молча;
- интеграционная проверка двух реальных Windows-компьютеров в одной LAN.

## Runtime validation

Discovery, TLS/enrollment и heartbeat пока не проверены между двумя реальными
Windows-компьютерами в одной локальной сети. Unit-тесты, локальный `cargo check`
и GitHub Actions не заменяют такую проверку. Этот сценарий оставлен обязательным
перед пилотной приёмкой.
