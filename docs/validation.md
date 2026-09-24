# Prototype validation — 2026-09-19

[Documentation](README.md) · [Architecture](architecture.md) · [Feature backlog](../FEATURES.md)

## Reading this journal

This is an append-only development record: earlier failures and test counts describe
their stage, not necessarily the current build. Later follow-ups supersede them.

As of 20 September 2026, recorded checks cover file operations with patched SSHFS,
explicit remote execution, a Delta forwarding diagnostic, remote agent process
startup and Finder favorite clicks after remount. User feedback additionally confirms
the Delta workflow and app-launched Claude. These are distinct observations, not a
claim that every app or agent scenario passed end to end.

Clean-Mac setup, full editor-save coverage, network-loss recovery and signed
release-to-release updates remain unvalidated. Ordinary terminals in a mount
still execute locally. Follow [installation](install.md) for current instructions.

## Initial filesystem result

Latest result: the [FSKit corrections](sshfs-fskit.md) are implemented and a real RWS file-operation cycle passes using the isolated patched SSHFS build. Earlier failure records below are preserved as diagnostic history; full editor and recovery acceptance remain outstanding.

## Simplified connection follow-up

`connect`/`disconnect`, remembered backend and SSHFS path, separate mount/SSH
status, and generated Finder shortcuts are implemented. The current automated
suite passes on the tested Mac: 34 tests, formatting and Clippy with warnings
denied. This is a local result, not a new Linux CI result.

Two runs on a dedicated temporary workspace verified repeated connect, a Unicode
file whose SHA-256 matched an independent remote read, Linux execution in the
mapped nested directory, and preservation of remote exit code 37. A second
configuration aimed at the same volume but a different remote directory failed
the random challenge check and acquired no ownership receipt. Challenge cleanup
was confirmed. A process holding an open file/current directory caused normal
unmount to fail with Resource busy; after it exited, normal and repeated
disconnect passed. Remote bytes remained intact after disconnect, then the
disposable directory and its AppleDouble metadata were explicitly cleaned up.

The user's existing Documents volume was verified and adopted without an
unmount. Generated Connect and Status scripts passed; the Shell-VM script opened
a remote PTY where `uname -s` returned Linux and `pwd` the remote workspace root.
GUI double-click launch was not separately exercised. The Documents mount remains
active. Original config, shortcuts, binary, logs and exact paths remain private
in `.rws-local/`.

Delta's own command execution remains local according to its documented model;
no supported SSH agent backend was found. Its active worktree and Python
environment were not migrated. See [connection and execution](connection.md).
This work does not validate editor saves, network-loss recovery or automatic
Finder sidebar persistence. Mount verification now requires a writable root.

## Intermediate acceptance snapshot

Global Delta forwarding follow-up: the current suite passes 46 tests locally,
with formatting and Clippy. Personal rule generation is generic across the
registered workspaces. Live tests on two disposable projects under the existing
mount passed Linux Python execution, correct nested CWD, Git status, and translation
of a Delta-format Mac-local Git remote URL. One project used an accented directory;
the other simulated Delta's cloned Git metadata. The same wrapper also read the
active Delta worktree's Git root, status and translated local remote successfully,
without rewriting its metadata. Both disposable projects were removed afterward.

The private installed binary and Delta personal rule file were verified. The rule
checks routing before commands, forwards remote workloads with a command-scoped
Git environment, and allows ordinary local projects. Missing registries, escaped
paths and unavailable mounts fail instead of authorizing local fallback. Tests
also cover Linux-created Git metadata and selection of another registered host.

