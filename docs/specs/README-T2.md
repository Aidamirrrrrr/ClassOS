# ClassOS T2 — одиночный снимок экрана

Статус: **реализация в процессе; контракт и тестовый pipeline готовы, DXGI и
реальный capture ещё не проверялись на Windows/GPU.**

## Уже реализовано

- crate `screen-capture` с разделёнными контрактами `ScreenCapture` и
  `FrameEncoder`;
- `MockCapture` с обнаружением нескольких дисплеев и выбором primary;
- JPEG-кодирование RGB-кадра без сохранения на диск;
- protobuf-сообщения `CaptureRequest`, `Frame`, `CaptureError` для local IPC;
- protobuf-сообщения `ScreenshotRequest`, `ScreenFrame` для T1-канала;
- encode/decode и JPEG round-trip unit-тесты.

## Следующие шаги

1. Реализовать DXGI Desktop Duplication за `ScreenCapture` в Session Host.
2. Протянуть `CaptureRequest` через T0 IPC и `ScreenFrame` через Service.
3. Добавить кнопку снимка и отображение JPEG в Teacher Console.
4. Провести обязательную приёмку на Intel/AMD/NVIDIA, 1080p/4K и двух
   мониторах.

Кадры по умолчанию живут только в памяти: capture → encode → отправка →
discard. Debug-dump на диск в T2 не добавляется.
