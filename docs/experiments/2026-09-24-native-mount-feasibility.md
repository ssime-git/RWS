# Native macOS mount feasibility, 2026-09-24

Status: exploratory. No production host, RWS configuration, application, or active RWS volume was changed. The current SSHFS/FSKit backend remains in use.

## Goal and environment

Determine whether a native macOS filesystem protocol can replace SSHFS/FSKit while retaining `/Volumes/RWS-ssime-omarchy` and `/Volumes/RWS-razer-1` and ordinary Unix project semantics. Mac: macOS 27.0 arm64. Razer remained mounted during the experiments; Omarchy was disconnected. Both real Linux hosts are Arch based, reachable by SSH/Tailscale, and had no Samba installation at inventory time. These facts do not authorize remote service installation.

## SMB: exact path works; Unix semantics fail

A disposable Samba 4.19 server and a disposable Arch Samba 4.24.7 server were tested on localhost, using synthetic files and credentials. Apple's `NetFSMountURLSync` mounted a share named `RWS-test` under `/Volumes`, creating `/Volumes/RWS-test` without a precreated directory or privileged helper. The mount was `smbfs` and was normally unmounted after each probe.

With vanilla Samba, a symlink created by macOS was an ordinary file on Linux, and a Linux symlink did not appear as a symlink on macOS. `chmod 0755` did not round trip. Samba 4.24.7 with `smb3 unix extensions = yes` still failed the symlink and mode probes. Enabling `vfs_fruit` with stream metadata made `chmod 0755` round trip, but symlinks still failed in both directions. AppleDouble sidecars also appeared. Consequently SMB does not meet RWS's coding-workspace contract in the tested configurations. Do not add SMB as the default mount backend on the strength of the path test alone. Samba's SMB3 Unix extensions documentation addresses Unix/Linux clients, and the macOS SMB client did not provide the needed behavior in this experiment.

## NFS: native mount possible; export semantics unproven

`mount_nfs -o vers=4,tcp,port=2049` mounted a disposable NFSv4 export at `/private/tmp/rws-nfs-mount` as the logged-in user. This establishes that a native NFS client exists and can mount a prepared path without macFUSE. It does not establish automatic creation of exact `/Volumes/RWS-*` paths: the NetFS NFS URL experiment returned status 61 against the NFSv4-only test server.

Several disposable NFS-Ganesha 4.3 server configurations were attempted on OrbStack: Docker overlay storage, a named Docker volume, and an Ubuntu VM exporting both btrfs and tmpfs. The client could mount, but file operations returned `Operation not permitted`; Ganesha logged `fsal_common_is_referral ... Forbidden action`. On the first export macOS displayed the export owner as UID 4294967294, so NFSv4 identity mapping also needs an explicit validation. The server was allowed to finish its post-restart grace period before the final retry. These failures may be limitations of the disposable server/storage environment and do not establish that NFS itself fails on the real Arch hosts. They also do not prove that NFS meets the product contract.

An additional write test against OrbStack's own NFS share was denied by that share's permissions, so it provided no POSIX-semantics result. No project file was changed. All disposable test mounts were normally unmounted, the test VM/container/volume removed, and OrbStack restored to its initially stopped state.

## Decision gate

The earlier SMB design in `.agents/superpowers/specs/2026-09-24-rws-native-smb-mounts-design.md` is superseded as an implementation choice by the symlink results above. The isolated branch is named `dev/native-mount-feasibility`. No native-backend product code should be shipped until a representative disposable Linux NFS server passes bidirectional symlinks, modes, rename, Unicode, editor-save behavior, and a repeated mount/unmount cycle on macOS, followed by a fixed `/Volumes` path and login-recovery proof. Remote provisioning, NFS security/UID mapping, mountpoint ownership, and rollback must be designed and tested before touching either real host. The existing FSKit path remains the only installed backend; its durability after reboot is not established.

## Follow-up: userspace NFSv3 export passed POSIX probes

A disposable Alpine `unfs3` 0.10.0 container was reached directly over its private OrbStack address. Loopback port forwarding initially returned `Permission denied`; direct container-IP mounting succeeded. The Mac mounted with `mount_nfs -o vers=3,tcp,port=2049,mountport=20048,nolocks,nfc`. Synthetic Mac-created files and symlinks were verified from Linux, and Linux-created symlinks and modes were verified from macOS. Atomic replacement, case-distinct files, and Unicode filename lookup passed. The macOS `nfc` mount option was necessary for an NFD lookup of a Linux NFC filename; without it that lookup failed. Extended attributes worked through macOS AppleDouble sidecars.

Both real Arch hosts report UID/GID 1000:1000, while the Mac user is 501:20. A second disposable export used `all_squash,anonuid=1000,anongid=1000` with the export directory owned 1000:1000 mode 0700. The Mac user created files, changed executable mode and created symlinks; Linux saw the new objects owned by 1000:1000. This is an encouraging compatibility result, not a production-server acceptance result.

The test server does **not** implement the NFS network lock manager. A `flock` probe returned `EOPNOTSUPP` (errno 45), so this userspace server is unsuitable as the RWS production backend. A standard Linux kernel NFS server must pass locks and repeated recovery tests before deployment. NFSv4 remains unvalidated: the earlier Ganesha test failed in its test export implementation, not in a verified kernel server.

Read-only inventory of both real hosts: Arch based, btrfs `/home`, Tailscale active, `nfs-utils` absent, NFS service inactive, UID/GID 1000:1000. Their Tailscale IPv4 addresses were `100.99.168.72` (Omarchy) and `100.106.23.6` (Razer) at inspection time. No package, service, export, firewall or project file on either host was changed.

The branch now contains an explicit experimental NFS backend in Rust and the macOS app, a privileged one-time `/Volumes/RWS-NAME` directory creator, native NFS mounting with receipt/SSH challenge verification, independent login retries per workspace, and an NFS-only installation path that does not require SSHFS. These are implementation checks; the installed app/config remain unchanged. Do not enable NFS for the two real workspaces until a kernel NFS server, exact-path mount, login/reboot, network-loss and rollback acceptance pass.

## Kernel-server trial in a disposable Arch OrbStack machine

A new Arch machine in OrbStack was updated and given `nfs-utils`. Its kernel NFS server exported a dedicated directory as the NFSv4 pseudoroot (`fsid=0`), restricted to the private OrbStack subnet, with `all_squash` to the test owner. The macOS client returned `Input/output error` for `vers=4` (NFSv4.0) but mounted the export with `vers=4.1,port=2049`. This validates the mount negotiation only. A directory create became delayed, and creating the first child file blocked until a forced unmount, then returned `ENOENT`. The same behavior recurred with an in-memory tmpfs export, so changing away from OrbStack's btrfs storage did not resolve it. The kernel-server trial **failed** the file-operation acceptance test; no locking or editor acceptance is claimed.

The client address recorded by the server was the expected private Mac address, and the NFS server was active and listening on TCP 2049. The failure may reflect the containerized machine/NFSD environment; no causal attribution to NFSv4 or btrfs on the real hosts is justified. Both temporary mounts were force-unmounted after I/O stopped responding. The disposable machine was deleted and OrbStack returned to its initially stopped state. The test did not touch Omarchy, Razer, either production volume, or the production RWS configuration.

The prototype now requests NFSv4.1 on port 2049 and addresses the NFSv4 pseudoroot (`host:/`); the SSH challenge must still prove that this root is the configured remote directory. This command shape remains a hypothesis for real Arch servers until an authorized isolated export passes file operations and locks. The privileged local preparation supports an existing empty, unmounted root-owned `/Volumes/RWS-NAME` directory without removing or renaming it.
