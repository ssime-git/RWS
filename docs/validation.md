# Prototype validation — 2026-09-19

Latest result: the [FSKit corrections](sshfs-fskit.md) are implemented and a real RWS file-operation cycle passes using the isolated patched SSHFS build. Earlier failure records below are preserved as diagnostic history; full editor and recovery acceptance remain outstanding.

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
