# Connect, disconnect and execute on the VM

[Documentation](README.md) · [App user guide](install.md) · [Architecture](architecture.md)

RWS exposes remote files on the Mac. Applications launched on the Mac still
execute their commands on the Mac unless they explicitly use SSH or RWS.

## Configure once

After registering a workspace, save this machine's backend and experimental
SSHFS binary. Replace the sample executable path with the actual build output:

```sh
rws settings --backend fskit --sshfs /absolute/path/to/sshfs
rws connect demo
rws status demo
rws disconnect demo
```

Use `--config /absolute/path/config.json` on every command if using a separate
configuration. `RWS_SSHFS` remains an explicit executable override. `mount` and
`unmount` remain aliases. `settings --backend default` restores the SSHFS default
backend. Existing configuration files remain readable without new fields.

Connection is repeatable: an already verified volume succeeds without remounting.
Disconnecting an absent volume also succeeds. A busy volume is left mounted;
close its files and terminals and retry. There is no forced unmount.

Status shows mount identity and SSH reachability separately. `--no-probe` skips
SSH. The SSH probe has a 12-second deadline; it checks the remote directory as
well as the connection. Neither state means an application's commands run remotely.
On systems other than macOS, mount inspection is reported as unavailable.

## Finder shortcuts

```sh
rws --config /absolute/path/config.json shortcuts demo --directory /absolute/path/shortcuts
```

Double-click `Connect-demo.command`, `Disconnect-demo.command`, or
`Status-demo.command`. `Shell-VM-demo.command` opens an explicitly remote shell.
The shortcuts contain absolute paths, preserve failures, and wait for Enter when
run interactively. Generation refuses to overwrite existing files. Keep the CLI
at its recorded location; regenerate into a new directory after moving it.
These CLI-generated shortcuts do not change Finder preferences or sidebar entries.
The native app separately attempts to manage its favorite; see [Finder behavior](finder-macos.md).

## Existing mounts and verification

An opaque FSKit volume UUID does not identify its SSH destination. On a fresh
connection, RWS writes an exclusive random `.rws-verify-*` directory containing
a small proof file on the configured remote host. It reads the proof through the
mount and checks the mount instance before and after reading. Only then does it
save a private receipt beside the configuration, under `CONFIG.mount-state/`.
The proof is removed through SSH. This requires a **writable remote root**, a
POSIX shell, and the ordinary `mkdir`, `cat`, `rm`, and `rmdir` utilities.
Read-only workspace verification is not supported by this version.

For a volume created by an older RWS version, verify it without disconnecting:

```sh
rws connect demo --verify-existing
```

Without that option, RWS refuses to adopt an unrecognized volume. A configuration
pointing at another remote directory fails verification and cannot acquire a
receipt for that mount. Replacing a volume invalidates the prior receipt.
Configuration-specific locks prevent overlapping operations on the same entry;
other configurations must independently verify the actual mounted destination.

If verification fails after mounting, the volume can remain mounted without a
receipt; the error says so. Correct the configuration and verify the existing
volume, or close your work and eject it in Finder. If a connection interruption
prevents challenge cleanup, the error identifies the exact remote recovery path.
Inspect that path; do not recursively delete unrelated `.rws-verify-*` directories.
Crash locks must be inspected before removing a stale lock. OS-level uninterruptible
filesystem I/O remains an underlying FSKit limitation, even with subprocess deadlines.

## Delta and Linux commands

### Install one rule for every registered project

```sh
/absolute/path/rws --config /absolute/path/config.json delta-rules
```

This installs a managed block in Delta's personal rules, choosing the existing
nonempty `AGENT.md`/`AGENTS.md` in its documented user directories (or
`DELTA_CONFIG_DIR`). Existing instructions are preserved and backed up privately
beside the RWS configuration (`agent-rule-backups/`) before changing them.
Repeating installation is idempotent: only the managed block between the
`BEGIN/END RWS REMOTE EXECUTION` markers is replaced, never your own rules.
Use `--output /absolute/path/AGENT.md` for an explicit location.

The block embeds the absolute paths of the `rws` binary and configuration that
installed it. Once installed, the app keeps it current: at startup and after a
workspace is added, it runs `delta-rules --if-installed`, which rewrites an
existing block with the bundled CLI's paths and does nothing when no block
exists — the first installation stays an explicit action (the app button or
the command above). Set the app preference `refreshDeltaRules` to `false` to
stop the automatic refresh; removing the block re-enables nothing by itself.

The installed rule applies across projects. It checks the actual filesystem
checkout with `rws context --cwd /absolute/checkout`. A verified registered mount
returns JSON with `mode: remote`, its host and its remote directory. A local
project returns `mode: local`. Missing configuration, unavailable/unverified
mounts, unknown `RWS-*` volumes, or paths escaping a mount return an error, not
permission to execute locally. Register future mounts in the same configuration;
there is no project-specific instruction to copy.

