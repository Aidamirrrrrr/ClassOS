# 0003 — DXGI Desktop Duplication как основной screen backend

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

Нужен захват экрана для thumbnail-грида (1–2 FPS, много ПК одновременно) и full-screen remote control (15–30 FPS, один ПК). Veyon и большинство конкурентов используют устаревшие GDI-based методы, которые дороже по CPU.

## Рассмотренные варианты

1. **GDI BitBlt** — прост, но дорог по CPU, не даёт dirty regions/cursor metadata, плохо масштабируется на много одновременных источников.
2. **Windows.Graphics.Capture** — современный API, хорош для захвата конкретных окон, но менее заточен под full-desktop duplication сценарий множества станций.
3. **DXGI Desktop Duplication API** — создан Microsoft именно под desktop collaboration / remote desktop сценарии, отдаёт GPU-surface, dirty rectangles, move rectangles, cursor state.

## Решение

DXGI Desktop Duplication — основной capture backend. `Windows.Graphics.Capture` остаётся резервным вариантом для захвата отдельных окон в будущем (`architecture/01_TECHNICAL_ARCHITECTURE.md` §18).

## Последствия

- Capture-код инкапсулируется за trait `ScreenCapture` (§17), чтобы backend можно было сменить/дополнить без переписывания вызывающего кода.
- Требуется тестирование на реальном железе (Intel/AMD/NVIDIA, 1080p/4K, один/два монитора) — VM-тесты недостаточны (§134).
- Оптимизация через dirty regions сознательно отложена после MVP (§23) — на T2 достаточно full-frame encode.
