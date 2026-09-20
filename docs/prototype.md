# CLI reference and quick start

[Documentation](README.md) · [App user guide](install.md) · [Architecture](architecture.md)

## Requirements

- macOS for mounting; the Rust core and non-mount CLI tests also target Linux.
- Rust stable and the macOS Command Line Tools to build. The initial local checks used Rust 1.98.1.
- Working system `ssh` and `sftp`, with a configured host and a POSIX-compatible remote login shell.
- For Finder access, install [macFUSE](https://macfuse.github.io/) and a compatible SSHFS implementation. The currently tested FSKit path requires the [patched RWS SSHFS build](sshfs-fskit.md); a stock SSHFS installation alone does not provide those corrections. RWS does not install or configure system extensions. The appropriate backend must be configured and tested on your Mac.

## Build and register a workspace

```sh
cargo build --locked
./target/debug/rws workspace add demo \
  --ssh user@devbox \
  --remote /home/user/projects/demo \
  --mount /Volumes/RWS-demo
./target/debug/rws workspace list
./target/debug/rws doctor --workspace demo
```

Replace the example destination and remote directory with your own. Use an SSH alias for custom ports, IPv6, bastions, identities, and other SSH configuration. RWS never rewrites `~/.ssh/config` or disables host-key checks.

Configuration defaults to `~/Library/Application Support/RWS/config.json` on macOS. Use `--config PATH` for a separate configuration. Writes use a lock and atomic replacement. A stale lock after a crash requires inspection and manual removal. The app can add workspaces and save mount settings, but has no full workspace editor or remove action yet. For manual JSON edits, stop RWS operations first and preserve existing mount receipts; do not modify an active registration.

## Execute without mounting

```sh
./target/debug/rws exec --workspace demo -- pwd
./target/debug/rws exec --workspace demo -- git status --short
./target/debug/rws shell --workspace demo
```

`--workspace` starts at the remote workspace root. Without it, RWS maps the current local directory to its corresponding remote directory, rejecting paths outside configured roots. Symlinks are resolved locally and escaping links are rejected. Explicit execution does not need a mounted filesystem.

Arguments following `--` are literals. For intentional shell syntax, invoke it explicitly: `rws exec --workspace demo -- sh -c 'printf hello'`. This executes on the remote host. Remote command quoting assumes a POSIX-compatible login shell; fish/csh are not supported in this prototype.

RWS replaces itself with OpenSSH on Unix for command and terminal handling. `shell` requests a PTY and opens the remote `$SHELL` as a login shell (falling back to `/bin/sh` when unset); use it for interactive coding-agent CLIs already installed and authenticated remotely. Noninteractive `exec` does not request a PTY. Disconnect persistence and cross-device reattachment are not implemented.

## Automatic terminal switch (zsh)

```sh
rws hook install                      # writes the eval line into ~/.zshrc
rws --config path/to/config.json hook install   # same, with that config baked in
```

`hook install` appends one marked line to `~/.zshrc` (`$ZDOTDIR` respected,
`--zshrc PATH` overrides) and replaces its own previous line on re-run; other
content is preserved. The equivalent manual form is
`eval "$(rws hook zsh)"` in `~/.zshrc`. The macOS app runs this installation
automatically for its active configuration and bundled CLI, so app users get
the integration without any manual command.

With the hook installed, `cd` into a verified mount (or opening a terminal
there, in any emulator) asks once per shell:
`RWS: switch to <host> (<workspace>)? [Y/n]`. Enter or `y`/`o` opens
`rws shell` on the mapped remote directory; `n` keeps the shell local. The
answer is remembered per workspace for that shell's lifetime: re-entering the
volume reapplies it without asking, and a new terminal asks again. Leaving the
remote shell with `exit` returns to the local zsh; RWS does not switch again
until you leave the volume and re-enter it. Details:

- Detection calls `rws context --cwd`, so only registered workspaces with a
  verified mount identity switch; an unregistered `RWS-` volume reports its
  error once per entry instead of silently running locally.
- Scope is interactive zsh only. Scripts, other shells, IDE-internal and
  noninteractive processes are not covered; broader coverage remains
  [#3](https://github.com/ssime-git/RWS/issues/3).
- Without an accessible controlling terminal (scripts, IDE-internal
  subprocesses) nothing prompts and nothing switches.
- `RWS_AUTO_MODE=remote` or `RWS_AUTO_MODE=local` forces the mode without a
  prompt — the programmatic surface for agents and automation.
- `RWS_NO_AUTO_SHELL=1` disables the switch for a shell;
  `RWS_AUTO_PREFIX` overrides the `/Volumes` detection prefix;
  `RWS_CONFIG` points detection and the shell at a non-default
  configuration file and overrides a baked `--config` path.
- The snippet embeds the absolute path of the `rws` that printed it; reprint
  after moving the binary.

## Mount and edit

Use the [connection guide](connection.md) for `connect`, `disconnect`, `status`,
saved backend settings and Finder-launchable shortcuts. `mount`/`unmount` remain
aliases. `status` probes a verified mount with a bounded directory read and
reports `connected (unresponsive mount: …)` when I/O fails or hangs — typically
after a network loss; `connect NAME --repair` then force-ejects that dead
volume, terminates the workspace's stale SSHFS server process if one
outlived the ejection, verifies the mount point answers, and mounts again.
It is refused while the mount answers normally, and stops with a
remediation message (`sudo pkill -9 fskitd`, reboot as fallback) instead of
hanging when the FSKit service itself is wedged. Mount identity verification now uses a disposable remote proof file and
therefore requires a writable workspace root. Existing volumes can be verified
with `connect NAME --verify-existing` without disconnecting them.

On the tested macOS 27.0 / macFUSE 5.4.0 stack, use the [experimental SSHFS build](sshfs-fskit.md) for rename, Unicode filenames, and repeated directory listings. RWS now returns when the mounted filesystem is detected while SSHFS continues in the background. Full editor and recovery acceptance remains pending.

```sh
./target/debug/rws mount demo --fskit --dry-run
./target/debug/rws mount demo --fskit
open /Volumes/RWS-demo
# In a terminal with rws installed on PATH:
cd /Volumes/RWS-demo
rws exec -- pwd
# After leaving the mounted directory and closing files:
rws unmount demo
```

Install the CLI on PATH with `cargo install --path . --locked` if desired. A dry run prints the exact program and argument array without invoking SSH or creating a mount directory. Mounting refuses a nonempty directory or a symlink mount point. `--fskit` selects the user-space backend and requires a direct child of `/Volumes`; macFUSE creates the mount directory. Enable its FSKit module in System Settings > General > Login Items & Extensions. Without this flag or a saved FSKit setting, SSHFS uses its default backend. An already verified mount succeeds without remounting; an unrecognized volume requires explicit verification. Startup checks a filesystem device boundary while foreground-mode SSHFS remains alive, then verifies the remote destination through a disposable proof file. Startup has a 30-second polling deadline; errors identify a private log beside the configuration. SSHFS runs in its own process group and exits on OS unmount. Authentication must already work noninteractively. This prototype has no RWS daemon or automatic reconnect policy.

`doctor` reports SSH/SFTP executable presence, verifies that `sshfs --version` succeeds, and optionally runs remote `pwd` with noninteractive SSH authentication. An SSHFS executable whose macFUSE library is missing is reported as unusable; mounting checks this before creating a mount directory. A missing mount dependency yields a nonzero result even if SSH works. It does not prove that macFUSE is loaded or a mount will succeed.

For the left sidebar, follow [Finder configuration and acceptance](finder-macos.md). The guide covers display categories, stale shortcuts after remount, and independent checks of Finder-created files.

## Limits

- The CLI implements a subset of the original v0.1 specification.
- No unified Finder volume, bounded-cache guarantee, write atomicity guarantee, offline mode, or custom filesystem.
- No application execution redirection, PATH shims, port manager, transfer engine, remote daemon, or persistent sessions. Generated Finder shortcuts launch RWS commands, including an explicitly remote shell.
- `workspace list` and dry runs emit JSON. There is no global `--json` mode yet; command stdout/stderr are streamed unchanged, with execution location on stderr.
- Local environment forwarding is left to the user's SSH configuration; RWS adds no environment forwarding or credential copying.
- SSHFS/macFUSE behavior, filesystem latency, interruption handling, and Finder saves still require physical end-to-end validation.
- A native macOS app is implemented. Windows and mobile applications remain roadmap work.

## Manual acceptance check

Use a dedicated test directory on an authorized host. Mount it, create and edit a small file through Finder or an editor, then independently inspect its contents through `rws exec`. Check nested-directory mapping, remote failure exit codes, interactive Ctrl+C, and terminal resizing. Unmount normally. Only then test controlled disconnections and a second host; do not use valuable project data for fault injection.

## FSKit switches that do not activate

A similar activation issue is tracked in [macFUSE issue #1194](https://github.com/macfuse/macfuse/issues/1194). Registration does not prove enablement. A community workaround edits the FSKit enabled-module preference and restarts its service. A user-authorized, backed-up repair enabled a real mount on one test Mac; this is not a general setup guarantee and is not performed automatically by RWS. Follow the repository setup skill for diagnosis and temporary-permission cleanup. Do not treat the legacy kernel-extension setup as a required step for FSKit.

## Agents installed on the VM

`rws agent --workspace demo -- claude` (or `codex`, `opencode`, another executable)
uses SSH with a PTY and the remote user's login shell.
`rws agent --cwd /mounted/subdirectory -- claude` starts in the remote
directory mapped from that mounted path instead of the workspace root, with
the same resolution and verified-mount requirement as `exec --cwd`; the app's
optional « Sous-dossier » field uses this route. It loads the remote login
profile, changes to the configured remote directory afterwards, prints the remote
host/OS/directory on stderr and execs the requested agent there. Arguments after
`--` are literal. `--no-tty` supports noninteractive diagnostics such as `--version`.
The remote login shell must support POSIX `-lc` semantics. Executables must be
installed and available in that remote environment; no local PATH or credentials
are copied. Missing agents and failed SSH connections return errors, with no local
fallback. An agent can still perform its own network operations according to its
remote configuration.

The RWS app provides an executable field and **Lancer sur la VM** for the selected
workspace. It opens a private launcher in macOS Terminal, which hosts SSH; the
agent runs on the VM. Enter just the executable name or its absolute remote path,
not a shell command with flags. CLI users can supply additional literal arguments.
The mount is not required for this explicit remote launch. Independent local IDE
agent buttons are not intercepted by this feature. Sessions do not persist after
SSH disconnect unless managed separately on the VM.