For remote projects the agent is instructed to forward the complete shell
script, including pipes, redirects and expansion, through:

```sh
rws --config /absolute/path/config.json exec --cwd /absolute/checkout --git-context -- sh -c 'python3 -m pytest'
```

`--git-context` maps the current repository's Git directory, worktree and common
directory for that invocation. It accepts both Mac and Linux absolute Git
metadata paths when they remain within the registered workspace, handles linked
worktrees, and translates Mac-local remote URL prefixes inside that mount via
Git's `url.insteadOf` mechanism. It does not rewrite any `.git` metadata. Metadata
outside that workspace is refused; local remote URLs on other mounts are not
automatically mapped. Start a separate wrapper invocation for another repository;
do not use `cd` or `git -C` to switch repositories inside the same command-scoped
Git environment. Existing macOS virtual environments must be preserved; Linux
dependencies can use a separate environment such as `.venv-rws`.

These are instructions for agent-issued commands, not a hook into Delta's
process launcher. Project/folder rules or user messages can override personal
rules. Delta's automatic preparation, direnv loading and native Git operations
are not redirected by this rule. Verify the rule's first use in Delta with
remote `uname -s`, `hostname`, and `pwd`; installing a file alone does not
prove that a Delta turn has followed it.

Delta's **interactive integrated terminals** are a separate path: they start
the user's login zsh, so the installed
[terminal integration](prototype.md#automatic-terminal-switch-zsh) prompts
there like in any other terminal when the working directory is a verified
mount. Measured on 2026-09-20 with a login-zsh PTY in the mounted workspace:
the `[Y/n]` prompt appeared, Enter opened the remote shell on the mapped
directory (`uname -s` → Linux), `n` kept the terminal local. Confirm inside
Delta's own terminal pane once per setup; a simulation is not that pane.

### Choose a checkout inside the mount

Mount the workspace **before** adding its repository in Delta. Use File > Add
Project to open the repository under the mounted volume. In the project menu
at the bottom of a new conversation, select **Existing Local Checkout** before
sending the first message. Here “local” means the folder visible to the Mac;
that folder can be the SSHFS mount. The global RWS rule then resolves it remotely.

Reattaching a folder does not relocate an existing conversation's isolated
checkout. A checkout under Delta's Application Support directory remains local,
even when the sidebar displays the same project name. Preserve that checkout
and its changes; do not silently execute against a different remote copy.
Check the actual `pwd` and `rws context` output before continuing development.
This workflow applies to any repository inside a verified registered mount.

See Delta's [checkout documentation](https://delta.dev/docs/concepts/worktrees).
The mounted existing-checkout diagnostic was exercised in the live Delta UI;
relocation of an existing isolated checkout was not validated.

### Native execution limits

Delta documents that its agent command tool uses `/bin/sh` on the computer running
the turn. Its Default Shell setting affects interactive terminals only. No SSH
agent backend was found in the documented settings checked on 2026-09-19:
[agent terminals](https://delta.dev/docs/agents/terminals),
[settings](https://delta.dev/docs/configuration/settings),
[collaboration execution location](https://delta.dev/docs/collaboration/collaborate-thread).

A remote filesystem mount therefore does not relocate Delta's command tool.
Measured on 2026-09-20: `/bin/sh -c` in the mounted workspace runs on the Mac
with no prompt and no redirection — expected, since a noninteractive sh reads
no startup files and the zsh integration deliberately never acts without a
controlling TTY. **Feasibility decision:** without a Delta-side execution
backend, transparent redirection of this tool path is not implementable from
outside the app; the installed instruction rules remain the supported
mechanism for agent-issued commands, and `RWS_AUTO_MODE=remote|local` is the
environment contract available to any launcher that wants to force a mode.
From a local terminal inside a mounted subdirectory, use:

```sh
/absolute/path/rws --config /absolute/path/config.json exec -- uname -s
/absolute/path/rws --config /absolute/path/config.json exec -- python3 -m pytest
```

Both commands execute on the configured SSH host in the mapped subdirectory.
For deliberate shell operators, use `exec -- sh -c 'command1 && command2'`.
`--workspace demo` starts at the workspace root, not the current subdirectory.
SSH errors never cause the requested command to fall back to local execution.

Instructing Delta to use these commands is a convention, not enforcement of all
its terminal, Git and file operations. To have Delta's own processes on Linux,
run its supported Linux desktop app and send turns there. That installation and
remote-desktop workflow are not validated here.

Delta-created worktrees may contain absolute Mac paths in their Git metadata.
Use `--git-context` for the supported same-workspace mapping described above.
Do not rewrite an active worktree's `.git` or reuse a macOS virtual environment
on Linux. Linux dependencies are not installed or migrated automatically.
