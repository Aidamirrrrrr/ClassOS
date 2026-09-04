#!/usr/bin/env bash
# Статическая проверка Windows-кода с macOS/Linux хоста.
#
# Зачем отдельный скрипт: обычный `cargo check` на не-Windows хосте пропускает
# весь код под `cfg(windows)` (например crates/agent-service/src/runtime.rs) и
# поэтому не ловит ошибки компиляции в нём. Дополнительно `rustc` из PATH может
# оказаться сборкой из пакетного менеджера (Homebrew), которая не является
# rustup-toolchain: она игнорирует rust-toolchain.toml, не имеет
# x86_64-pc-windows-msvc std и падает с обманчивым "can't find crate for core"
# вместо настоящей ошибки. Поэтому toolchain выбирается явно.
set -euo pipefail

TARGET="x86_64-pc-windows-msvc"
TOOLCHAIN="$(sed -n 's/^channel[[:space:]]*=[[:space:]]*"\(.*\)"/\1/p' "$(dirname "$0")/../rust-toolchain.toml")"

if ! command -v rustup >/dev/null 2>&1; then
    echo "требуется rustup: https://rustup.rs" >&2
    exit 1
fi

TOOLCHAIN_BIN="$(rustup run "$TOOLCHAIN" rustc --print sysroot)/bin"
if [ ! -x "$TOOLCHAIN_BIN/cargo" ]; then
    echo "toolchain $TOOLCHAIN не установлен: rustup toolchain install $TOOLCHAIN" >&2
    exit 1
fi

if ! command -v cargo-xwin >/dev/null 2>&1; then
    # cargo-xwin поставляет Windows CRT/SDK, без которых не собираются
    # зависимости с C-кодом (ring в transport).
    echo "требуется cargo-xwin: cargo install cargo-xwin" >&2
    exit 1
fi

# Каталог toolchain идёт первым, чтобы cargo вызывал именно свой rustc.
export PATH="$TOOLCHAIN_BIN:$PATH"

echo "==> cargo xwin check --workspace --all-targets --target $TARGET"
cargo xwin check --workspace --all-targets --target "$TARGET"

echo "==> cargo xwin clippy --workspace --all-targets --target $TARGET"
cargo xwin clippy --workspace --all-targets --target "$TARGET" -- -D warnings

echo "==> Windows cross-check пройден"
