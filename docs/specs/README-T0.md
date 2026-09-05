# ClassOS T0 — служба и хост пользовательской сессии

Статус по `docs/specs/BACKLOG.md`: **реализация завершена; runtime-проверка выполнена только в CI.**
This document describes what has been built and validated *from a
non-Windows development machine*. Per spec §2 and §185, T0 can only be
declared complete after the full Definition-of-Done chain (install → reboot
→ login → heartbeat → crash → restart → logout → new user login) has been
verified on a real Windows 11 machine. That has **not** happened yet — see
"Known limitations" and "What remains before T1" below.

## Architecture implemented

```text
Windows Service (classos-service.exe service)
LocalSystem
      │
      │ Named Pipe, explicit ACL (SYSTEM + session user only)
      ▼
Session Host (classos-session.exe)
interactive user session
```

Crates (`Cargo.toml` workspace, edition 2024):

- `protocol` — generated protobuf types (`proto/local_ipc.proto`:
  `Envelope`, `SessionHello`, `ServiceHello`, `Ping`/`Pong`,
  `GetSessionInfo`/`SessionInfo`, `Shutdown`) plus `framing.rs`
  (`FramedReader`/`FramedWriter`: 4-byte little-endian length prefix +
  protobuf payload, 64 KiB max frame). Fully host-portable.
- `agent-core` — OS-independent business logic: `AgentError` (thiserror),
  minimal `AgentConfig`/device-id/instance-id helpers, `domain.rs`
  (`Session`/`ProcessSpec`/`ManagedProcess`), `traits.rs`
  (`SessionProvider`, `SessionProcessLauncher`,
  `LocalIpcServer`/`LocalIpcConnection`), `supervisor.rs`
  (`SessionSupervisor` desired-state reconciliation state machine with
  exponential backoff and crash-loop detection), and `mocks.rs`
  (`MockSessionProvider`, `MockProcessLauncher`). Fully host-portable and
  unit-tested (17 tests) without any Win32 dependency.
- `windows-platform` — all raw `unsafe` Win32 FFI, isolated per spec §88-90:
  `handles.rs` (`OwnedHandle`, `EnvironmentBlock` RAII wrappers),
  `sessions.rs` (`WTSGetActiveConsoleSessionId`, `ProcessIdToSessionId`),
  `process.rs` (`WTSQueryUserToken`, `CreateEnvironmentBlock`,
  `CreateProcessAsUserW`, liveness/termination), `security.rs`
  (dynamic SID lookup + explicit SDDL-based Named Pipe ACL, never a
  default descriptor), `pipes.rs` (pipe naming, raw
  `GetNamedPipeClientProcessId`). Windows-only; not buildable on this host.
- `agent-service` — `classos-service.exe`. Host-portable `lib.rs` (CLI
  parsing only); `#[cfg(windows)]` modules in the binary itself:
  `service.rs` (SCM integration via `windows-service`), `runtime.rs`
  (the async event loop wiring `SessionSupervisor` + IPC together),
  `windows_adapters.rs` (`WindowsSessionProvider`/`WindowsProcessLauncher`
  implementing agent-core's traits), `ipc.rs` (server-side Named Pipe
  connection via `tokio::net::windows::named_pipe`).
- `agent-session` — `classos-session.exe`. Host-portable `lib.rs` (CLI
  parsing only); `#[cfg(windows)]` `ipc_client.rs` (client-side Named
  Pipe connection) and `runtime.rs` (handshake + Ping/Pong/GetSessionInfo/
  Shutdown event loop).

## Development on non-Windows hosts

This was built and validated on macOS (arm64). The two-toolchain setup
used throughout:

- Default toolchain: Homebrew's `rustc`/`cargo` (`/opt/homebrew/bin/cargo`),
  targeting the host (`aarch64-apple-darwin`).
