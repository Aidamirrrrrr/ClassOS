# ClassOS T0 — Service / Session Host

Status per `docs/specs/BACKLOG.md`: **Impl: in progress, not `done`.**
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
  unit-tested (16 tests) without any Win32 dependency.
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

**What requires a real Windows machine** (or the `windows-latest` CI job,
see `.github/workflows/rust-ci.yml`): everything in "Known limitations"
below, including the entire smoke-test checklist (spec §159) and
Acceptance Tests A–F (spec §109-114).

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
- Log rotation (spec §81) is not implemented — `service.log` is opened in
  append mode and grows unbounded within a single install. A simple
  size/daily rotation is needed before any extended real-world run.
- SCM failure recovery for the *Service process itself* (Acceptance Test C,
  spec §111) is configured via `sc.exe failure` in the install script but
  has never been triggered/observed.
- `windows-service`'s `define_windows_service!`/`service_control_handler`
  usage, the `SECURITY_ATTRIBUTES` lifetime handling around
  `create_with_security_attributes_raw`, and the handshake/heartbeat
  concurrency in `runtime.rs` are all new, non-trivial, and have zero
  runtime verification. Treat all of it as "should be correct by careful
  reading of the Win32/tokio/windows-service docs and Rust's borrow
  checker", not as "proven correct".
- `GetSessionInfo`/`SessionInfo`/`Shutdown` messages are implemented on
  both ends per the protocol schema, but the T0 `runtime.rs` service-side
  event loop never actually sends `GetSessionInfo` or `Shutdown` to a
  connected Session Host — only the Ping/Pong heartbeat path is exercised
  end-to-end by the current wiring. The message types and Session Host
  handling exist and are protocol-correct, but this particular
  request/response round-trip and the graceful-shutdown-via-IPC flow
  (spec §74) are unexercised. `graceful_shutdown` in `runtime.rs`
  currently terminates the tracked process directly rather than sending a
  `Shutdown` envelope and waiting; this is a simplification, not the
  spec's exact described flow.
- Session lock/unlock (`WTS_SESSION_LOCK`/`UNLOCK`) events are forwarded
  from the SCM into `ServiceEvent`, but nothing currently consumes them to
  update `SessionInfo.is_locked` — the Session Host always reports
  `is_locked: false`. Spec §76-77 scope for T0 is satisfied structurally
  (the field and the event plumbing exist) but the actual state relay is
  not wired up.
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
3. Implement log rotation (spec §81) before doing any multi-hour run.
4. Decide (or explicitly defer with a documented reason) whether to wire
   up the currently-unused `GetSessionInfo`/`Shutdown` request/response
   flow and lock/unlock state relay, or leave them as protocol-complete
   but functionally inert for T0 sign-off purposes.
5. Only after all of the above: update `docs/specs/BACKLOG.md`'s T0 row
   from `not started`/`in progress` to `done`, and only then start
   `docs/specs/T1_NETWORK_AND_DEVICE_DISCOVERY_SPEC.md`.
