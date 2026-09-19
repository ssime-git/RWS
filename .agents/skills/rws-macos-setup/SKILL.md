---
name: rws-macos-setup
description: Use when an agent needs to install, configure, validate, or troubleshoot the RWS prototype on a new Mac, including Rust, SSHFS/macFUSE, FSKit activation, and an authorized remote test workspace.
---

# RWS setup on a new Mac

Bring a fresh Mac to a verified remote-workspace workflow. Record separate results for build, SSH execution, filesystem mount, Finder edits, and unmount. Passing unit tests or `rws doctor` does not prove mounting works.

Read the checkout's `docs/prototype.md` and `docs/validation.md` before starting. This skill is stored three directories below the repository root. Do not depend on another machine's `.rws-local`, temporary Rust installation, account names, SSH keys, or cached package downloads. The current corrected path has passed a real RWS mount/file-operation/unmount cycle on one Mac using an experimental SSHFS build. Full editor and recovery acceptance is still pending. Use the validation record, not assumptions of prior success.

## Inputs and evidence

Obtain the checkout location, an authorized SSH alias/destination, and permission to create a disposable remote test directory. Ask missing questions one at a time while completing independent local checks. If cloning is necessary, use https://github.com/ssime-git/RWS or a verified fork supplied by the user.

Create `.rws-local/diagnostics/` for private setup notes and screenshot originals. Record OS/build, architecture, dependency versions, selected workspace, commands, outcomes, and any temporary grants that need revocation. Capture useful milestones: dependency installer success, FSKit module activation, mounted volume in Finder, a verified edited file, and final permission cleanup. Use permitted screenshot tooling; if unavailable, record that limitation. Original screenshots may contain account names and other apps: keep them ignored by Git. Only reviewed, anonymized images belong in public README assets. Never capture password entry.

## 1. Toolchain

Check `sw_vers`, `uname -m`, `xcode-select -p`, `git --version`, `cargo --version`, and `ssh -V`. Reuse working tools.

If developer tools are absent, start `xcode-select --install` and have the user complete macOS installation. If an unaccepted full-Xcode license blocks tools but `/Library/Developer/CommandLineTools` exists, test the command with `DEVELOPER_DIR=/Library/Developer/CommandLineTools`. This is a per-process selection; do not silently switch the system-wide developer directory or accept license terms for the user.

If Rust is absent, consult the current official Rust installation instructions at https://www.rust-lang.org/tools/install. Use the official stable toolchain and a persistent installation for the new Mac. Explain any shell-profile changes; a temporary toolchain is only a disclosed fallback. Verify Cargo works in a fresh shell.

From the checkout root:

```sh
cargo build --locked
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Install missing rustfmt/clippy components through rustup. No fixed historical test count is an acceptance criterion.

## 2. SSH and mount dependencies

Test the selected host with system OpenSSH and SFTP. Preserve its existing host-key policy and authentication; let the user handle first-connection trust or login prompts. Do not copy credentials or install a remote service. Tailscale is optional. The current command wrapper assumes a POSIX-compatible remote login shell.

For mounting, obtain compatible macFUSE and SSHFS packages from https://macfuse.github.io/ and its official linked releases. These are **two separate installations**. Verify official checksums when supplied and package signing/notarization before installation. Follow installer prompts; user authentication and license acceptance may require handoff.

Check `sshfs --version` actually runs. A file present on PATH is insufficient: `Library not loaded: libfuse3.4.dylib` or missing MFMount indicates an incomplete/broken macFUSE installation. Package receipts help distinguish installed SSHFS from missing macFUSE. Resolve executable PATH differences without writing global shell configuration unnecessarily.

For FSKit, verify the installed macOS/package combination supports it. The prototype's `--fskit` route uses a direct child of `/Volumes`. Enable the macFUSE FSKit module through System Settings > General > Login Items & Extensions; exact labels vary with macOS. Kernel-extension approval and reducing startup security are not FSKit setup steps. Do not switch to the legacy kernel backend merely because FSKit activation fails.

For the diagnosed macOS 27/macFUSE 5.4/SSHFS 3.7.5 combination, follow `docs/sshfs-fskit.md`: run the pinned-source build script and select its output with `RWS_SSHFS`. The script verifies hashes, preserves source and license, and never replaces system SSHFS. It needs existing GLib development files; install missing dependencies only when needed. Record the selected binary in private setup notes.

## 3. Isolated workspace

Select a unique workspace name and unused `/Volumes/RWS-...` path. Use an empty, dedicated remote directory; with user authorization, `mktemp -d /tmp/rws-test.XXXXXX` on the remote host can provide one. Record its exact returned path. Do not use a remote home directory or valuable project for write/failure tests.

Use the following example only after replacing the example host and path with the selected values:

```sh
./target/debug/rws --config .rws-local/setup.json workspace add setup-test \
  --ssh user@devbox --remote /tmp/rws-test.EXAMPLE --mount /Volumes/RWS-setup-test