- A second, independent rustup-managed toolchain (`brew install rustup`,
  kept out of `PATH` by default) provides the
  `x86_64-pc-windows-msvc` target for type-checking Windows-only code:

  ```bash
  rustup toolchain install stable --profile minimal
  rustup target add x86_64-pc-windows-msvc
  rustup component add clippy   # for the msvc-target clippy commands below
  ```

**What builds/tests on the default host toolchain** (no `PATH` override):

```bash
cargo test --workspace --exclude windows-platform
cargo clippy --workspace --exclude windows-platform --all-targets -- -D warnings
cargo fmt --all -- --check
```

This covers `protocol`, `agent-core` (including all `SessionSupervisor`
state-machine and backoff-calculation unit tests, run against
`MockSessionProvider`/`MockProcessLauncher` — no real Win32 needed), and
the host-portable `lib.rs` halves of `agent-service`/`agent-session` (CLI
argument parsing). Both binaries' `main.rs` also compile and run on
non-Windows hosts, but only print "only runs on Windows" and exit 1
(`#[cfg(not(windows))]` stub) — this is intentional, not a bug.

**What only *type-checks*, never builds/runs/tests, on this host**
(`windows-platform`, and the `#[cfg(windows)]` real implementation modules
in `agent-service`/`agent-session`):

```bash
PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
  cargo check --workspace --target x86_64-pc-windows-msvc

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
  cargo clippy --workspace --target x86_64-pc-windows-msvc --all-targets -- -D warnings
```

There is no MSVC linker or Windows execution environment on this machine,
so `cargo build`/`cargo test`/running the binaries for the msvc target is
categorically impossible here. Every commit in this milestone's history
was validated with both command sets (host test suite green, msvc
check+clippy green, zero warnings) before being made.

