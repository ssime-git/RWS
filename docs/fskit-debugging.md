# FSKit diagnosis — 2026-09-19

Environment: macOS 27.0 (26A428), macFUSE 5.4.0, SSHFS 3.7.5. These findings apply to this tested combination. The installed SSHFS executable and system libraries were not modified.

## Rename capability mismatch

SSHFS 3.7.5's `sshfs_rename` rejects every nonzero flag with `EINVAL`. Its initialization does not clear macFUSE's extended-rename capability.

A minimal local FUSE filesystem, with no SSH connection, reproduced the mismatch:

| Test | Requested Darwin capabilities | Callback flags | Result |
| --- | --- | --- | --- |
| Default capabilities | 235 | 4 (`RENAME_EXCL`) | EINVAL |
| Clear `FUSE_DARWIN_CAP_RENAME_EXT` | 227 | 0 | Rename succeeds |

The probe deliberately rejects nonzero flags, matching SSHFS. Changing only the negotiated capability alters the request FSKit sends.

To confirm the effect in SSHFS itself, the official 3.7.5 sources were compiled twice against the installed library. The unchanged build reproduces EINVAL. An experimental build that clears `conn->want_darwin & FUSE_DARWIN_CAP_RENAME_EXT` during initialization passes rename to a new name, replacement of an existing file, readback, and rename back. Independent SSH execution confirms the resulting remote contents. This is capability negotiation, not silently discarding rename flags or replacing rename with copy/delete.

Finder displayed the text file and nested folder during this experimental run. A raw screenshot is retained privately. Editor save acceptance, concurrent rename semantics, and broader regression coverage remain outstanding. At this diagnostic stage it was not yet integrated. The subsequent [implementation](sshfs-fskit.md) adds a reproducible local build and RWS lifecycle management; system SSHFS remains unchanged.

## Mount command lifecycle

SSHFS 3.7.5 calls `fuse_mount`, connects SSH, and only then calls `fuse_daemonize`. macFUSE 5.4.0 explicitly forces foreground operation when daemonization is requested after mounting has started. RWS waits synchronously for SSHFS to exit, which therefore happens on unmount; its post-exit mount check then necessarily fails.

The resulting RWS correction must explicitly handle a foreground SSHFS child, confirm mount readiness while it is running, preserve startup diagnostics, and define process cleanup on failure/unmount. A bounded startup wait and lifecycle tests are needed. Merely suppressing the warning or removing the post-mount check would hide the problem.

## Available tools and remaining work

- Available and exercised: Command Line Tools/Clang, installed FUSE headers/library, Homebrew GLib/GThread headers and libraries, the existing Rust toolchain, GitHub CLI, and authorized SSH access to an isolated test directory.
- Meson, Ninja, and pkg-config were not on PATH. They were unnecessary for the diagnostic builds: direct Clang compilation succeeded using existing dependencies.
- No new installation, Full Disk Access grant, system extension change, or boot-security change was needed.
- Experimental sources, binaries, probes, and logs remain in ignored `.rws-local/diagnostics/`. Both diagnostic mounts were removed afterward; the remote test data remains.
- Next: package a reproducible, pinned experimental SSHFS build or patch procedure; correct RWS process lifecycle; validate editor saves, overwrite/no-replace behavior, startup failure, and normal unmount. Do not distribute an experimental binary as an official SSHFS release.

## Source references

- [Official SSHFS package and source mapping](https://github.com/macfuse/macfuse/wiki/File-Systems-%E2%80%90-SSHFS)
- [SSHFS 3.7.5 source](https://github.com/libfuse/sshfs/blob/sshfs-3.7.5/sshfs.c): `sshfs_init`, `sshfs_rename`, and main mount/daemonize ordering.
- [macFUSE library helper at the 5.4.0 submodule revision](https://github.com/macfuse/library/blob/00e9044ac2b6349eab5cd9dc8792e1a0544e9e54/lib/helper.c): `fuse_daemonize` forces foreground after mount starts.