./target/debug/rws --config .rws-local/setup.json doctor --workspace setup-test
./target/debug/rws --config .rws-local/setup.json exec --workspace setup-test -- pwd
./target/debug/rws --config .rws-local/setup.json shell --workspace setup-test
```

The interactive shell runs remotely and uses the remote `$SHELL`; the terminal window remains local. Exit that session before issuing subsequent local commands. Verify literal arguments and a known nonzero exit status, not just connection success. In the interactive session, verify Ctrl+C interrupts a disposable foreground command and terminal resizing reaches the remote PTY; record either as unverified if the available tools cannot exercise it.

## 4. Mount acceptance

```sh
./target/debug/rws --config .rws-local/setup.json mount setup-test --fskit --dry-run
./target/debug/rws --config .rws-local/setup.json mount setup-test --fskit
open /Volumes/RWS-setup-test
```

macFUSE creates the FSKit mount point; do not create it with sudo or relax `/Volumes` permissions. RWS starts foreground-mode SSHFS in a separate process group and returns on readiness. Startup errors or a 30-second polling timeout identify a private log beside the config. SSH authentication and host trust must already work with BatchMode; mounting has no credential prompt. Confirm the actual mounted filesystem. SSHFS has been observed returning zero when mounting failed; RWS now checks the device boundary. `doctor` only checks prerequisite execution and optional SSH connectivity.

Create a uniquely named text file through the mount, read it independently using `rws exec`, test rename and replace of a disposable file, edit/save it in a local editor, and confirm the new contents remotely. Basic writes do not validate editors that save using atomic rename. If rename fails, reproduce with direct SSHFS on the same isolated directory to separate the RWS wrapper from the filesystem stack; record the failure rather than hiding it with copy/delete. Test a nested-directory command using an absolute path to the built RWS binary and an absolute `--config` path after changing directory. Verify remote changes become visible locally, recording observed delays. Keep failures separate from successes.

For the current FSKit route, use the `3.7.5-rws-fskit2` build: conversion mode requires valid UTF-8 NFC remote child names. Read the Unicode contract in `docs/sshfs-fskit.md`; `--raw-names` is an explicit opt-out, not a Finder compatibility fix. Test exclusive creation, read, rename, replacement and symlinks with accents and supplementary-plane emoji. Exercise NFC/NFD aliases in both directions: create, read through the other form, update length, stat through the original form, delete through the other form, and confirm absence. Compare saved bytes independently over SSH. Conversion mode disables metadata caches at the cost of additional remote requests.

Verify Finder separately: the volume is labeled `RWS-WORKSPACE`. If absent from Locations, inspect Finder > Go > Computer. File > Add to Sidebar on the selected volume worked on the tested Mac, but its entry disappeared after remount. Record persistence separately and re-add the retained demo mount if needed. Obtain user approval before changing Finder disk-display preferences; do not classify the remote mount as local to force appearance.

Close test files, leave the mounted directory, and run `rws unmount setup-test`. Verify the volume is absent and the remote data still exists. Clean up only the exact disposable files/directory created during this run. For a continuing demo, explicitly record any retained mount or test directory.

## 5. FSKit activation failures

If switches stay off, first verify registration of `io.macfuse.app.fsmodule.macfuse` and `io.macfuse.app.fsmodule.macfuse-local` with `pluginkit -m -A -D -i ID`. Registration is not enablement. Consult current upstream information, including https://github.com/macfuse/macfuse/issues/1194; it is a community report, not an Apple-supported repair procedure.

A read-only diagnostic is:

```sh
plutil -p "$HOME/Library/Group Containers/group.com.apple.fskit.settings/enabledModules.plist"
```

If access is denied despite correct file ownership/mode, investigate macOS privacy decisions. Do not prescribe chmod/chown or sudo as a privacy bypass. A confirmed Full Disk Access denial may require a **temporary, explicitly approved grant to the user's chosen terminal**. Describe the breadth of this permission and record its prior state. Respect automation tools that prohibit controlling that terminal; hand the command to the user rather than using another automation route.

If macFUSE entries are absent, an upstream workaround adds them and restarts `fskitd`. This is **not a validated automatic RWS setup step**. Before any such repair: obtain explicit authorization for the preference edit and service restart, verify the exact array and registered identifiers, preserve other entries, create a backup, consider active FSKit mounts, and provide rollback. Do not fetch and execute a community script blindly. Stop at a blocked permission or unsupported OS behavior with exact evidence.

Keep the agreed temporary grant active through the entire approved diagnosis/repair/verification sequence; do not revoke it between a read and the immediately following approved repair. If macOS requires a terminal restart for the grant to take effect, arrange it with the user before retrying. Run repair helpers as the logged-in user; scope sudo to a service restart rather than running the whole helper as root. Remove any grant added for troubleshooting after its final required step. macOS may retain Full Disk Access in a running terminal until it quits: an OFF switch alone does not prove effective revocation. Preserve the user's output and active work, then arrange and verify the required quit/reopen. If deferred, report the remaining grant lifetime explicitly. Never claim full setup success before real mount/edit/unmount checks pass.

## Completion record

Report: versions; build/test result; SSH/shell result; mount/edit/unmount result; screenshot locations; retained test data; temporary-permission final state; unresolved OS blockers. Continue useful independent steps, but label every unverified capability. Update the setup notes when new evidence changes the procedure.
