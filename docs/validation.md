# Prototype validation — 2026-09-19

## Verified locally

- Rust 1.98.1 build and test execution on Apple Silicon macOS using installed Command Line Tools.
- 12 automated integration tests passing: 3 core tests and 9 CLI tests.
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

- Actual SSHFS mount, Finder browsing/editing/saving, cache behavior, and unmount: SSHFS is installed, but cannot load the missing macFUSE library.
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
