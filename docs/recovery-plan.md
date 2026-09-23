# Reliable setup and recovery — 2026-09-23

Goal: finish the authorized repair of setup drift and unbounded recovery, and
provide actionable diagnosis before any repair. Existing user configuration,
remote files and explicit integration opt-outs must survive upgrades.

## Design

Use a shared bounded subprocess runner for maintenance commands. A timeout must
return an error even if a child is stuck in kernel I/O; it must not imply that
the OS released resources. Never continue to remount after failed ejection.

`doctor` checks dependencies, mount identity and responsiveness independently,
SSH access, durable installation and managed integration references. It emits a
structured report and optional private JSON file. `doctor --repair` and `repair`
share orchestration: record before, refresh stale managed integrations, connect
missing workspaces or repair verified unresponsive ones only when requested,
then record after. Unknown mounts are never adopted implicitly. FSKit service
restart remains an explicitly authorized manual operation affecting other volumes.

Installation and app relaunch reconcile managed integration references to the
durable binary/configuration. New integrations require the existing app preference
or explicit CLI opt-in; absent integrations are not silently enabled. Existing
custom configuration choices remain explicit. LaunchAgent captures maintenance
output in private logs. No user-specific host/path is embedded in product code.

## Implementation and test sequence

- [x] Bounded maintenance subprocess runner: exit codes, spawn errors, timeout,
  child/descendant cleanup, no blocking wait after timeout (`src/process.rs`).
- [x] Wire bounded execution into connect repair, unmount, dependency checks and
  SSH probes (`src/main.rs`). Real wedged-FSKit recovery remains unverified.
- [x] Managed integration audit/reconciliation, idempotence and preservation of
  user text; integrate durable install and app startup. Test stale development
  paths, absent rules, custom config and repeated setup.
- [x] Doctor report and repair orchestration: JSON and text, private report,
  health vs identity, refused unknown mounts, failures remain visible after repair.
- [x] LaunchAgent diagnostic logs and documentation.
- [ ] Review, format, lint and relevant Rust/Swift tests. Install updated durable
  binary with permission; real macOS mount recovery and Delta test are separate
  acceptance checks, never inferred from unit tests or identity-only status.

## Observed evidence

The terminal hook used the development configuration while the mounts belonged
to the durable installation. Separately, an ordinary read from one workspace
blocked while direct SSH read of the same AGENTS.md succeeded. `connect --repair`
waited on diskutil through an unbounded `Command::status()`. Delta's last send
was accepted and stalled during context preparation. These observations do not
establish the original cause of the FSKit deadlock or guarantee Delta compatibility.
