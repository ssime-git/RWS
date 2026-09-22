# Durable macOS RWS Installation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Install RWS, SSHFS, configuration, mount verification state, and login remounting in durable macOS user locations.

**Architecture:** A new installer module owns canonical macOS paths and staged, versioned artifact installation. `Config` gains an optional mount-state generation so the atomic config replacement selects both SSHFS release and verification receipts. A generic login agent invokes `autostart run`, which reads the canonical registry and mounts every workspace.

**Tech Stack:** Rust, clap, serde JSON, macOS LaunchAgents, existing RWS lifecycle/mount code.

---

## File map

- `src/installation.rs`: canonical layout, injectable layout provider, staging, validation, atomic installation, and validated receipt migration.
- `src/config.rs`: optional mount-state generation and receipt directory selection.
- `src/lifecycle.rs`: read/write receipts through the selected generation while preserving legacy config behavior.
- `src/autostart.rs`: generic plist and conservative legacy-agent cleanup helpers.
- `src/main.rs`: `install`, `autostart run`, managed integration selection, aggregate mount execution.
- `src/lib.rs`: export installation module.
- `tests/cli.rs`: public CLI behavior using temporary directories.
- `docs/install.md`: durable macOS setup, refresh commands, and constraints.

## Chunk 1: Canonical state and safe installer

### Task 1: Add mount-state generation routing

**Files:**
- Modify: `src/config.rs`
- Modify: `src/lifecycle.rs`
- Test: unit tests in both files

- [ ] **Step 1: Write failing tests** for a legacy config retaining its adjacent receipt directory and a config with `mount_state_generation` selecting `mount-state/<id>/`.
- [ ] **Step 2: Run** `cargo test --locked lifecycle::tests config::tests` and confirm the new assertions fail because the selector does not exist.
- [ ] **Step 3: Implement** an optional validated generation field and a single receipt-directory resolver used by record, verified, and forget.
- [ ] **Step 4: Run** the targeted tests and confirm they pass.
- [ ] **Step 5: Commit** `git commit -am "Version RWS mount verification state"`.

### Task 2: Stage and activate a durable installation

**Files:**
- Create: `src/installation.rs`
- Modify: `src/lib.rs`
- Test: unit tests in `src/installation.rs`

- [ ] **Step 1: Write failing tests** covering canonical Application Support paths, source preservation, copied SSHFS path rewrite, receipt copying to a new generation, executable permissions, missing source failure, and a staged-copy failure preserving active config.
- [ ] **Step 2: Run** `cargo test --locked installation::tests` and confirm failure due to the missing module.
- [ ] **Step 3: Implement** `Layout`, an injectable layout provider for tests, source validation, exclusive staging directory, release ID generation, private copy helpers, and config-last activation. Stage all files before replacing `bin/rws` then atomically replace config last; retain referenced release and bounded backup. Parse each source receipt and migrate it only when its host, remote root, and mount root match a validated workspace.
- [ ] **Step 4: Run** `cargo test --locked installation::tests` and confirm pass.
- [ ] **Step 5: Commit** `git add src/installation.rs src/lib.rs src/config.rs src/lifecycle.rs && git commit -m "Install RWS in durable macOS state"`.

## Chunk 2: Generic login remounting

### Task 3: Replace per-workspace plist generation

**Files:**
- Modify: `src/autostart.rs`
- Test: unit tests in `src/autostart.rs`

- [ ] **Step 1: Write failing tests** asserting one `io.rws.mounts` plist has only the managed binary and `autostart run`, contains no config/workspace values, and only recognizes exact legacy RWS-owned plist names.
- [ ] **Step 2: Run** `cargo test --locked autostart::tests` and confirm failure against the current per-workspace output.
- [ ] **Step 3: Implement** generic plist generation and filesystem cleanup limited to exact legacy labels. Keep launchctl execution outside pure rendering helpers and report unload failures without deleting unrelated files.
- [ ] **Step 4: Run** targeted tests and confirm pass.
- [ ] **Step 5: Commit** `git add src/autostart.rs && git commit -m "Use one generic RWS mount agent"`.

### Task 4: Add public install and autostart-run commands

**Files:**
- Modify: `src/main.rs`
- Modify: `tests/cli.rs`

- [ ] **Step 1: Write failing CLI tests** using a subprocess-local canonical-layout override for `--config SOURCE install`, missing source rejection, installed-path `RWS_SSHFS` override rejection, `autostart install` rejecting an absent installed binary and embedding only the installed binary/default config, and `autostart run` rejecting a supplied source config while trying every canonical workspace then returning nonzero if one fails.
- [ ] **Step 2: Run** the exact new CLI tests and confirm they fail because the subcommands are absent.
- [ ] **Step 3: Implement** macOS-only `install`, default-config-only `autostart run` using `Config::load_existing`, and aggregate mount invocation reusing normal mount logic. `autostart install` resolves only the installed `Layout::binary()` and canonical config, rejecting any missing/incomplete durable installation even when invoked from `target/debug`. Ensure an already verified mount is success and no mount is silently skipped. Centralize the installed-binary `RWS_SSHFS` rejection in the SSHFS selector so mount, repair, and doctor all enforce it.
- [ ] **Step 4: Run** new CLI tests and confirm pass.
- [ ] **Step 5: Commit** `git add src/main.rs tests/cli.rs && git commit -m "Run durable RWS mounts at login"`.

## Chunk 3: Integrations, documentation, and verification

### Task 5: Point integrations to installed RWS only after installation

**Files:**
- Modify: `src/main.rs`
- Test: relevant unit/CLI tests

- [ ] **Step 1: Write failing tests** showing install reports refresh commands and that managed integration selection rejects a stale `RWS_SSHFS` override.
- [ ] **Step 2: Run** targeted tests and observe red.
- [ ] **Step 3: Implement** explicit refresh guidance; do not silently edit zsh or Delta configuration.
- [ ] **Step 4: Run** targeted tests and confirm green.
- [ ] **Step 5: Commit** `git commit -am "Guide refresh of durable RWS integrations"`.

### Task 6: Document and verify

**Files:**
- Modify: `docs/install.md`
- Modify: `.agents/skills/rws-macos-setup/SKILL.md` if its durable-path instructions need alignment

- [ ] **Step 1: Document** the canonical layout, migration command, generic LaunchAgent, retry behavior, macFUSE GUI authorization constraint, and manual hook/Delta refresh.
- [ ] **Step 2: Run** `cargo fmt --check`.
- [ ] **Step 3: Run** `cargo clippy --locked --all-targets -- -D warnings`.
- [ ] **Step 4: Run** `cargo test --locked`; if the existing sandbox process-enumeration test fails, report it separately and run the remaining suite with the exact named test skipped.
- [ ] **Step 5: Run** `git diff --check` and inspect `git status --short`.
- [ ] **Step 6: Commit** `git add docs .agents && git commit -m "Document durable macOS RWS setup"`.
