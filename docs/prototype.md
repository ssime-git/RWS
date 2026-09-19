# Using the prototype

## Requirements

- macOS for mounting; the Rust core and non-mount CLI tests also target Linux.
- Rust stable and the macOS Command Line Tools to build. The initial local checks used Rust 1.98.1.
- Working system `ssh` and `sftp`, with a configured host and a POSIX-compatible remote login shell.
- For Finder access, install [macFUSE](https://macfuse.github.io/) and its separately distributed [SSHFS package](https://github.com/macfuse/macfuse/wiki/File-Systems-%E2%80%90-SSHFS). RWS does not install or configure system extensions. The appropriate backend must be configured and tested on your Mac.

## Build and register a workspace

```sh
cargo build --locked
./target/debug/rws workspace add demo \
  --ssh user@devbox \
  --remote /home/user/projects/demo \
  --mount "$HOME/RWS/demo"
./target/debug/rws workspace list
./target/debug/rws doctor --workspace demo
```

Replace the example destination and remote directory with your own. Use an SSH alias for custom ports, IPv6, bastions, identities, and other SSH configuration. RWS never rewrites `~/.ssh/config` or disables host-key checks.

Configuration defaults to `~/Library/Application Support/RWS/config.json` on macOS. Use `--config PATH` for a separate configuration. Writes use a lock and atomic replacement. A stale lock after a crash requires inspection and manual removal. There is no configuration editor or remove command yet; edit the JSON while no RWS registration command is running.

## Execute without mounting

```sh
./target/debug/rws exec --workspace demo -- pwd
./target/debug/rws exec --workspace demo -- git status --short
./target/debug/rws shell --workspace demo
```

`--workspace` starts at the remote workspace root. Without it, RWS maps the current local directory to its corresponding remote directory, rejecting paths outside configured roots. Symlinks are resolved locally and escaping links are rejected. Explicit execution does not need a mounted filesystem.

Arguments following `--` are literals. For intentional shell syntax, invoke it explicitly: `rws exec --workspace demo -- sh -c 'printf hello'`. This executes on the remote host. Remote command quoting assumes a POSIX-compatible login shell; fish/csh are not supported in this prototype.

RWS replaces itself with OpenSSH on Unix for command and terminal handling. `shell` requests a PTY and opens the remote `$SHELL` as a login shell (falling back to `/bin/sh` when unset); use it for interactive coding-agent CLIs already installed and authenticated remotely. Noninteractive `exec` does not request a PTY. Disconnect persistence and cross-device reattachment are not implemented.

## Mount and edit

```sh
./target/debug/rws mount demo --dry-run
./target/debug/rws mount demo
open "$HOME/RWS/demo"
# In a terminal with rws installed on PATH:
cd "$HOME/RWS/demo"
rws exec -- pwd
# After leaving the mounted directory and closing files:
rws unmount demo
```

Install the CLI on PATH with `cargo install --path . --locked` if desired. A dry run prints the exact program and argument array without invoking SSH or creating a mount directory. Mounting refuses a nonempty directory or a symlink mount point. Mount lifecycle is delegated to SSHFS and the OS; this prototype has no daemon or automatic reconnect policy.

`doctor` reports SSH/SFTP executable presence, verifies that `sshfs --version` succeeds, and optionally runs remote `pwd` with noninteractive SSH authentication. An SSHFS executable whose macFUSE library is missing is reported as unusable; mounting checks this before creating a mount directory. A missing mount dependency yields a nonzero result even if SSH works. It does not prove that macFUSE is loaded or a mount will succeed.

## Limits

- The CLI implements a subset of the original v0.1 specification.
- No unified Finder volume, bounded-cache guarantee, write atomicity guarantee, offline mode, or custom filesystem.
- No application launcher, PATH shims, port manager, transfer engine, remote daemon, or persistent sessions.
- `workspace list` and dry runs emit JSON. There is no global `--json` mode yet; command stdout/stderr are streamed unchanged, with execution location on stderr.
- Local environment forwarding is left to the user's SSH configuration; RWS adds no environment forwarding or credential copying.
- SSHFS/macFUSE behavior, filesystem latency, interruption handling, and Finder saves still require physical end-to-end validation.
- Windows and the graphical/mobile applications remain roadmap work.

## Manual acceptance check

Use a dedicated test directory on an authorized host. Mount it, create and edit a small file through Finder or an editor, then independently inspect its contents through `rws exec`. Check nested-directory mapping, remote failure exit codes, interactive Ctrl+C, and terminal resizing. Unmount normally. Only then test controlled disconnections and a second host; do not use valuable project data for fault injection.
