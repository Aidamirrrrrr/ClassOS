# ClassOS — T2 Implementation Specification

**Файл:** `docs/specs/T2_SCREEN_CAPTURE_DXGI_SPEC.md`
**Статус:** Spec-ready
**Milestone:** T2
**Предпосылка:** T1 завершён (аутентифицированное сетевое соединение Teacher ↔ Agent работает)

---

## 1. Цель T2

Доказать одиночный кадр экрана: Teacher нажимает «Take Screenshot» на конкретном enrolled устройстве и видит актуальный desktop ученика.

T2 **не** делает continuous streaming (это T3) и **не** делает remote input (T4).

---

## 2. Definition of Done

```text
Teacher Console → выбрать online device → [Take Screenshot]
↓
Session Host выполняет DXGI capture
↓
кадр кодируется в JPEG
↓
передаётся по T1-каналу Service → Teacher
↓
Teacher видит актуальный desktop (не устаревший, не чёрный экран)
```

Повторить на устройстве с двумя мониторами — как минимум detection + захват primary работает.

---

## 3. Non-goals

```text
continuous streaming / FPS-target
video encoding (H.264 и т.п.)
dirty-region optimization
remote input
multi-display switching UI
```

---

## 4. Архитектурные решения (не переоткрывать)

- ADR-0003: DXGI Desktop Duplication — основной backend, за трейтом `ScreenCapture`.
- Capture выполняется в **Session Host** (interactive session), не в Service — Service не имеет доступа к desktop пользователя (ADR-0002). Кадр передаётся через уже существующий T0 Named Pipe IPC до Service, а дальше — через T1 сетевой канал до Teacher.

```text
DXGI (Session Host)
↓ Named Pipe (T0 IPC, новое сообщение Frame)
Service
↓ T1 network channel (новое сообщение ScreenFrame)
Teacher Console
```

---

## 5. `ScreenCapture` trait

```rust
trait ScreenCapture {
    fn displays(&self) -> Result<Vec<Display>>;
    fn start(&mut self, display: DisplayId) -> Result<()>;
    fn next_frame(&mut self) -> Result<Frame>;
    fn stop(&mut self);
}
```

Implementations:

```text
DxgiDesktopCapture   — production
MockCapture          — для unit-тестов Supervisor/pipeline без реального Win32/GPU
```

`Frame` — raw кадр (ширина/высота/формат/буфер), без знания о сети/кодировании — encoding отделено (`FrameEncoder`, см. §7).

---

## 6. Новые сообщения протокола

### 6.1 Local IPC (`local_ipc.proto`, расширение T0 envelope)

```protobuf
message CaptureRequest {
  uint32 display_id = 1;
}

message Frame {
  uint32 display_id = 1;
  uint32 width = 2;
  uint32 height = 3;
  bytes encoded_data = 4;   // уже JPEG на этом этапе — Session Host кодирует сам
  string format = 5;        // "jpeg"
}

message CaptureError {
  string code = 1;          // machine-readable, см. §127 арх. RFC
  string message = 2;
}
```

### 6.2 Network protocol (`classos_network.proto`, расширение T1 envelope)

```protobuf
message ScreenshotRequest {
  string device_id = 1;
}

message ScreenFrame {
  string device_id = 1;
  uint32 display_id = 2;
  uint32 width = 3;
  uint32 height = 4;
  bytes encoded_data = 5;
  string format = 6;
  int64 captured_at_unix_ms = 7;
}
```

Почему кодирование делает Session Host, а не Service: избегаем лишней копии сырого кадра через privileged процесс без необходимости, и Service остаётся «тонким» относительно UI-специфичной работы (соответствует ADR-0002 — Service не занимается desktop-related операциями напрямую).

---

## 7. Encoding

MVP-декодер — **JPEG**, простая реализация, без hardware acceleration (`01_TECHNICAL_ARCHITECTURE.md` §22). Обязательно спроектировать trait, чтобы H.264 (T3+/будущее) подключался без изменения вызывающего кода:

```rust
trait FrameEncoder {
    fn encode(&mut self, frame: RawFrame) -> Result<EncodedFrame>;
}
```

---

## 8. Multiple displays

```text
Device reports:
Display 0 — 1920x1080 primary
Display 1 — 1920x1080
```

T2 обязателен: detection всех дисплеев + захват primary. Переключение на non-primary в Teacher UI — не обязательно для T2 (можно реализовать сразу, если дёшево, но не блокер DoD).

---

## 9. Приватность (обязательное поведение, не опция)

```text
capture → encode → отправка → discard
```

Никакого персистентного сохранения кадра на диск ни на Session Host, ни на Service, ни в Teacher Console по умолчанию (`01_TECHNICAL_ARCHITECTURE.md` §121, §42 roadmap). Если для отладки нужен dump на диск — только за explicit debug-флагом, никогда не включённым по умолчанию, и такой путь обязателен к упоминанию в `README-T2.md` как известное ограничение/debug-only feature.

---

## 10. Индикация на стороне ученика

T2 показывает единичный скриншот — сам факт захвата one-off кадра менее заметен, чем continuous stream, но **тем не менее** это остаётся "экран показан преподавателю" — правило прозрачности (§120 арх. RFC: не делать скрытое наблюдение) обязывает: если Session Host в принципе уже умеет показывать overlay (заложено в T0/T1 UI-слое), к моменту T3 (continuous stream) индикатор обязателен. Для T2 (единичный скриншот по явному запросу teacher) минимально допустимо отложить постоянный визуальный индикатор до T3/T4, но это нужно явно зафиксировать как временное исключение в `README-T2.md`, а не молчаливо забыть.

---

## 11. Security

- `ScreenshotRequest` разрешён только через уже аутентифицированное T1-соединение — никакого отдельного anonymous capture endpoint.
- Ошибки capture (`CaptureError`) не должны утекать debug-информацию, потенциально ценную атакующему (пути на диске, версии драйверов) в user-facing UI — логировать подробно, показывать teacher — только machine-readable код + человеко-понятное сообщение.

---

## 12. Тесты

### Unit

```text
Frame/CaptureRequest/CaptureError encode-decode
MockCapture pipeline: display list, start/stop, next_frame
FrameEncoder trait: JPEG encode roundtrip (encode → decode → сравнить размеры/не пустой буфер)
```

### Integration (реальное железо, не только VM — см. арх. RFC §134)

```text
Intel integrated GPU
AMD
NVIDIA
1080p / 4K
single display / dual display
full-screen DirectX приложение на экране во время capture (не должно ломать DXGI duplication)
```

### End-to-end

```text
Teacher → ScreenshotRequest → устройство offline → явная ошибка, не таймаут в вечность
Teacher → ScreenshotRequest → устройство online → кадр приходит < 2 сек на LAN
```

---

## 13. Acceptance criteria

1. Скриншот успешно захватывается и отображается для каждой протестированной GPU-комбинации из §12.
2. Многомониторная машина как минимум детектится корректно (все дисплеи перечислены), primary захватывается.
3. Полноэкранное DirectX-приложение (например Roblox/Unity) на экране ученика не ломает и не крашит capture pipeline.
4. Кадр нигде не сохраняется персистентно без явного debug-флага.
5. Ошибка capture доходит до Teacher Console как понятное сообщение, не зависание UI.

---

## 14. Что дальше

`T3_CONTINUOUS_STREAMING_SPEC.md` — превращение одиночного screenshot в grid thumbnails (1–2 FPS, много устройств) + selected device stream (8–15 FPS).
