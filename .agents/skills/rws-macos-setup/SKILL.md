---
name: rws-macos-setup
description: Use when an agent needs to install, configure, validate, or troubleshoot the RWS prototype on a new Mac, including Rust, the built-in NFS bridge mount backend (no macFUSE), the legacy SSHFS/macFUSE/FSKit backend, and an authorized remote test workspace.
---

# RWS setup on a new Mac

To connect an already working Mac to a VM, use `rws-vm-onboarding` instead; this skill is for building and validating RWS itself on a new Mac.

Bring a fresh Mac to a verified remote-workspace workflow. Record separate results for build, SSH execution, filesystem mount, Finder edits, and unmount. Passing unit tests or `rws doctor` does not prove mounting works.

**Default mount backend: the built-in NFS bridge.** `rws connect` starts `rws nfs-bridge`, a localhost-only NFSv3 server that forwards to the remote host over the system `ssh -s HOST sftp`; macOS mounts it with its own NFS client. It needs no macFUSE, no SSHFS, no remote install and no sudo. Mount points must be user-owned (`~/RWS/NAME`), never under `/Volumes`. Read `docs/nfs-bridge.md`. macFUSE/SSHFS (sections 2b and 5) is a legacy backend: install it only when the user explicitly asks for it.

Read the checkout's `docs/prototype.md`, `docs/nfs-bridge.md` and `docs/validation.md` before starting. This skill is stored three directories below the repository root. Do not depend on another machine's `.rws-local`, temporary Rust installation, account names, SSH keys, or cached package downloads. The current corrected path has passed a real RWS mount/file-operation/unmount cycle on one Mac using an experimental SSHFS build. Full editor and recovery acceptance is still pending. Use the validation record, not assumptions of prior success.

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

Test the selected host with system OpenSSH and SFTP. Preserve its existing host-key policy and authentication; let the user handle first-connection trust or login prompts. Do not copy credentials or install a remote service. Tailscale is optional. The current command wrapper assumes a POSIX-compatible remote login shell. Mounts and the LaunchAgent use `BatchMode=yes`: the key must work without an interactive prompt (agent or unencrypted key, as the user prefers).

### 2a. NFS bridge (default)

Nothing else to install. Select it and keep mount points in the user's home:

```sh
./target/debug/rws --config .rws-local/setup.json settings --backend nfs
```

The remote server must be OpenSSH's `sftp-server` (atomic saves use `posix-rename@openssh.com`). `rws doctor` reports `sshfs` as not required. If a workspace was registered under `/Volumes`, `rws workspace relocate NAME` moves it to `~/RWS/NAME`; `connect` refuses `/Volumes` with this backend because macOS deletes such mount points at unmount and recreating them needs root.

### 2b. Legacy SSHFS backend (only on explicit request)

For this backend, obtain compatible macFUSE and SSHFS packages from https://macfuse.github.io/ and its official linked releases. These are **two separate installations**. Verify official checksums when supplied and package signing/notarization before installation. Follow installer prompts; user authentication and license acceptance may require handoff.

Check `sshfs --version` actually runs. A file present on PATH is insufficient: `Library not loaded: libfuse3.4.dylib` or missing MFMount indicates an incomplete/broken macFUSE installation. Package receipts help distinguish installed SSHFS from missing macFUSE. Resolve executable PATH differences without writing global shell configuration unnecessarily.

For FSKit, verify the installed macOS/package combination supports it. The prototype's `--fskit` route uses a direct child of `/Volumes`. Enable the macFUSE FSKit module through System Settings > General > Login Items & Extensions; exact labels vary with macOS. Kernel-extension approval and reducing startup security are not FSKit setup steps. Do not switch to the legacy kernel backend merely because FSKit activation fails.

For the diagnosed macOS 27/macFUSE 5.4/SSHFS 3.7.5 combination, follow `docs/sshfs-fskit.md`: run the pinned-source build script and select its output with `RWS_SSHFS`. The script verifies hashes, preserves source and license, and never replaces system SSHFS. It needs existing GLib development files; install missing dependencies only when needed. Record the selected binary in private setup notes.

## 3. Isolated workspace

