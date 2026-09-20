# macOS CLI prototype implementation plan

> **Historical plan.** This records the initial CLI iteration, including its then-blocked
> mount checks. It is not the current installation procedure or feature status.
> Use [documentation](README.md), [features](../FEATURES.md) and [validation](validation.md).

**Goal:** register a workspace, mount it with SSHFS, and execute a command or shell on its owning SSH host.

**Architecture:** a Rust library holds workspace validation and path mapping; platform adapters build OpenSSH and SSHFS invocations; a small CLI owns configuration and user interaction. No daemon, remote installation, custom filesystem, or mobile code in this iteration.

**Tech stack:** Rust, clap, serde/serde_json, system OpenSSH and SSHFS. Remote command quoting assumes a POSIX-compatible login shell. The prototype uses synchronous child processes; interactive sessions replace the local process on Unix so OpenSSH owns terminal handling.

## Tasks

- [x] Add Cargo manifest and failing integration tests in `tests/core.rs` for workspace boundaries, nested paths, invalid destinations, and remote argv round trips through a real POSIX shell.
- [x] Implement `src/workspace.rs` and `src/transport.rs`; rerun `cargo test --test core`.
- [x] Add CLI tests in `tests/cli.rs` for isolated config registration, duplicate rejection, inspectable dry-run invocation, missing prerequisites, and command exit status.
- [x] Implement `src/config.rs` and `src/main.rs`: `workspace add/list`, `exec`, `shell`, `mount`, `unmount`, and `doctor`. Use `--config` to isolate tests; reject malformed data and ambiguous roots. Never create or modify SSH configuration.
- [x] Run formatting, tests, and lint; test doctor and an isolated local CLI example. Actual SSH checks passed against the user-selected host. Mount checks remain blocked by missing SSHFS/macFUSE; see docs/validation.md.
- [x] Document installation, limitations, manual remote acceptance checks, contribution workflow, and CI. Record exactly which end-to-end checks remain unverified.

## Verification details

Path tests cover sibling-prefix confusion, nested directories, traversal, and a symlink escaping the mount. Quoting tests pass spaces, quotes, newlines, Unicode, and shell metacharacters as literal argv, and preserve the remote directory. CLI tests use temporary configuration and subprocess adapters to inspect boundaries without connecting to real hosts. Remote tests must verify the remote working directory, remote exit status, and saved contents.

## Repository

Initialize a local Git repository on `prototype/cli` if Git works via the installed Command Line Tools. Do not publish or choose a license on the user's behalf. Keep build products, caches, real host configuration, and test data out of Git.

## Remaining acceptance gate

- [ ] Install SSHFS/macFUSE and validate mounting, Finder editing, saved contents, and unmount on the dedicated remote test workspace.