Delta was not running/accessible through the UI tool at verification time. Thus
rule installation and the relay are validated, but a Delta agent turn obeying
the installed rule remains unverified. Its internal automatic setup/native Git
processes are not intercepted. See [global rules and limits](connection.md#install-one-rule-for-every-registered-project).

This table supersedes unresolved statements in the historical investigation sections below. The current required experimental binary is `3.7.5-rws-fskit3`.

| Workflow | Recorded result |
| --- | --- |
| Rust build, formatting, Clippy, automated tests | Passed locally; 46 tests after global forwarding follow-up |
| SSH exec, quoted arguments, remote working directory, interactive login shell | Passed on one authorized Linux host |
| Mount readiness and ordinary unmount | Passed; retained demo mount intentionally left active |
| Unicode I/O and repeated root/child listing | Passed with fskit3; live regression failed before correction |
| Finder sidebar click, New Folder, file copy/paste | Passed after replacing the stale shortcut; remote existence/content confirmed |
| Local editor modification/save/reopen | Pending |
| Automatic sidebar entry persistence after remount | Unresolved; manual replacement procedure documented |
| Network-loss/crash recovery and remote-change notifications | Not validated; intermediate diagnostic left two OS processes blocked |
| Second host and fresh physical Mac | Pending |
| Screenshots | Private originals retained; no sanitized README images published |

See [Finder setup](finder-macos.md) for operational steps and [the roadmap](../ROADMAP.md#2-reliable-everyday-macos-use--p1) for the next acceptance work. Setup permissions used during the initial FSKit repair were revoked; later Finder work needed no new privacy grant.

## Verified locally

- Rust 1.98.1 build and test execution on Apple Silicon macOS using installed Command Line Tools.
- 21 automated tests passing: 4 lifecycle tests, 3 core tests and 14 CLI tests.
- `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings` passing.
- Independent code review identified mount-root alias overlap; a failing regression test reproduced it and the fix now resolves existing ancestors before comparing roots.
- Missing SSHFS produces a clear error without creating the mount directory.
- Repository published at https://github.com/ssime-git/RWS with `prototype/cli` as the initial default branch.

## Verified on an authorized Linux SSH host

- OpenSSH and SFTP connection.
- Actual `rws exec` prints the expected remote working directory.
- Spaces, quotes, Unicode, and `$(id)` are passed as literal arguments.
- Remote exit code 37 is preserved by the CLI.
- `rws shell` opens a real remote PTY; `test -t 0` succeeds and normal exit closes the connection.
- A file written and read through a remote command in a unique temporary directory has expected contents. The test file and directory were removed afterward.

These checks validate remote execution, not editing through the filesystem mount.

## Not yet verified

- Full editor save workflows and concurrent remote-edit cache behavior. Basic Finder browsing, rename, Unicode filenames and the mount lifecycle passed the follow-ups below.
- PTY resize and Ctrl+C under a long-running job, interrupted remote writes, and network-loss recovery.
- Coding-agent launch: no agent was installed or authenticated as part of these checks.
- Second host, transfer behavior, and performance targets from v0.1.
- A complete setup on a second physical Mac; CI does not exercise an actual macFUSE volume.

## Local tooling note

The initial build used a temporary Rust installation without changing the user's shell profile. A normal Rust installation is needed for future builds after temporary files are removed. The compiled `target/debug/rws` can be run directly now.

On this machine, setting `DEVELOPER_DIR=/Library/Developer/CommandLineTools` permits use of the installed compiler and Git without changing global Xcode settings or accepting a license on the user's behalf.

## Default-shell correction

The initial implementation forced `sh -l`. RWS now starts the remote `$SHELL` as a login shell. A regression test verifies shell selection and workspace positioning with quoted paths. On the authorized Linux host, the new session displays the configured Bash prompt and reports Bash running as a login shell.

## Incomplete dependency installation

The installed SSHFS binary failed to start because `/usr/local/lib/libfuse3.4.dylib` was missing; only the SSHFS package receipt was present. A regression test now covers this installed-but-unusable state in both doctor and mount. The official macFUSE installer was downloaded, its SHA-256 matched release metadata, and its notarized signature was verified. Installation was opened for the user to complete; mount validation remains pending.

## FSKit mount attempt

After installing macFUSE, both modules were registered. A mount of a dedicated remote temporary directory failed with `File system extension not enabled`; the GUI switches remained off. SSHFS returned zero despite the failed mount. RWS now supports `mount --fskit` and checks the actual filesystem device boundary before reporting mount success. Regression tests cover both behaviors.

Initially, System Settings showed both macFUSE FSKit modules disabled. Reading the FSKit enabled-module preference was denied by macOS even outside the execution sandbox. See [macFUSE issue #1194](https://github.com/macfuse/macfuse/issues/1194) for the related activation workaround.

## FSKit activation and real I/O follow-up

On macOS 27.0 build 26A428 with macFUSE 5.4.0 and SSHFS 3.7.5, the user ran a reviewed local helper after approving temporary terminal Full Disk Access. It preserved the five Apple entries, backed up the preference, added the two registered macFUSE module identifiers, and restarted `fskitd`. This single-machine recovery is not an automatically supported setup step. Boot security was not changed.

Observed against an isolated remote temporary directory:

- The OS reports an actual FSKit/macFUSE volume under `/Volumes`.
- Creating a directory, writing a text file, `fsync`, and reading it back succeed. An independent remote command confirms the exact contents.
- Renaming that file through the mount fails with `EINVAL` (22), repeatedly. The same rename succeeds through remote SSH.
- The rename failure also occurs with direct SSHFS `-f` and `-d`, without RWS. The debug trace advertises server POSIX rename support but shows no outgoing rename request for the failed operation. This narrows investigation to the local filesystem stack; it does not establish which component is responsible.
- Finder opens the volume, but its captured contents view is empty despite the files being accessible by path. Finder/editor acceptance has not passed.
- `rws unmount` returns zero and the OS mount entry disappears. Remote test data remains available.
- The ordinary `rws mount --fskit` command stays open while the volume is mounted, with SSHFS fork warnings. After unmount it reports no mounted filesystem. This was the synchronous child-process lifecycle defect, corrected in the implementation below; do not interpret this delayed error as proof that the preceding I/O did not occur.

Raw screenshots and SSHFS traces are retained in ignored `.rws-local/diagnostics/`; they are not public README assets. The dedicated remote test directory is retained for diagnosis. The terminal's temporary Full Disk Access switch was turned off; the user then confirmed quitting it and a process check found no running Ghostty process, completing removal of this temporary grant.

## Reproducible setup skill

The repository includes `.agents/skills/rws-macos-setup/SKILL.md`, linked from root agent guidance and the README. Structure, references, and privacy-sensitive examples were checked; independent agent scenarios reviewed the procedure. A complete setup on a second, fresh Mac has not been executed.

## First GitHub CI run

[Run 35415731910](https://github.com/ssime-git/RWS/actions/runs/35415731910) passed on both `macos-latest` and `ubuntu-latest` for commit `0885718`: formatting, locked tests, and Clippy with warnings denied. This validates the automated suite, not FSKit mounting or Finder editing.

## Implemented FSKit corrections

- `RWS_SSHFS` selects a custom executable for mount and doctor; the system installation remains untouched.
- `scripts/build-sshfs-fskit.sh` completed from pinned downloaded sources with verified hashes. Its compiled output was used for the real acceptance run.
- RWS mount returned zero in approximately 0.5 seconds while the actual volume stayed mounted.
- Create/read/fsync, rename, replace-existing, delete, and `renamex_np(RENAME_EXCL)` refusing overwrite passed. Independent SSH read confirmed the saved data.
- RWS unmount returned zero; the volume entry and its SSHFS process disappeared.
- A review reproduced an early-exit orphan. A failing regression test was added; `waitid(WNOWAIT)` now retains the child PID until group cleanup and the regression passes.
- Timeout, early exit (including exit zero), startup diagnostics, private log creation, live-child readiness and executable selection have automated coverage.

These tests do not validate every editor, concurrent remote writes, or network interruption recovery. Raw screenshots remain private.

Final-binary follow-up: nested working-directory mapping through the live mount passed. A nonexistent remote directory returned an error with the precise SSHFS diagnostic in its private log; no test volume or SSHFS process remained. Finder displayed the saved files (`10-rws-lifecycle-fixed.png`, private). SSHFS still emits its preexisting file-descriptor/fork warnings into the log; these are not claimed fixed.

## Follow-up on a remote Documents directory

A newly authorized Documents mount was exercised inside a uniquely created test subdirectory. Standard and exclusive creation with ASCII names, spaces in paths, UTF-8 file contents, fsync/readback, rename, replacing an existing target, no-replace protection, deletion of test files, nested CWD mapping, and remote exit code 37 passed. The final file's SHA-256 matched an independent remote read.

A filename containing composed accented characters failed: exclusive creation reported EEXIST while leaving an empty file remotely; subsequent ordinary opening reported ENOENT. ASCII exclusive creation passed, so this is not established as a general O_EXCL failure. Finder omitted the accented entry while listing the ordinary test file. At this stage Unicode filename handling was a confirmed limitation; the subsequent correction is recorded below. Avoid treating this run as full filesystem compatibility validation.

The requested Documents volume and test file were retained for user inspection. Exact paths, checksums, and a Finder screenshot are stored only in ignored local diagnostics. Existing user files were not selected for mutation. Network fault injection was not performed on this broader user-data mount.

## Unicode and Finder follow-up

The final `3.7.5-rws-fskit2` build passed exclusive creation, NFC/NFD reads, accented/Japanese/emoji filenames, rename, replacement, accented symlinks, and independent SSH SHA-256 comparison inside a new disposable Documents subdirectory. Both NFC-to-NFD and NFD-to-NFC alias sequences passed write/read/stat/unlink checks after disabling byte-keyed metadata caches. A pre-fix test reproduced stale existence after unlink; the final build reports the file absent. See [the conversion contract and performance tradeoff](sshfs-fskit.md#unicode-filenames); arbitrary existing remote naming schemes are not validated.

Finder displays the accented/emoji test file. With approved disk categories enabled, adding the selected volume through File > Add to Sidebar shows it under Locations with an eject button. The entry disappeared following remount and was added again for the retained final mount. Automatic sidebar persistence is not fixed. Private captures `12-unicode-fixed-finder.png` and `13-unicode-and-sidebar.png` record these states. The final Documents mount and test files remain available; no further privacy grant was used.

## Root listing regression and correction

A user report disproved the previous implication that browsing the retained volume was fully functional. The first root enumeration succeeded and subsequent reads returned no entries, although mkdir succeeded remotely. A fresh diagnostic mount reproduced this; SFTP traces showed READDIR repeatedly reaching EOF on the same handle. The new live regression script failed before the fix.

With `3.7.5-rws-fskit3`, repeated root/child enumeration and Unicode create/read/delete checks passed on both the diagnostic and final Documents mounts. A private SFTP handle per enumeration fixes the cursor reuse; review also identified the requirement to disable `nullpath_ok`. One intermediate diagnostic build crashed before this flag was corrected, leaving blocked OS I/O/unmount processes; its volume is absent from the mount table, but process cleanup was not established. The final corrected mount remains responsive. No global FSKit service restart was performed.

After replacing the stale sidebar shortcut, clicking it displayed the actual root contents. Finder New Folder created a visible directory, independently confirmed over SSH. Finder copy/paste created a visible text file whose contents were confirmed remotely. The user's earlier directories remained intact. Automated Cargo checks still pass (21 tests, format and Clippy). Screenshots and detailed traces are private. Full editor save workflows and automatic sidebar persistence remain unvalidated/unresolved respectively.

## Live Delta command forwarding — 2026-09-20

After user-completed Tailscale SSH reauthentication, Documents connected and
`status --no-probe` reported a verified RWS mount. A diagnostic sent through
the running Delta UI loaded the personal RWS rule, but resolved the existing
conversation's checkout under Delta Application Support as local. It returned
Darwin. Adding the mounted repository again did not relocate that checkout.

In the new draft opened by Add Project, selecting **Existing Local Checkout**
before sending the diagnostic made Delta use the mounted repository. Without
providing an explicit wrapper command in the diagnostic prompt, the agent ran
`rws context`, received `mode: remote` with `mount_verified: true`, and forwarded
the workload through `rws exec --git-context`. Observed UI output confirmed
Linux, the configured remote host, Linux Python, and the matching remote Git
root. Its first Python probe had a quoting error; the agent corrected it and
retried through RWS, returning Linux successfully. No local workload fallback
was observed in this remote diagnostic.

This validates one real Delta agent turn using the mounted primary checkout.
It does not validate migration of existing isolated checkouts, native Delta
process forwarding, or development/build completion. The old checkout was
preserved and the Documents mount left connected.

## Connection and routing release checks — 2026-09-20

The current uncommitted connection/routing implementation passed 46 tests on
macOS (12 library, 24 CLI, 3 core, 7 routing), `cargo fmt --check`, and
`cargo clippy --locked --all-targets -- -D warnings` immediately before commit.
The suite covers saved settings, mount receipt identity, safe challenge cleanup,
bounded SSH probes, shortcut generation, missing-registry refusal, path escapes,
and same-workspace Git metadata/URL mapping.

Earlier isolated live acceptance covered repeated connect/disconnect, busy
unmount refusal, rejection of an incorrect remote root, Unicode I/O with an
independent SSH hash comparison, nested Linux execution and remote exit status
37. Generic forwarding was exercised on two disposable projects, including an
accented directory and Delta-style Git metadata. These checks complement the
live Delta diagnostic above; they do not establish network-failure recovery or
persistent Finder sidebar integration. Private configuration and raw evidence
remain excluded from Git.

## Native app and release pipeline — 2026-09-20

A SwiftUI frontend and development/signed-release GitHub workflows were added.
The final Swift suite passed 15 tests; release tooling passed 7 tests.
A local Apple silicon development bundle compiled, its Info.plist validated,
its embedded CLI reported the expected version, and its ad-hoc code signature
passed deep/strict verification. Release metadata/key-pair/publication tests
passed, including refusing a mismatched key, refusing changes to a published
release and preserving a draft after upload failure.

The app launched and appeared in the running-app inventory, but the UI
automation tool failed while reading its window; the graphical connect/open/
disconnect flow has not been validated. The previous CLI and Delta diagnostic
results do not substitute for this app acceptance test.

No Apple Developer account was available. No public release, notarization or
installed Sparkle update is claimed. Production release attempts require the
configured keys and successful signing/notarization; development bundles have
updates disabled. Production bundles enable automatic update checks but disable
silent installation on quit; explicit installation is guarded by mount status.
See [setup and remaining acceptance](macos-app.md).

## Automatic startup detection — 2026-09-20

The screenshot reported a generic configuration-format rejection. The current
source decoder and bundled CLI both successfully read the actual existing
configuration; the exact original selection/build responsible for the rejection
was not established. Selection now uses the CLI's normalized JSON and errors
identify the selected file.

Startup discovers known configurations without a disk-wide search, preserves
their original location/receipts, and checks the macFUSE runtime plus SSHFS
version. Tests cover discovery precedence/ambiguity, dependency errors and literal
process-scoped SSHFS overrides. A read-only acceptance test against this Mac's
existing configuration, installed dependencies and mount status passed; its
configuration bytes remained identical. No system package was installed and
no active mount was disconnected. Detection does not prove FSKit activation.

The user supplied a screenshot establishing that the prior app window renders.
Native UI automation still could not acquire the app window, so the revised
startup UI requires a user relaunch check; helper/CLI checks are not claimed as
a complete graphical mount test.

## Finder automatic pinning — 2026-09-20

The live acceptance checks added a disposable favorite twice, verified exactly one
entry and unchanged other favorites, then removed the disposable entry. The existing
verified Documents mount was also pinned twice with one resulting favorite. Native
Finder UI inspection showed the entry under Favourites; clicking it displayed the
existing remote root contents. The previously existing Locations entry was preserved.
No active mount was disconnected for this test. Disconnect/remount and the complete
new-workspace form flow are not claimed as end-to-end validated.

A Swift ARC crash on the special last-item pointer was caught before shipping and
fixed by passing that sentinel inside a C bridge. The final suite passed 23 tests.
The API is deprecated, so pin failures remain explicit and manual Finder pinning is
the fallback.

## Remote agent launch — 2026-09-20

Direct non-login `exec -- claude --version` and the equivalent Codex/OpenCode/Gemini
checks failed with exit 127 on the VM because SSH's default PATH omitted their
user installations. The dedicated `agent` route now loads the remote login shell.
Live checks through that route returned Linux, the expected remote hostname and
workspace directory. Remote Python observed child PIDs/executable links via Linux
`/proc` and successful versions for Claude Code 2.1.274, codex-cli 0.154.0,
OpenCode 1.18.31 and Gemini 0.60.0. A PTY launch of Claude's version command also
passed. An absent agent returned 127; a fake-SSH regression returned 255 without
executing the locally available fake agent. Argument quoting and the app launcher
were tested, including shell metacharacters in executable/config paths.

These checks prove remote process startup, not authenticated inference, agent tool
execution, or the complete graphical button flow. No paid model request was sent.
Independent agents launched from a Mac IDE remain local unless explicitly configured
for remote execution. The app's terminal window is local; the launched agent is on
the configured SSH host. No automatic local fallback is implemented.

## Stability correction — stale bookmarks and build replacement, 2026-09-20

The earlier sidebar claim was insufficient: the user reproduced an unresolved
favorite after remount. Read-only inspection confirmed an unresolved RWS bookmark.
A regression that replaces a folder at the same path failed against the previous
implementation with two entries, then passed with explicit removal/recreation.
Each new entry carries its managed mount path; disconnect removes it even after
unmount. Legacy unresolved entries are migrated by the current RWS volume's exact
name, captured before disconnect when needed.

The live Documents workspace was connected, pinned, disconnected, unpinned,
reconnected and pinned again by the same CLI/helper paths used by the app. Status
confirmed a verified RWS mount. Swift tests: 26 executed, 25 passed, 1 optional
pin-only test skipped. No remote files were deleted. At this point the Mac was
locked: the final post-remount Finder click must still be performed, so this is
not yet a graphical end-to-end acceptance claim.

Builds now stage and validate before replacing the canonical app. The previous
bundle becomes a ZIP archive, no launchable backup is left, running bundles are
not replaced, and failures preserve the existing build. 17 release tests cover
publication rollback, refusal cases, metadata and production settings. The app
shows revision and UTC build timestamp. Historical executable backups from this
session were converted to verified ZIPs. Production signing/notarization remains
unavailable without an Apple Developer account.

## Finder click acceptance completed — 2026-09-20, 13:13 local

On resumption the workspace had been disconnected; clicking its retained favorite
produced the expected missing-original error. The authorized live test then
connected, pinned, disconnected, removed the favorite, reconnected and pinned
the Documents workspace again. Native Finder automation clicked the refreshed
favorite and displayed the remote root contents at `/Volumes/RWS-Documents/`.
It navigated to local Documents and clicked the remote favorite again; the same
remote contents appeared without an alert. This completes the post-remount Finder
click check previously blocked by the locked Mac. The mount was left connected.
A disconnected favorite does not auto-connect: use RWS to reconnect first.

The dedicated RWS window still cannot be inspected by the native automation tool
(pipe closes). This is not a claim that the complete graphical app button flow
was automated. CLI lifecycle, production sidebar helper and actual Finder clicks
were verified together.

## Terminal auto-switch acceptance — 2026-09-20, 15:00 local

The zsh integration installed automatically by the app (bundled `rws`,
`.rws-local` configuration baked as absolute paths) was exercised on the real
`razer-documents` workspace through a pseudo-TTY harness. Entering
`/Volumes/RWS-Documents/test-delta` prompted `RWS: switch to razer@razer-1
(razer-documents)? [Y/n]`; Enter opened the remote login shell directly in
`/home/razer/Documents/test-delta` (exact subdirectory mapping). Inside that
session: `pwd`, a pipe (`pwd | tr`), a variable with `&&` chaining, `git
--version` and `python3 -V` all executed on the VM; `exit` returned to the
local zsh still inside the mount, with no re-switch. Answering `n` kept the
shell local with the answer remembered; a shell without a controlling TTY
neither prompted nor switched. `RWS_AUTO_MODE=remote|local` forced both modes
in tests. 70 Rust tests (including PTY prompt tests through `script(1)`) and
the Swift suite passed.

Not covered by this entry: interactive Ctrl+C and terminal-resize behavior in
a human session (informally exercised, not recorded), other shells than zsh,
IDE-internal and noninteractive processes (issues #1/#2), and terminals opened
before the integration was installed (one `source ~/.zshrc` required).

## Delta execution paths acceptance — 2026-09-20, 18:10 local

Issue #2's command-creation paths were exercised in the live Delta UI on the
`test-delta` project inside the mounted workspace, by the user, with
screenshots retained.

- **Rules enabled (personal AGENT.md block):** an agent-issued
  `uname -s; hostname; pwd` executed remotely — `Linux / razer /
  /home/razer/Documents/test-delta` — after the agent's own `rws context`
  check, per the installed rules.
- **Rules disabled (AGENT.md renamed, Delta restarted):** the same request
  executed locally — `Darwin / Mac.lan / /Volumes/RWS-Documents/test-delta`.
  No hidden native redirection exists; instruction rules are the mechanism
  for this path, confirming the feasibility decision in
  docs/connection.md § Native execution limits.
- **SSH failure (Tailscale cut after the thread was prepared, rules
  enabled):** `git status` failed with `rws: resolve mount path:
  Input/output error (os error 5)`; the agent reported the failure and
  stopped, executing nothing locally. After the link returned, the same
  request ran remotely again.
- **Delta worktree preparation is its own local path:** with the link cut
  before thread preparation, Delta itself failed to create its checkout in
  the mounted folder (`os error 5`) and one agent turn fell back to a local
  `/Users/...` cwd — outside RWS's reach, consistent with the documented
  limits.

Not covered: Delta worktrees pointing at Mac-absolute Git metadata
(`--git-context` documented, not re-exercised here), and multi-workspace
switching during one thread.

## No-accidental-local-fallback acceptance — 2026-09-20, 18:40 local

Executed against the real razer-documents workspace with the bundled CLI:

- **Agent absent on the VM, local same-named witness on PATH:** `rws agent
  -- definitely-absent-xyz` exited 127 with the remote shell's "not found";
  the local witness executable never ran (no proof file written).
- **SSH interruption:** SIGINT during `rws exec -- sleep 20` ended with
  exit 255; nothing continued locally.
- **Unregistered RWS volume:** `exec --cwd /Volumes/RWS-missing-test`
  exited 1 with "unregistered RWS volume … no local fallback".
- **Directory outside every workspace:** `exec --cwd /tmp` exited 1 with
  "refusing local fallback".
- **VM unreachable during an agent turn:** recorded above (Delta execution
  paths acceptance): the agent reported the failure and stopped.
- **Terminal hook without a TTY:** stays silent and local by design; a `n`
  answer is an explicit user choice, not an accidental fallback.

Limit: a refused authentication was not simulated separately; SSH runs with
BatchMode where applicable and any interactive prompt therefore fails into
the same visible-error path as an unreachable host. Section 6 of the
architecture document now records the per-path integration decision this
evidence supports.

## Exact context and worktrees acceptance — 2026-09-20, 19:40 local

Executed with two distinct hosts: the razer-documents workspace
(razer@razer-1) and a disposable OrbStack Alpine machine
(rws-second@orb, workspace orb-second, remote root with accents and
spaces: "rws éssai deux").

- **Two hosts, subdirectories, accents/spaces:** `exec --cwd` from mounted
  subdirectories resolved to the exact remote directory on the right host on
  both machines ("…/rws éssai deux/sous dossier" on rws-second;
  "…/Documents/dossier éàç ü" on razer-1).
- **Agent in a chosen subdirectory:** new `rws agent --cwd` started the
  remote login environment in the mapped subdirectory on both hosts; the
  app's launcher gains an optional relative-subdirectory field using this
  route (".." and absolute paths rejected).
- **Escaping symlink:** a mounted symlink pointing outside the workspace was
  rejected ("escapes its registered RWS workspace through a symlink").
- **Worktree with Mac-absolute metadata:** a `git worktree` created from the
  Mac inside the mount carries `gitdir: /Volumes/…`; `exec --git-context`
  from that worktree ran Git remotely on the mapped directory with the right
  branch, and the `.git` file was byte-identical afterwards — metadata
  preserved, nothing rewritten.
- No Python environment or other local artifact was created in the remote
  projects by these runs.

The orb-second workspace and OrbStack machine are kept for future
multi-host testing; the workspace was disconnected after the runs.
Limits: the second host is a local virtual machine, not a second physical
network; Delta's own isolated-checkout relocation remains out of scope as
documented in docs/connection.md.
# Recovery maintenance — 23 September 2026

The setup/recovery changes were checked locally with Rust build, formatting,
Clippy (`--all-targets -D warnings`) and 120 passing tests. One additional
process-inspection lifecycle test cannot run in the automation sandbox:
`list processes: Operation not permitted`. It was run separately and remains
unverified here, not treated as a passing test.

New tests cover bounded subprocess output/timeouts, descendant-held pipes,
managed hook preservation and duplicate refusal, custom configuration retention,
private before/after reports, idempotent durable installation and safe existing
LaunchAgent refresh. Swift command/configuration tests were added, including
acceptance of the Rust durable `mount_state_generation` field. The local Swift
test invocation fails while linking the package manifest with an undefined
`PackageDescription.Package.__allocating_init` symbol, before app compilation.
The updated app has therefore **not** been built or installed locally.

The updated CLI was installed into the real durable user installation and the
existing generic LaunchAgent received private log paths (effective at next
login; loaded service state not verified). A real diagnosis confirmed both SSH
destinations reachable, one responsive verified mount and one disconnected mount.
An explicit repair of the disconnected workspace terminated at the 30-second
mount-start deadline and saved its before/after report. Its SSHFS log repeatedly
reported `invalidConnection` and requested helper installation; helper execution
also reported `Operation not permitted` in this automation environment. This
does not prove that the host's helper installation is missing. No global FSKit
restart was performed, no reboot recovery was verified, and Delta completion
remains unverified. Do not describe these maintenance changes as a fix for the
underlying FSKit deadlock or as clean-Mac end-to-end acceptance.

## Apple toolchain repair follow-up — 23 September 2026

The manifest linker failure was traced to obsolete Swift 5.10 private interfaces
left in the Swift 6.4 Command Line Tools installation. A controlled copy without
those interfaces compiled the same manifest; the original reproduced the undefined
constructor. The user then ran the SHA-256-guarded helper, preserving the two
interfaces and an obsolete duplicate SwiftBridging module map under backup names.
SwiftPM now resolves the package and downloads the pinned Sparkle artifact with
the system installation; the manifest failure is resolved.

The macOS 27 SDK subsequently fails app compilation because its SwiftUIMacros
plugin is absent from the installed Command Line Tools. Selecting the already
installed macOS 26.5 SDK explicitly gets through application compilation, but
the Swift tests remain blocked by the missing XCTest module. No Swift test is
claimed passing. Full Xcode is not installed, and no system-wide SDK/developer
directory setting was changed. The 120 runnable Rust tests and 17 release-tool
tests pass after the repair; the separate process-inspection sandbox limitation
remains. Delta and live remount acceptance have not been repeated in this step.

## Persistent mount intent — 23 September 2026

Connect/Disconnect now records desired state independently of mount receipts.
The periodic LaunchAgent skips paused workspaces and re-reads intent under the
same operation lock used by explicit commands. Tests cover a busy disconnect,
both retry/disconnect orderings with deterministic barriers, a later retry after
failure, aliases and dry runs, and preservation of a concurrent pause during
durable installation. Legacy plists gain a 30-second periodic schedule; absent
intent entries retain the former default of connecting registered workspaces.

A failing diagnostic regression demonstrated that an on-disk plist previously
reported success even when its loaded service could not be verified. Doctor now
uses bounded, read-only launchctl queries to check the loaded executable,
arguments, schedule and disabled status; unfamiliar output is not a success.
No service is automatically enabled or restarted by this diagnostic.

The Swift configuration round-trip regression failed on the old decoder and
passed after adding the typed intent map. A standalone executable compiled from
the production Domain.swift verified round-trip preservation and rejection of
invalid values locally, without XCTest or installing full Xcode. XCTest cases
are included for CI. The 17 Python release-tool tests also passed locally.

These are automated implementation checks, not physical reboot, network-loss,
Finder, or Delta acceptance. No user mount was disconnected or modified for this
work. Updating a plist does not update an already loaded job; deployment and
live acceptance remain separate. Unknown or empty legacy operation-lock files
still require inspection rather than unsafe automatic removal.

A final repeated-suite run exposed an intermittent advisory-lock release failure.
A deterministic fork/pipe regression reproduced it: an inherited descriptor kept
the lock held after the parent's file close. The lock guard now explicitly unlocks
before closing, after removing its legacy sentinel, including early-error paths.
The regression verifies immediate reacquisition while the child is still alive.
After this fix, three consecutive local Rust runs each passed 141 tests, excluding
the one known sandbox-blocked process-inspection test. Formatting, Clippy with
warnings denied, and diff whitespace checks passed. Full Swift/XCTest and the
unfiltered cross-platform Rust suites remain CI checks at this recording point.