Select a unique workspace name and an unused mount path (`~/RWS/NAME` for NFS). Use an empty, dedicated remote directory; with user authorization, `mktemp -d /tmp/rws-test.XXXXXX` on the remote host can provide one. Record its exact returned path. Some hosts clear `/tmp` at boot (Alpine's `bootmisc` does) and an OrbStack machine wakes on any SSH connection, so `orbctl stop` is not a network outage and a restarted machine loses that directory; for outage or reboot tests choose a directory under the remote home and a real network cut. Do not use a remote home directory or valuable project for write/failure tests.

Use the following example only after replacing the example host and path with the selected values. With the NFS backend, the mount point is a user-owned directory such as `~/RWS/setup-test`; with the legacy FSKit backend it is an unused `/Volumes/RWS-...` path.

```sh
./target/debug/rws --config .rws-local/setup.json workspace add setup-test \
  --ssh user@devbox --remote /tmp/rws-test.EXAMPLE --mount "$HOME/RWS/setup-test"
./target/debug/rws --config .rws-local/setup.json doctor --workspace setup-test
./target/debug/rws --config .rws-local/setup.json exec --workspace setup-test -- pwd
./target/debug/rws --config .rws-local/setup.json shell --workspace setup-test
```

The interactive shell runs remotely and uses the remote `$SHELL`; the terminal window remains local. Exit that session before issuing subsequent local commands. Verify literal arguments and a known nonzero exit status, not just connection success. In the interactive session, verify Ctrl+C interrupts a disposable foreground command and terminal resizing reaches the remote PTY; record either as unverified if the available tools cannot exercise it.

## 4. Mount acceptance

### NFS bridge

```sh
./target/debug/rws --config .rws-local/setup.json connect setup-test --dry-run
./target/debug/rws --config .rws-local/setup.json connect setup-test
mount | grep 127.0.0.1                      # 127.0.0.1:/setup-test on ~/RWS/setup-test (nfs)
open "$HOME/RWS/setup-test"
```

`--dry-run` must show `rws nfs-bridge USER@HOST:REMOTE MOUNT --export NAME`. Run the file checks below plus these bridge-specific ones, each verified independently over SSH:

- Remote atomic replace (`mv tmp file` on the host) is visible on the Mac within about two seconds.
- No `._*` file appears remotely after writing a file with extended attributes (`xattr -w`).
- Kill only the bridge's `ssh` child (`pgrep -P <bridge pid> ssh`), then read a never-read file: it must succeed without an application-visible error. Read uncached files, otherwise the kernel cache hides the test.
- Kill the bridge itself: operations fail within about 8 seconds and `umount -f` works without sudo; no `mount_nfs`/`umount` process remains.
- `rws disconnect` makes the bridge exit by itself; an external `diskutil unmount` with intent `connected` is remounted by the LaunchAgent within about 30 seconds.

A `connect` or `disconnect` blocked by `Resource busy` means files are open; never force it on the user's real workspaces. A failed `disconnect` still records the pause, so run `connect` again to restore the `connected` intent.

### Legacy FSKit backend

```sh
./target/debug/rws --config .rws-local/setup.json mount setup-test --fskit --dry-run
./target/debug/rws --config .rws-local/setup.json mount setup-test --fskit
open /Volumes/RWS-setup-test
```

macFUSE creates the FSKit mount point; do not create it with sudo or relax `/Volumes` permissions. RWS starts foreground-mode SSHFS in a separate process group and returns on readiness. Startup errors or a 30-second polling timeout identify a private log beside the config. SSH authentication and host trust must already work with BatchMode; mounting has no credential prompt. Confirm the actual mounted filesystem. SSHFS has been observed returning zero when mounting failed; RWS now checks the device boundary. `doctor` only checks prerequisite execution and optional SSH connectivity.

Create a uniquely named text file through the mount, read it independently using `rws exec`, test rename and replace of a disposable file, edit/save it in a local editor, and confirm the new contents remotely. Basic writes do not validate editors that save using atomic rename. If rename fails, reproduce with direct SSHFS on the same isolated directory to separate the RWS wrapper from the filesystem stack; record the failure rather than hiding it with copy/delete. Test a nested-directory command using an absolute path to the built RWS binary and an absolute `--config` path after changing directory. Verify remote changes become visible locally, recording observed delays. Keep failures separate from successes.

For the legacy FSKit route, use the `3.7.5-rws-fskit3` build (the NFS bridge composes names to NFC itself and needs no build): conversion mode requires valid UTF-8 NFC remote child names. Read the Unicode contract in `docs/sshfs-fskit.md`; `--raw-names` is an explicit opt-out, not a Finder compatibility fix. Test exclusive creation, read, rename, replacement and symlinks with accents and supplementary-plane emoji. Exercise NFC/NFD aliases in both directions: create, read through the other form, update length, stat through the original form, delete through the other form, and confirm absence. Compare saved bytes independently over SSH. Conversion mode disables metadata caches at the cost of additional remote requests.

Run `python3 scripts/test-mount-listing.py MOUNT` (for example `~/RWS/setup-test`) on the authorized mounted root; repeated root listings after create/delete must remain accurate. This test creates a unique disposable directory and retains it on failure. Earlier builds returned an empty root after the first enumeration despite successful remote mkdir.

### Finder settings and the left sidebar

Read `docs/finder-macos.md` for the complete procedure and symptom table. In Finder > Settings > Sidebar, inspect Connected servers, External disks and Hard disks under Locations; record prior states, including mixed checkboxes. With existing user authorization or approval for these preference changes, enable the requested categories. This can expose other volumes in the sidebar. These are display settings, not a reason to grant Full Disk Access or change FSKit activation.

With the NFS bridge, the mount is a folder in the home directory: drag `~/RWS/NAME` to Favorites. Moving from FSKit leaves favorites, editor recents and Delta projects pointing at `/Volumes/RWS-*`; tell the user to re-add them from `~/RWS/...`.

With the legacy FSKit backend: On the tested Mac, enabling the disk categories was insufficient. Go > Computer (⇧⌘C), select `RWS-WORKSPACE`, then File > Add to Sidebar placed the current volume under Locations with an eject button. If the sidebar is hidden, show it from View; expand Locations if collapsed. RWS does not configure Finder automatically and must not present the remote mount as local just to force appearance.

After remount, the entry may disappear or remain stale. If clicking it reports that the original item cannot be found, first open the actual mount path (`~/RWS/...`, or `/Volumes/RWS-...` with FSKit) with ⇧⌘G. If that works, remove only the stale sidebar shortcut through its context menu and add the current volume from Computer again. Removing a shortcut is different from ejecting a volume. Automatic sidebar persistence remains unresolved.

Validate by actually clicking the sidebar entry, repeatedly leaving/reopening the root, creating a unique folder with ⇧⌘N, and copying a test file through Finder. Independently verify the directory and file bytes remotely. Finder has no generic New File command; use an editor Save dialog or copy/paste. Copy/paste success does not validate editor save patterns. Capture the resulting root view, not only the icon. If changes made outside Finder are missing, reopen the directory and record refresh behavior rather than claiming live notifications.

If a diagnostic crashes and OS I/O or unmount processes remain blocked, record them separately from the current mount's health. Stop accumulating probes on that volume. Do not automatically restart all FSKit services or reboot around the user's active work. The absence of a mount entry does not establish process cleanup.

Close test files, leave the mounted directory, and run `rws unmount setup-test`. Verify the volume is absent and the remote data still exists. Clean up only the exact disposable files/directory created during this run. For a continuing demo, explicitly record any retained mount or test directory.

## 5. FSKit activation failures (legacy backend only)

Hung FSKit mounts (many `/sbin/mount -t macfuse` processes that never exit, every SSHFS mount timing out) are a reason to propose the NFS bridge rather than repairing FSKit. Switching an existing durable setup, in this order: `rws install` with the new binary first (an older RWS rejects the `nfs` key and would break the LaunchAgent and zsh hook), then `rws settings --backend nfs`, `rws workspace relocate NAME` for each workspace, `rws connect NAME`. Uninstalling macFUSE afterwards needs the user's sudo: run its official `uninstall_macfuse.sh` from a copy (it deletes its own bundle), then remove the preference pane and `/usr/local/bin/sshfs`, which it leaves behind. Let the user type the password. The stuck processes disappear at the next restart.


If switches stay off, first verify registration of `io.macfuse.app.fsmodule.macfuse` and `io.macfuse.app.fsmodule.macfuse-local` with `pluginkit -m -A -D -i ID`. Registration is not enablement. Consult current upstream information, including https://github.com/macfuse/macfuse/issues/1194; it is a community report, not an Apple-supported repair procedure.

A read-only diagnostic is:

```sh
plutil -p "$HOME/Library/Group Containers/group.com.apple.fskit.settings/enabledModules.plist"
```

If access is denied despite correct file ownership/mode, investigate macOS privacy decisions. Do not prescribe chmod/chown or sudo as a privacy bypass. A confirmed Full Disk Access denial may require a **temporary, explicitly approved grant to the user's chosen terminal**. Describe the breadth of this permission and record its prior state. Respect automation tools that prohibit controlling that terminal; hand the command to the user rather than using another automation route.

If macFUSE entries are absent, an upstream workaround adds them and restarts `fskitd`. This is **not a validated automatic RWS setup step**. Before any such repair: obtain explicit authorization for the preference edit and service restart, verify the exact array and registered identifiers, preserve other entries, create a backup, consider active FSKit mounts, and provide rollback. Do not fetch and execute a community script blindly. Stop at a blocked permission or unsupported OS behavior with exact evidence.

Keep the agreed temporary grant active through the entire approved diagnosis/repair/verification sequence; do not revoke it between a read and the immediately following approved repair. If macOS requires a terminal restart for the grant to take effect, arrange it with the user before retrying. Run repair helpers as the logged-in user; scope sudo to a service restart rather than running the whole helper as root. Remove any grant added for troubleshooting after its final required step. macOS may retain Full Disk Access in a running terminal until it quits: an OFF switch alone does not prove effective revocation. Preserve the user's output and active work, then arrange and verify the required quit/reopen. If deferred, report the remaining grant lifetime explicitly. Never claim full setup success before real mount/edit/unmount checks pass.

## 6. Terminal and agent integration

Put the durable CLI on PATH first. Every later command, and every agent that
follows the installed rules, can then call `rws`. After `rws install` has
succeeded, run this on the Mac. Do not run it on the VM, and never point the
link at `target/` or at the app bundle:

```sh
mkdir -p "$HOME/.local/bin"
ln -sfn "$HOME/Library/Application Support/RWS/bin/rws" "$HOME/.local/bin/rws"
zsh -lic 'command -v rws && rws --version && rws agent-rules --help >/dev/null'
```

If an `rws` already exists there and is not a link to the managed copy, ask
before replacing it. If `~/.local/bin` is not on the login PATH, ask before
adding `export PATH="$HOME/.local/bin:$PATH"` to `~/.zshrc`. If
`agent-rules` is unknown, the durable copy is older than the build: run
`rws install` from the new build first. Record the link and the
`command -v rws` output.

After an active RWS configuration and a verified mount exist, offer the user's
interactive zsh integration. With authorization to edit the user's zsh startup
file, run the built RWS binary using its absolute path and bake in the active
private config:

```sh
/absolute/path/to/rws --config /absolute/path/to/setup.json hook install
```

The hook triggers inside the mount roots registered in the configuration when the shell starts, so it works with `~/RWS/NAME` (NFS) as with `/Volumes/RWS-NAME` (FSKit). After `workspace add` or `relocate`, only new terminals see the new root. To check it in a real terminal tab, run `RWS_AUTO_MODE=remote; cd ~/RWS/NAME`: the tab must print `RWS remote shell: HOST:REMOTE_DIR` and show the remote prompt. Piping input through `script(1)` does not exercise this reliably.

`hook install` manages only its marked `# RWS auto-shell hook` line in
`~/.zshrc` (or `$ZDOTDIR/.zshrc`); it preserves all other content and replaces
only its own prior line. Do not assume that building RWS, mounting a volume, or
installing the macOS app has installed this hook. Use a durable binary/config
location: the hook should not point at a disposable build or temporary setup
directory.

In a *new* interactive zsh (or after the user explicitly sources the startup
file), entering a verified mounted workspace by `cd` or starting the terminal
there must prompt once per workspace: `RWS: switch to <host> (<workspace>)?
[Y/n]`. Enter/Y opens `rws shell` at the mapped remote path; `n` leaves that
shell local. `exit` returns to the local zsh. `RWS_AUTO_MODE=remote` and
`RWS_AUTO_MODE=local` are deliberate automation overrides. Noninteractive
processes, non-zsh shells and IDE-internal subprocesses must remain untouched.
Do not claim this is a transparent process redirector.

Install the personal agent rules as an explicit setup step, with the same
absolute binary and config:

```sh
/absolute/path/to/rws --config /absolute/path/to/setup.json agent-rules --all
/absolute/path/to/rws --config /absolute/path/to/setup.json agent-rules --list
```

`--all` writes a block only for agents whose configuration directory exists
(Claude Code, Codex, pi, Cline, Devin CLI, Delta); `--list` shows each rules path
and whether the block is present. Record which agents and files were written.
The rules tell agents to ask Mac or VM once per workspace and to remember the
answer for 24 hours, stored outside the mount. To reset that choice before a
test, run `choice clear` and start a fresh agent turn; it should ask again:

```sh
/absolute/path/to/rws --config /absolute/path/to/setup.json choice clear --cwd /absolute/checkout
```

These rules guide agents to use `rws context`, `rws choice` and `rws exec`; they
do not redirect agent preparation, native Git operations, or arbitrary local
processes. Delta's interactive integrated terminal does load login zsh, so it
uses the zsh hook when its working directory is a verified mount. Mount the
workspace before adding its existing checkout to Delta. Validate an accepted
prompt in a real terminal with `uname -s`, `hostname` and `pwd`; if terminal UI
automation cannot safely exercise this, record it as unverified rather than
simulating it in a non-TTY shell.

## Completion record

Report: mount backend (NFS bridge or legacy SSHFS); versions; build/test result; SSH/shell result; mount/edit/unmount result;
`rws` PATH link (`command -v rws`); zsh-hook installation and real-terminal result; agent-rules installation (agents and
files written) and Delta-terminal result; the Mac/VM choice prompt in a fresh agent turn; screenshot locations; retained test data;
temporary-permission final state; unresolved OS blockers. Continue useful
independent steps, but label every unverified capability. Update the setup notes
when new evidence changes the procedure.
