# Prototype validation — 2026-09-19

## Verified locally

- Rust 1.98.1 build and test execution on Apple Silicon macOS using installed Command Line Tools.
- 14 automated integration tests passing: 3 core tests and 11 CLI tests.
- `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings` passing.
- Independent code review identified mount-root alias overlap; a failing regression test reproduced it and the fix now resolves existing ancestors before comparing roots.
- Missing SSHFS produces a clear error without creating the mount directory.
- Local Git repository initialized on `prototype/cli`; nothing published to GitHub.

## Verified on an authorized Linux SSH host

- OpenSSH and SFTP connection.
- Actual `rws exec` prints the expected remote working directory.
- Spaces, quotes, Unicode, and `$(id)` are passed as literal arguments.
- Remote exit code 37 is preserved by the CLI.
- `rws shell` opens a real remote PTY; `test -t 0` succeeds and normal exit closes the connection.
- A file written and read through a remote command in a unique temporary directory has expected contents. The test file and directory were removed afterward.

These checks validate remote execution, not editing through the filesystem mount.

## Not yet verified

- Reliable Finder browsing/editing/saving and cache behavior. Basic mounting and I/O now work, but rename and the mount command lifecycle remain blocked as detailed below.
- PTY resize and Ctrl+C under a long-running job, interrupted remote writes, and network-loss recovery.
- Coding-agent launch: no agent was installed or authenticated as part of these checks.
- Second host, transfer behavior, and performance targets from v0.1.
- GitHub Actions execution and Linux CI results: the workflow is prepared but has not run remotely.

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
- The ordinary `rws mount --fskit` command stays open while the volume is mounted, with SSHFS fork warnings. After unmount it reports no mounted filesystem. Its synchronous child-process lifecycle still needs correction; do not interpret this delayed error as proof that the preceding I/O did not occur.

Raw screenshots and SSHFS traces are retained in ignored `.rws-local/diagnostics/`; they are not public README assets. The dedicated remote test directory is retained for diagnosis. The terminal's temporary Full Disk Access switch was turned off; the user then confirmed quitting it and a process check found no running Ghostty process, completing removal of this temporary grant.

## Reproducible setup skill

The repository includes `.agents/skills/rws-macos-setup/SKILL.md`, linked from root agent guidance and the README. Structure, references, and privacy-sensitive examples were checked; independent agent scenarios reviewed the procedure. A complete setup on a second, fresh Mac has not been executed.
