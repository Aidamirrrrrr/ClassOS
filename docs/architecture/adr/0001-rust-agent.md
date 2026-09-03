# 0001 — Rust для Windows-агента

**Статус:** Accepted
**Дата:** 2026-09-03

## Контекст

Student Agent (Service + Session Host) — самая критичная по надёжности и безопасности часть системы: работает с высокими привилегиями (LocalSystem), делает Win32/DXGI syscalls, должен пережить reboot/crash/network failure без ручного вмешательства (`architecture/01_TECHNICAL_ARCHITECTURE.md` §170).

## Рассмотренные варианты

1. **C++** — прямой доступ к Win32/DXGI, огромная экосистема, но ручное управление памятью и handle'ами повышает риск security-багов в LocalSystem-компоненте.
2. **C#/.NET** — быстрая разработка, хорошие Windows bindings, но рантайм/GC overhead нежелателен для процесса, который должен быть незаметен во время урока (CPU/RAM budget, см. `specs/T0_*` §115–116).
3. **Rust** — memory safety без GC, зрелые Win32-биндинги (`windows` crate), маленький footprint, нативная производительность, код частично переиспользуем в Tauri-based Teacher Console.

## Решение

Rust, с `windows-rs` для Win32/DXGI и Tokio для async runtime.

## Последствия

- Весь unsafe Win32 FFI обязан быть изолирован в крейте `windows-platform` (см. ADR не требуется на уровне модуля — это уже зафиксировано в §148 архитектурного RFC).
- Порог входа для найма выше, чем C#: первый серьёзный найм — Senior Windows Systems Engineer со знанием Rust или готовностью его выучить (`product/02_MARKET_AND_INVESTMENT_ANALYSIS.md` §57).
- Teacher Console (Tauri) может переиспользовать `protocol`-крейт напрямую — не нужен второй независимый клиент протокола.
