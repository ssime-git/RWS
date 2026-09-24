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
