# Guided VM onboarding (`rws onboard`)

Date: 2026-10-22 · Status: approved by delegation ("propose et implémente").
Choices marked **(call)** are the builder's, not the user's.

## Intent

Someone with a working Mac and a Linux VM already reachable through Tailscale
should go from "RWS is installed" to "my VM folder is open in Finder and in a
terminal" with the least possible typing and no guesswork. The baseline is
checked first; when it is not met, RWS says exactly what to do and stops —
it does not install anything for the person.

## Goals

- One read-only command that checks the Mac and, given a host, the VM, and
  explains every failure with a concrete fix (`rws onboard check`).
- One command that runs that check and, only when it passes, registers the
  workspace on the NFS bridge, mounts it and proves it works (`rws onboard add`).
- The same engine drives the macOS app (an "Ajouter une VM" assistant) and the
  agent skill, through `--json`. **(call)**
- Tailscale machines are offered as a list, so nobody types a hostname.

## Non-goals and anti-goals

- No installation or configuration of Tailscale, OpenSSH, SSH keys, `known_hosts`
  or `~/.ssh/config`; no password handling; no remote installation or sudo.
- No fresh-Mac provisioning (Xcode, Rust) in this flow.
- Never a silent fallback: a failed check never turns into a partial setup.
- The zsh hook and Delta rules stay explicit opt-ins (see the integration write
  policy in `docs/architecture.md`).

## Checks

Each check has an id, a status (`ok`, `warn`, `fail`), a one-line detail and,
for `warn`/`fail`, a `fix` written for a human. `check` exits 1 when any check
fails; warnings do not fail it.

| Id | Meaning | Fail/warn handling |
| --- | --- | --- |
| `mac.ssh` | `ssh` executable present | fail: install Xcode command line tools |
| `mac.tailscale` | Tailscale CLI found, running, logged in | fail with the exact missing step; warn when the CLI is absent but the host answers |
| `vm.reachable` | `ssh -o BatchMode=yes HOST true` succeeds | classified from SSH's own message: unresolved name, refused, timeout/no route (Tailscale hint), public-key denied (key setup steps), unknown host key (connect once and confirm the fingerprint), changed host key (stop and investigate, never auto-fixed) |
| `vm.sftp` | the host's SFTP subsystem answers the protocol handshake | fail: SFTP server missing or disabled |
| `vm.atomic_save` | the SFTP server advertises `posix-rename@openssh.com` | warn: editors' atomic saves may fail |
| `vm.directory` | the remote folder exists, is a directory and is writable | fail: create it / fix permissions, with the exact command |
| `vm.tailscale_peer` | the host name is one of the tailnet's machines | warn only: host is outside Tailscale |

## Commands

```
rws onboard check [--host USER@HOST] [--remote DIR] [--json]
rws onboard add NAME --host USER@HOST --remote DIR [--mount PATH]
                [--install] [--dry-run] [--json]
```

- `check` without `--host` reports the Mac checks and the Tailscale machines
  (name, DNS name, online, OS) so a UI can build its picker.
- `add` runs `check`; on any fail it prints the checks and exits 1 without
  touching the configuration. When it passes it adds the workspace (mount
  defaults to `~/RWS/NAME`), selects the NFS backend if the configuration has
  none, connects (which attests with a proof file) and reports the result.
  `--install` then runs the durable installation and the LaunchAgent. `--dry-run`
  prints the planned steps and writes nothing.
- Orchestration re-invokes this same executable's existing subcommands
  (`workspace add`, `settings`, `connect`, `install`, `autostart`) rather than
  duplicating their logic. **(call)**

## App and skill

- App: the "Ajouter un espace" box gains a machine picker fed by
  `onboard check --json`, a "Vérifier" button listing each check with its fix,
  and "Ajouter" uses `onboard add`. **(call)**
- Skill `rws-vm-onboarding`: the A-to-Z procedure for an agent — Mac baseline,
  VM baseline, what to tell the person when NOK, `onboard add`, validation
  (file semantics, network cut), opt-in integrations, troubleshooting, and the
  completion record. `rws-macos-setup` remains the build-from-source reference.

## Testing

Pure parsers (SSH failure classifier, SFTP version reply, Tailscale status) are
unit tested; the commands are tested end to end with fake `ssh` and `tailscale`
executables placed first on `PATH`; `onboard add --dry-run` covers orchestration
without mounting. Live acceptance runs against the real razer host and the
OrbStack machine, including failure cases (unreachable, unknown user, missing
directory), and is recorded in the validation journal.
