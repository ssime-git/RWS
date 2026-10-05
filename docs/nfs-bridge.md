# NFS bridge backend (no macFUSE)

[Documentation](README.md) · [Troubleshooting](troubleshooting.md) · [Validation journal](validation.md)

The NFS backend mounts a workspace without macFUSE, SSHFS or any privilege.
RWS itself serves the files: `rws connect` starts `rws nfs-bridge`, a small
NFSv3 server that listens on `127.0.0.1` only and forwards every operation to
the remote host over the system `ssh -s HOST sftp` subsystem. macOS mounts that
server with its built-in NFS client.

```mermaid
flowchart LR
    Finder["Finder / editor"] --> Client["macOS NFS client (kernel)"]
    Client -->|"NFSv3, 127.0.0.1 only"| Bridge["rws nfs-bridge"]
    Bridge -->|"system ssh, SFTP"| Remote["sftp-server on the remote host"]
```

Nothing is installed on the remote host: the bridge uses the same SSH
configuration, keys and agent as `rws exec`.

## Switch an existing configuration

Mount points must belong to you. macOS deletes a mount point under `/Volumes`
when its volume is unmounted, and creating one there again requires root, so
the NFS backend refuses `/Volumes` paths.

```sh
rws install                                # durable binary first (see below)
rws settings --backend nfs
rws workspace relocate demo                # now mounts at ~/RWS/demo
rws connect demo
```

`workspace relocate` refuses a mounted workspace; disconnect it first. Pass
`--mount PATH` to choose another user-owned directory. Update Finder favorites,
editor recents and Delta projects that pointed at the old `/Volumes` path.

Install the new durable binary **before** saving the backend in the canonical
configuration. An older RWS rejects the unknown `nfs` setting, which would stop
the LaunchAgent and the zsh hook from reading the configuration.

`rws settings --backend fskit` or `--backend default` returns to SSHFS. Both
backends can stay installed while you compare them.

## Behavior

| Situation | Result |
| --- | --- |
| Bridge process killed | Operations fail within about a second; `umount -f` works without sudo |
| SSH connection lost (sleep, network change) | The failing operation returns an I/O error; the next one reconnects SFTP automatically |
| Volume unmounted (Finder eject, `rws disconnect`) | The bridge notices within a second and exits |
| Autostart, volume missing | The LaunchAgent remounts it on its next 30-second pass |
| Editor saves through a temporary file and rename | Supported through OpenSSH `posix-rename@openssh.com` |
| `._*` AppleDouble files and `.DS_Store` | Kept in the bridge's memory and never written remotely; extended attributes are lost at unmount |
| File names | Composed (NFC) before reaching the remote host; NFD and NFC spellings open the same file |

Metadata and directory listings are cached for one second, like `actimeo=1`.
A file replaced on the remote host is visible on the Mac within about two
seconds, once the bridge closes its idle read handle.

## Limits

- **Locking:** the mount uses `nolocks`. Programs that rely on advisory locks across machines are not protected.
- **Ownership:** every file appears owned by the local user. The remote server still enforces the SSH account's real permissions.
- **Server:** remote renames use an OpenSSH extension. Other SFTP servers are untested.
- **Names:** remote names must be valid UTF-8. A name stored decomposed (NFD) on the remote host cannot be opened.
- **Hard links:** not supported.
- **Network:** the client mounts `soft` with a 20-second timeout, and the bridge answers within 15 seconds. A stalled link therefore gives errors, not a frozen Finder.

## Diagnose

- `rws status` and `rws doctor` work as with SSHFS. `doctor` reports `sshfs` as not required.
- The bridge writes its messages to the private mount log printed by `rws connect` (`mount-logs/` beside the configuration).
- `mount | grep 127.0.0.1` lists bridge volumes. Each one shows its workspace, for example `127.0.0.1:/demo`.
