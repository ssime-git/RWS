---
name: rws-vm-onboarding
description: Use when a person wants to connect a Mac to a remote Linux VM with RWS for the first time or add another VM, or asks why a VM cannot be added (SSH key denied, host key unknown or changed, VM unreachable, SFTP missing, remote folder missing). Assumes a working Mac and key-based SSH to the VM; Tailscale is recommended, not required.
---

# RWS VM onboarding, Mac and VM from A to Z

## Overview

Start from a working Mac and key-based SSH to the VM (`ssh USER@HOST true` works without a password); Tailscale is recommended for reaching the VM from anywhere. RWS checks that baseline. When something is missing it says exactly what, and **the person fixes it** — you never install or configure Tailscale, SSH keys, host trust or sudo for them. Everything goes through `rws onboard`: read its output, do not re-implement its checks. The volume appears at `~/RWS/NAME` through the NFS bridge: no macFUSE, no sudo, nothing installed on the VM beyond OpenSSH's SFTP server.

`rws` is `/Applications/RWS.app/Contents/Resources/bin/rws`, or `~/Library/Application Support/RWS/bin/rws` once installed durably, or `./target/debug/rws` in a checkout. After a durable install, link it onto PATH (`ln -sfn "$HOME/Library/Application Support/RWS/bin/rws" "$HOME/.local/bin/rws"`, check with `zsh -lic 'command -v rws'`). Ask before replacing an existing `rws` or editing `~/.zshrc`. Speak the person's language.

## Procedure

1. **Mac baseline.** `rws onboard check` (no host). It lists the person's Tailscale machines when Tailscale is connected; a `mac.tailscale` warning is fine if the VM is reachable over plain SSH. A `FAIL` means: show its `fix`, stop, resume when fixed.
2. **Ask, one question at a time:** which machine (offer the list, or take any `USER@HOST` they reach over SSH), the SSH **user** (never guess it), the absolute remote folder, a workspace name. Run `rws workspace list` first; an identical workspace already registered is reused, not duplicated.
3. **VM check.** `rws onboard check --host USER@MACHINE --remote /abs/dir` (add `--json` to parse). Exit 1 means at least one `FAIL`. Relay each `fix` verbatim; the person runs it in their own terminal. Repeat until `Ready.` A `warn` is information, not a failure.
4. **Dry run, then apply.** `rws onboard add NAME --host USER@MACHINE --remote /abs/dir --dry-run`, show the plan, and after agreement run it without `--dry-run`. Add `--install` for remount after login when the app is not managing that. Never combine `--install` with a `--config` that points to a different file than the installed one: it replaces the durable configuration.
5. **Prove it.** A mounted volume is not proof. Run `rws status NAME`, `rws exec --workspace NAME -- pwd`, `rws doctor --workspace NAME`, then a round trip: create a uniquely named file in `~/RWS/NAME`, read it back with `rws exec --workspace NAME -- cat FILE`, delete only that file.
6. **Network cut**, with the person's permission: cut the VM's route for about a minute (for example switch Tailscale off, if that is the route). Expected: reads and listings of the mount fail with `Input/output error` within seconds, listing `~/RWS` itself stays instant, and a few seconds after Tailscale is back the mount serves data again with no remount. Stopping an OrbStack machine is not an outage (it wakes on SSH).
7. **Integrations only on request:** `rws hook install` (zsh prompt `[Y/n]` when a shell enters the mount) and `rws agent-rules --all` (run on the Mac: rules for Claude Code, Codex, pi, Cline, Devin CLI and Delta; in a mount the agent asks Mac or VM once and keeps the answer 24 h). Never run them uninvited; a removed `.zshrc` line or agent rules block is a decision to respect.
8. **Record** what passed and what was not exercised (reboot, app clicks, delayed Tailscale start).

## When a check fails

| Code | Say | The person does |
| --- | --- | --- |
| `key_denied` | The VM has no key from this Mac | `ssh-copy-id USER@HOST`, typing the VM password at ssh's own prompt; no key yet: `ssh-keygen -t ed25519` first |
| `host_key_unknown` | First connection, never trusted | `ssh USER@HOST` once and confirm the fingerprint |
| `host_key_changed` | **Stop.** Rebuilt VM or impersonation | Only if they rebuilt it: `ssh-keygen -R NAME`. Never remove it for them without that confirmation |
| `tailscale_policy` | Tailscale SSH does not allow this user | Use an allowed user, or fix the ACL in the Tailscale admin console |
| `unreachable`, `unresolved` | No route to the VM | Check the host name and that `ssh USER@HOST true` works; if the VM is on their tailnet, connect Tailscale on both ends and pick the name from the machine list |
| `sftp_unavailable` | No SFTP subsystem | Add `Subsystem sftp internal-sftp` to `sshd_config`, reload sshd |
| `directory_missing`, `directory_not_writable` | Folder unusable | `mkdir -p` or `chown` on the VM, run by the person |
| `no_posix_rename` (warn) | Editors' atomic saves may fail | Use OpenSSH's own `sftp-server` |

## Never

- Type, store or relay a password or passphrase. If one is pasted in chat, say it is exposed and should be changed.
- Disable host-key checking, set `StrictHostKeyChecking=no`, edit `~/.ssh/config` or `authorized_keys`, or enable password login.
- Use a `/Volumes/...` mount path with this backend; the CLI refuses it.
- Run large Git operations, `git worktree` especially, through the mount: run them on the VM with `rws exec`. They froze the old FSKit stack.
- Claim success before step 5 has passed.

## Quick reference

Recovery of a dead mount: `rws repair --workspace NAME --mounts` (may force-eject; unsaved writes can be lost); `rws status NAME` reports `unresponsive mount`. The bridge listens on `127.0.0.1` without authenticating local processes, acceptable on a single-user Mac. The legacy macFUSE/SSHFS backend is only on explicit request: see `rws-macos-setup`, section 2b. Facts and dated evidence: `docs/onboarding.md`, `docs/nfs-bridge.md`, `docs/validation.md`.

## Common mistakes

- `rws status --workspace NAME`: wrong, the workspace is positional (`rws status NAME`).
- Testing in a remote `/tmp` directory on a host that clears it at boot.
- Reading a `warn` as a failure, or relaying a `fix` paraphrased instead of verbatim.
- Treating `rws doctor` or a mount entry as proof that I/O works.