**What actually builds and links into real Windows binaries on this host**
(beyond type-checking): [`cargo-xwin`](https://github.com/rust-cross/cargo-xwin)
cross-links against a downloaded Windows SDK/CRT using `rust-lld`, so real
PE32+ `.exe` files can be produced without an MSVC installation or a
Windows machine:

```bash
brew install llvm                       # provides clang-cl (xwin needs it present, even though linking uses rust-lld)
cargo install cargo-xwin
rustup component add llvm-tools-preview clippy   # rust-lld + clippy for the rustup toolchain

PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH" \
  cargo xwin build --target x86_64-pc-windows-msvc --workspace
PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH" \
  cargo xwin clippy --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings
```

This is strictly stronger than plain `cargo check`: it catches link-time
errors (missing symbols, ABI mismatches) that type-checking alone cannot.
It still cannot **run** anything — there is no Windows execution
environment on this machine, so the resulting `.exe` files have never been
launched, and none of the SCM/WTS/session/IPC runtime behavior has been
exercised. That gap is closed by testing on a real Windows VM/VPS (see
"What remains before T1" below).

**What has actually run and been observed on real Windows**: the
`service-smoke` job in `.github/workflows/rust-ci.yml` (`windows-latest`)
builds a release binary and runs `scripts/acceptance-t0.ps1`, which executes
the mechanical half of the T0 acceptance block for real: install
(`New-Service`, ACL, SCM failure recovery config), service reaching
`Running`, `SERVICE_RUNNING` present in the daily log, stop/start (the check
that caught the one real bug in this project's history), idle CPU budget, and
a clean purge on the way out.

**Correction (2026-09-05).** An earlier version of this paragraph claimed CI
confirmed `SESSION_DISCOVERED` / `SESSION_HOST_STARTED` / `IPC_HANDSHAKE_OK`
in the service log. It did not — the job only printed the log tail and
asserted nothing about its contents. The claim happened to be true in
substance, which is precisely what made it dangerous: a documented check that
nothing actually performed. The harness now asserts those three events, and
reports them as `NOT-RUN` — never as passed — on a machine with no console
session.

**First observed run (2026-09-05, CI run `33970228765`):** 13 checks passed,
0 failed, 9 `NOT-RUN`. The runner does have a console session, so the Session
Host really is launched there: killing it produced a replacement in **20.4 s**
(inside the 30 s requirement, outside the 10 s target noted in the checklist —
worth re-measuring on real hardware). Service handles moved 160 → 164 across
5 restart cycles; idle CPU was 0% over 60 s.

None of this is a substitute for the acceptance tests below — no reboot, no
real interactive login, no second user, no multi-hour run — but it is genuine
execution on real Windows, not just type-checking or cross-linking.

**What requires a persistent real Windows machine** (VPS/VM, not CI):
everything in "Known limitations" below, including the entire smoke-test
checklist (spec §159) and Acceptance Tests A–F (spec §109-114).

## Build

On Windows, with the `x86_64-pc-windows-msvc` target installed:

```powershell
rustup target add x86_64-pc-windows-msvc
cargo build --release
```

Produces `target\release\classos-service.exe` and
`target\release\classos-session.exe`.

## Local development run (spec §96)

Terminal A:

```powershell
cargo run -p agent-service -- run
```

This runs `classos-service.exe` as a normal (non-LocalSystem) foreground
process. In this mode `WindowsProcessLauncher` uses `LaunchMode::DevChildProcess`
(plain `std::process::Command`, not `CreateProcessAsUserW` — that API
requires a LocalSystem caller with `SE_TCB_NAME`, spec §13) and
automatically launches `classos-session.exe` with the correct
`--session-id`/`--pipe` arguments once it discovers the active console
session, end-to-end, including the Named Pipe handshake and heartbeat.

This is a deliberate deviation from the exact two-terminal manual
invocation shown in spec §96 (`cargo run -p agent-session -- --dev` in a
second terminal): rather than supporting a standalone `--dev` mode with no
service on the other end, `run` mode fully drives the supervisor loop
against a real (dev-privileged) child process, which is a more complete
end-to-end exercise of the same architecture. `classos-session.exe` always
requires `--session-id`/`--pipe`; there is no bare `--dev` flag.

The pipe's ACL still restricts access to the current user + SYSTEM (same
`PipeSecurityDescriptor` code path as `service` mode; spec §96's note
about dev-mode ACL being "only the current user" is naturally satisfied
since `WindowsSessionProvider`/`user_sid_for_session` resolve whatever
user is actually logged into the console session being tested).

## Service install / uninstall (spec §91-95)

```powershell
cargo build --release
.\scripts\install-service.ps1
Restart-Computer
```

After reboot and login:

```powershell
Get-Service ClassOSAgent   # expect: Running
.\scripts\status.ps1       # service status, running session hosts, log tail
```

Uninstall:

```powershell
.\scripts\uninstall-service.ps1          # keeps C:\ProgramData\ClassOS
.\scripts\uninstall-service.ps1 -Purge   # also removes logs/state/config
```

`install-service.ps1` also locks `C:\Program Files\ClassOS` down to
Read & Execute for standard users (spec §137-138) and configures SCM
failure recovery (restart at 5s / 15s / 60s, spec §95).

## Smoke-test procedure

See spec §159 for the full manual checklist (install, reboot, login,
correct user/session, handshake, heartbeat, lock/unlock, logout/login,
kill-and-restart, service restart, reboot, cross-user pipe ACL, CPU/handle
leak checks). **None of these have been executed** — see below.

## Known limitations

Per spec §175, plus limitations specific to how this milestone was built:

- Windows 11 x64 only targeted; not tested on Windows 10, Windows 11
  Education/Enterprise.
- Single physical console session only (no multi-session, no RDP support
  guarantee — spec §78 explicitly warns to verify an RDP admin session
  never hijacks the student Session Host; **this has not been verified**).
- No Teacher Console, no external network, no screen capture, no cloud
  auth, no auto-update, no policies, no AppLocker, no WinGet — all
  correctly out of scope for T0 (spec §3, §99-127).
- **This entire milestone was built and validated exclusively on macOS.**
  Every line touching Win32 (`windows-platform`, and the `#[cfg(windows)]`
  modules in `agent-service`/`agent-session`) has been type-checked via the
  `x86_64-pc-windows-msvc` target and, via `cargo-xwin`, actually compiled
  and linked into real PE32+ `classos-service.exe`/`classos-session.exe`
  binaries — but those binaries have never been **executed**. Nobody should
  treat T0 as "done" based on this work. Per spec §2/§185, the Definition
  of Done requires the full reboot → login → heartbeat → crash → restart →
  logout → new-user-login chain to work unattended on a real Windows 11
  machine, and none of that has been exercised.
- Service and Session Host logs use daily rotation with seven-file
  retention (`service.log.YYYY-MM-DD` and
  `session-{sessionId}.log.YYYY-MM-DD`, spec §79-81).
- SCM failure recovery for the *Service process itself* (Acceptance Test C,
  spec §111) is configured via `sc.exe failure` in the install script but
  has never been triggered/observed.
- **Found and fixed via CI, not by inspection**: the first `service-smoke`
  CI run installed and started the service successfully (confirmed
  `SESSION_DISCOVERED`/`SESSION_HOST_STARTED`/`IPC_HANDSHAKE_OK` in
  `service.log` on a real Windows runner), but `Stop-Service` then failed
  after ~2s. Root cause: `graceful_shutdown` reported no intermediate SCM
  status at all — it went straight from `Running` to `Stopped` after a
  fixed 3s sleep, violating spec §17's requirement that all four service
  states (including `STOP_PENDING`) be reported. Fixed by reporting
  `StopPending` immediately on `Stop`/`Shutdown` and bumping the
  checkpoint every second through the shutdown grace period. Re-run
  confirmed the full install → start → stop → uninstall cycle now
  succeeds. This is the kind of bug that `cargo check`/`cargo xwin`
  cannot catch — only real execution can — and is a concrete argument for
  getting a persistent Windows VPS/VM working rather than relying on CI
  alone.
- `windows-service`'s `define_windows_service!`/`service_control_handler`
  usage, the `SECURITY_ATTRIBUTES` lifetime handling around
  `create_with_security_attributes_raw`, and the handshake/heartbeat
  concurrency in `runtime.rs` are all new, non-trivial, and have zero
  runtime verification. Treat all of it as "should be correct by careful
  reading of the Win32/tokio/windows-service docs and Rust's borrow
  checker", not as "proven correct".
- After handshake the Service requests `SessionInfo`; the Session Host
  replies with its session, PID and username. Service shutdown now sends
  the protocol `Shutdown`, waits up to three seconds for a clean exit, and
  only then force-terminates the exact managed PID (spec §74). These paths
  compile for Windows but still require runtime verification on the
  persistent acceptance-test machine.
- Session lock/unlock (`WTS_SESSION_LOCK`/`UNLOCK`) events are consumed by
  the Service, stored as authoritative per-session state, and logged as
  `SESSION_LOCK_STATE_CHANGED`. The Session Host's initial `SessionInfo`
  still reports `is_locked: false`; the Service-owned state supersedes it
  after SCM notifications, as explicitly allowed by spec §77.
- The install/uninstall PowerShell scripts (§91-95) have been written
  carefully against the spec but never executed against a real SCM.

## What remains before T1 (spec §176 gate)

Per the gate: T1 cannot start until T0 passes, on real Windows hardware:
reboot, login/logout, host crash recovery, service restart, pipe ACL
cross-user isolation, and an 8-hour idle stability run. Concretely, before
this milestone can be marked `done` in `docs/specs/BACKLOG.md`:

1. Get a real Windows 11 VM/machine, install the two-toolchain-produced
   release build, and run through the full spec §159 smoke-test checklist.
2. Run Acceptance Tests A–F (spec §109-114) and fix whatever breaks —
   given the amount of unexercised code above, expect to find real bugs.
3. Verify daily log rotation/retention and the newly wired
   `GetSessionInfo`/`Shutdown`/lock-state paths during the extended run.
4. Only after all of the above: update `docs/specs/BACKLOG.md`'s T0 row
   from `not started`/`in progress` to `done`, and only then start
   `docs/specs/T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md`.
