# Connect a Mac to a VM

[Documentation](README.md) · [Install](install.md) · [NFS bridge](nfs-bridge.md) · [Troubleshooting](troubleshooting.md)

This guide takes a working Mac and a VM you can already reach over Tailscale to
"my VM folder is open in Finder and in a terminal". RWS **checks** your setup
first and tells you exactly what to fix; it never installs or configures
Tailscale, SSH keys, host trust or sudo for you.

## Before you start

| On the Mac | On the VM |
| --- | --- |
| RWS installed (the app, or `rws` from a build) | Reachable over Tailscale (the machine shows online in `tailscale status`) |
| Tailscale installed, connected and signed in | SSH login with a key works: `ssh USER@MACHINE true` returns without a password |
| `ssh` available (Apple's command line tools) | OpenSSH's SFTP server enabled (the default on most Linux distributions) |
| | The folder you want to mount exists and you can write to it |

Nothing is installed on the VM. The Mac needs no macFUSE and no sudo: RWS serves
the files itself through the [NFS bridge](nfs-bridge.md).

## In the app

Open **Ajouter une VM**. Pick a machine from **Machines Tailscale** (or type
`utilisateur@machine`), enter the remote folder and a name, then press
**Vérifier**. Each finding is listed with the exact fix. When everything is
green, press **Ajouter et monter**: the folder appears at `~/RWS/NAME` and opens
in Finder. **Ajouter et monter** runs the same checks again, so it is safe to
press it directly.

## In the terminal

```sh
rws onboard check                                   # this Mac, plus your Tailscale machines
rws onboard check --host dev@my-vm --remote /home/dev/project
rws onboard add demo --host dev@my-vm --remote /home/dev/project --dry-run
rws onboard add demo --host dev@my-vm --remote /home/dev/project
```

- `check` is read-only. It exits 1 when a check fails; warnings do not fail it.
- `add` runs the same checks. If one fails, **nothing is changed**. When they
  pass it picks the NFS backend (only for a configuration that has no backend and
  no workspace yet), registers the workspace with the mount point `~/RWS/NAME`
  (override with `--mount`, never under `/Volumes`), mounts it and proves it with
  a proof file. `--dry-run` prints the plan and writes nothing.
- Re-running `add` is safe: an identical workspace is reused and only `connect`
  runs again. A name that exists with different settings, or a configuration that
  still uses the legacy SSHFS/FSKit backend, is refused with the exact next step.
- `--install` also installs RWS durably for your user and starts the login agent
  that remounts after a restart. Use it without `--config` once RWS is installed:
  with a different `--config` file, `install` replaces the durable configuration
  with that file. The app already manages durable installation, so skip it there.
- `--json` prints a machine-readable report (see below).

## What is checked

| Check | Passes when | A failure means |
| --- | --- | --- |
| `mac.ssh` | the `ssh` command exists | install Apple's tools: `xcode-select --install` |
| `mac.tailscale` | Tailscale is connected | switched off, or signed out (warning if its command-line tool is missing) |
| `vm.reachable` | `ssh -o BatchMode=yes USER@HOST true` succeeds | see the next table |
| `vm.tailscale_peer` | the host is one of your Tailscale machines | warning only: the VM is outside your tailnet |
| `vm.sftp` | the VM answers the SFTP protocol | the SFTP subsystem is missing or disabled |
| `vm.atomic_save` | the server supports `posix-rename@openssh.com` | warning: some editors may fail to save atomically |
| `vm.directory` | the remote folder exists and is writable | missing, not a folder, or read-only |

## Fixing a failure

| You see | Why | You do |
| --- | --- | --- |
| `key_denied` | the VM has no public key from this Mac | `ssh-copy-id USER@HOST` (type the VM password once at ssh's own prompt). No key yet: `ssh-keygen -t ed25519` first |
| `host_key_unknown` | this Mac never trusted the VM | `ssh USER@HOST` once and confirm the fingerprint |
| `host_key_changed` | the VM was rebuilt, or something impersonates it | **Do not continue blindly.** If you rebuilt it: `ssh-keygen -R NAME`, then connect again. Otherwise investigate |
| `tailscale_policy` | Tailscale SSH does not allow this user on that machine | use a user your ACL permits, or change the SSH rules in the Tailscale admin console |
| `unresolved`, `unreachable` | no route to the VM | connect Tailscale on both ends; take the name from the machine list |
| `refused` | nothing listens on the SSH port | start the SSH server on the VM (`sudo systemctl enable --now sshd`) |
| `sftp_unavailable` | no SFTP subsystem | add `Subsystem sftp internal-sftp` to `/etc/ssh/sshd_config`, then reload sshd |
| `directory_missing` | the folder does not exist | `mkdir -p '/path'` on the VM |
| `directory_not_writable` | read-only for your user | `sudo chown $USER '/path'` on the VM, or pick a folder you own |

RWS never answers a changed host key by removing it, never turns host-key
checking off and never types a password: those decisions and secrets stay with you.

## Check that it works

```sh
rws status demo
rws exec --workspace demo -- pwd
rws doctor --workspace demo
```

A mounted volume alone is not proof. Create a uniquely named file in
`~/RWS/demo`, read it back with `rws exec --workspace demo -- cat FILE`, then
delete only that file. To see what happens when the network drops, switch
Tailscale off for about a minute: reads and listings of the mount fail with
`Input/output error` within seconds (Finder does not freeze), and a few seconds
after Tailscale returns the mount serves data again without a remount. If a mount
ever stays dead, `rws repair --workspace demo --mounts` ejects and remounts it
(unsaved writes on a dead mount can be lost).

## Optional integrations

Neither is installed by onboarding; both are explicit:

```sh
rws hook install       # zsh: a [Y/n] prompt to switch into the VM when a shell enters the mount
rws agent-rules --all  # on the Mac: Claude Code, Codex, pi, Cline, Devin, Delta ask Mac or VM once (24 h)
```

## Machine-readable output

`rws onboard check --json` prints `{"ok", "checks", "machines"}`. Each check is
`{"id", "status": "ok" | "warn" | "fail", "code", "detail", "fix"?}`; `code` is
stable (for translation), `fix` is present for `warn` and `fail`. Each machine is
`{"name", "dns_name", "online", "os", "ips"}`, online machines first.
`rws onboard add --json` prints `{"ok", "dry_run", "checks", "steps", "results",
"mount"}`: `steps` is the plan, `results` what ran (`done`, `warning` or `failed`
with the command's output). The app and the agent skill use these reports.

## Limits

- Onboarding assumes the baseline above; it diagnoses, it does not provision.
- The bridge listens on `127.0.0.1` without authenticating local processes, which
  suits a single-user Mac ([details](nfs-bridge.md)).
- The legacy macFUSE/SSHFS backend is not part of this flow; see
  [install](install.md#legacy-backend-macfuse-and-sshfs).
- The agent procedure for all of this is `.agents/skills/rws-vm-onboarding/SKILL.md`.
