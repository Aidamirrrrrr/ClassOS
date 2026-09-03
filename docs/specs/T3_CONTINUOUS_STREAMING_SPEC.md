# ClassOS — T3 Implementation Specification

**Файл:** `docs/specs/T3_CONTINUOUS_STREAMING_SPEC.md`
**Статус:** Spec-ready
**Milestone:** T3
**Предпосылка:** T2 завершён (одиночный screenshot работает end-to-end)

---

## 1. Цель T3

Превратить one-off screenshot (T2) в continuous streaming с двумя режимами, как описано в `01_TECHNICAL_ARCHITECTURE.md` §20–21:

- **Thumbnail mode** (grid из многих устройств одновременно): 1–2 FPS, ≤640×360, JPEG/WebP.
- **Selected mode** (один открытый ученик): 8–15 FPS на T3 (15–30 FPS — цель для v1, не hard-требование T3), выше качество.

---

## 2. Definition of Done

```text
Teacher Console открывает Room с 4–10 устройствами
↓
видит live thumbnail grid всех устройств одновременно (~1 FPS)
↓
кликает на одно устройство
↓
переходит в fullscreen live view с более высоким FPS/качеством
↓
закрывает fullscreen
↓
устройство возвращается в thumbnail-режим, остальные продолжали работать всё это время без сбоев
```

Нагрузочная проверка: 10–20 устройств одновременно в grid не создают заметной деградации на обычной school LAN/Wi-Fi (`01_TECHNICAL_ARCHITECTURE.md` §138).

---

## 3. Non-goals

```text
remote input (T4)
H.264/аппаратное кодирование (остаётся JPEG, см. §7)
dirty-region partial updates (явно отложено, см. §23 арх. RFC)
адаптивный битрейт по качеству сети (будущее v1)
```

---

## 4. Ключевое архитектурное решение T3: adaptive scheduling

Teacher Console **обязан** сообщать Agent видимость/выбранность устройства (`01_TECHNICAL_ARCHITECTURE.md` §94):

```text
Visible   — вкладка classroom открыта, устройство в grid
Hidden    — вкладка/приложение свёрнуто
Selected  — открыт fullscreen именно этого устройства
```

Agent не отправляет кадры вслепую с фиксированным FPS всем подряд — расход ресурсов управляется явным запросом от Teacher Console. Если classroom tab скрыт — thumbnail rate должен снижаться, а не продолжать слать кадры в никуда.

### 4.1 Новые сообщения (`classos_network.proto`)

```protobuf
enum StreamMode {
  STREAM_MODE_UNSPECIFIED = 0;
  STREAM_MODE_THUMBNAIL = 1;
  STREAM_MODE_SELECTED = 2;
}

message StreamSubscribe {
  string device_id = 1;
  StreamMode mode = 2;
  uint32 target_fps = 3;
  uint32 max_width = 4;
}

message StreamUnsubscribe {
  string device_id = 1;
}

// ScreenFrame из T2 переиспользуется, плюс:
message ScreenFrame {
  // ... поля из T2 ...
  StreamMode mode = 8;
  uint32 sequence = 9;
}
```

`target_fps`/`max_width` — предложение Teacher Console, Agent вправе скорректировать вниз (например при высокой CPU-нагрузке ученической машины) — не hard contract, а negotiation hint.

---

## 5. Приоритизация каналов

Экран не должен забивать control-канал (`01_TECHNICAL_ARCHITECTURE.md` §34):

```text
P0  Control (enrollment/heartbeat/будущие commands)
P1  (зарезервировано под Remote Input, T4)
P2  Health/Events (зарезервировано под T7)
P3  Selected Screen
P4  Thumbnail Screen
```

Для T3 конкретно: если транспорт (TLS/TCP на T1) не даёт полноценной multiplexing-приоритизации из коробки — реализовать логическую очередь на уровне приложения так, чтобы control-сообщения (например будущий `Lock`) не стояли в очереди позади потока thumbnail-кадров. Если это существенно поднимает сложность T3 — зафиксировать как явный технический долг с ADR, а не молча игнорировать.

---

## 6. Encoding (без изменений от T2)

Остаётся JPEG (ADR не требуется — это продолжение решения из T2/§22 арх. RFC). Единственное дополнение: разные target-качества для thumbnail vs selected режима (ниже quality/resolution для thumbnail ради экономии трафика).

---

## 7. Teacher Console frontend pipeline

Явный архитектурный запрет (`01_TECHNICAL_ARCHITECTURE.md` §93):

> нельзя гонять 20 JPEG через JSON bridge как base64.

```text
network frame (Rust/Tauri backend)
↓
decode
↓
native/shared buffer
↓
UI rendering (React) через нативный канал Tauri, не через сериализацию в JSON/base64
```

---

## 8. Partial failure UX

При bulk-подписке на grid из N устройств часть может не ответить/быть offline. Grid обязан показывать частичный успех по паттерну §97 арх. RFC (уже применяется к командам, здесь — к streaming):

```text
8 / 10 streaming
PC-04 offline
PC-09 stream start failed
```

Не показывать grid как «всё ок», если часть устройств не стримит.

---

## 9. Security / Privacy (наследуется от T2, дополняется)

- Правило прозрачности становится обязательным именно на T3: пока идёт continuous stream (thumbnail или selected) — это уже похоже на постоянное наблюдение, и хотя thumbnail-режим не требует того же индикатора, что и remote control (T4), Session Host обязан отслеживать факт активного стрима внутри своего состояния — это подготовка почвы для индикатора remote control в T4, а не что-то, что можно пропустить и реализовать «потом откуда-то с нуля».
- Кадры по-прежнему не персистентны (наследуется из T2 §9).
- `StreamUnsubscribe` обязателен при закрытии Teacher Console/потере соединения — Agent должен по таймауту сам прекращать стрим при отсутствии активной подписки (защита от «зависшего» стрима после краша Teacher Console).

---

## 10. Тесты

### Unit

```text
StreamSubscribe/Unsubscribe encode-decode
scheduling logic: Visible/Hidden/Selected → ожидаемый target FPS
priority queue: control message не блокируется потоком screen-сообщений (тест на модельной очереди)
```

### Integration / нагрузочные

```text
4 устройства одновременно в grid — стабильный FPS
10-20 устройств в grid — деградация в допустимых пределах, no crash
переключение grid → selected → grid несколько раз подряд без утечек памяти/handle
скрытие вкладки Teacher Console → thumbnail rate заметно падает (проверяется по факту принятых кадров в единицу времени)
потеря сети во время стрима → Unsubscribe/reconnect не оставляет "зависший" стрим на Agent
```

---

## 11. Acceptance criteria

1. Grid из 10+ устройств стримит одновременно без критической деградации на обычной school LAN.
2. Selected mode даёт заметно более высокий FPS/качество, чем thumbnail того же устройства.
3. Control-сообщения не блокируются потоком screen-кадров под нагрузкой.
4. Partial failure в bulk-подписке показывается явно, а не маскируется как полный успех.
5. Закрытие Teacher Console или падение сети не оставляет Agent в состоянии бесконечного стриминга в никуда.

---

## 12. Что дальше

`T4_REMOTE_CONTROL_SPEC.md` — mouse/keyboard через `SendInput`, поверх уже работающего selected-mode стрима.
