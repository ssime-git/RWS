# RWS native SMB mounts — design

Date: 2026-09-24
Status: proposed design; no host provisioning or mount backend change authorized by this document alone.

## Goal

Replace the macOS macFUSE/FSKit + SSHFS filesystem path with macOS's built-in SMB client while preserving the existing local mount paths exactly:

- `ssime-omarchy` → `/Volumes/RWS-ssime-omarchy`
- `razer-1` → `/Volumes/RWS-razer-1`

The product outcome is one signed/notarized RWS app/CLI distribution that can diagnose prerequisites, prepare a Linux host through a deliberate user-approved action, mount/reconnect on login, and report actionable state. A successful command exit or an SSH connection alone is not mount acceptance.

## Evidence and decision

The current installed design uses macFUSE/FSKit + SSHFS. The live Mac is macOS 27.0 arm64; after the recent reboot, `launchd` loads `io.rws.mounts` with a 30-second run interval, Razer is mounted, and Omarchy is disconnected. This is evidence that the existing reconnect mechanism does not reliably restore both volumes after reboot. No global FSKit restart or remote-host mutation is in scope here.

Choose native SMB as the next prototype because it removes the FUSE provider and FSKit mount-table from the client mount path, and macOS has a supported SMB client. Samba exports are scoped to a named share and obey underlying Unix permissions. The user must be told that Linux hosts need Samba service/share configuration and SMB credentials; SSH credentials cannot simply be reused as SMB credentials. Sources: [Apple Login Items support](https://support.apple.com/en-am/guide/mac-help/mh15189/mac), [Samba share configuration](https://www.samba.org/samba/docs/4.20/man-html/smb.conf.5.html).

A Rust implementation of a FUSE protocol provider would retain the FUSE/FSKit dependency surface and would not remove the demonstrated collision risk. Third-party virtual-drive apps do not meet the fixed-path requirement unless they explicitly support mounting at these `/Volumes` paths and reliable login recovery.

## Architecture

1. **Transport and setup**: keep OpenSSH for host reachability and bootstrap. Add a read-only `rws smb doctor` that reports SSH reachability, OS/package manager, Samba presence/service state, share reachability, credentials availability, target path ownership, and current mount identity. It must not change a host.
2. **Explicit host provisioning**: `rws smb setup --workspace NAME --plan` prints a complete, distro-aware plan and exact changes. Applying it requires an explicit interactive confirmation per host. The first remote-provisioner target is a disposable Linux VM only; do not infer production distro/firewall/service-manager details from SSH aliases. Before expanding support, publish and test an explicit distro/service/firewall matrix. The transaction installs/enables Samba only as needed, adds one narrowly scoped share for the configured remote root, creates/configures a dedicated SMB authentication secret without changing the Unix/SSH password, and restricts TCP/445 to the tailnet. It validates the effective config and reachability from both sides. The rollback manifest stores exact prior bytes/metadata and hashes for every RWS-owned change; rollback is compare-and-swap and refuses if any managed file, service state, or firewall rule has drifted since setup. Never rewrite unrelated Samba config or remove unrelated rules. Never expose SMB on public interfaces. If a distro's package/service/firewall state is unknown, stop before mutation and show the missing prerequisite.
3. **Mac mount feasibility gate (phase A)**: first prototype Apple NetFS (`NetFSMountURLSync`) as the native SMB entry point, because its API accepts a target mount path and separate credentials; validate the API on the installed macOS 27 SDK/runtime. Prove it can reliably create/use the exact existing `/Volumes/RWS-*` targets from a per-user LaunchAgent without macFUSE, root-owned pre-created directories, or changing paths. `mount_smbfs` by itself is not accepted as sufficient until mountpoint creation and permissions are proven. If a privileged helper is required, stop after phase A and present a separate design for its least-privilege operations, Apple Service Management approval, signing/notarization, upgrade/uninstall, and recovery before implementing it. Store SMB secrets in the unlocked user login Keychain; startup while locked, missing secret, and Keychain prompt behavior must fail visibly without hanging. Never put passwords in command arguments, logs, config, or LaunchAgent plist. Retain SSH aliases for SSH execution independently of the SMB hostname.
4. **Login and recovery**: only after the feasibility gate, one per-user LaunchAgent waits for Tailscale interface and host resolution, then performs bounded SMB-port and share probes before reconciling configured mounts. Use bounded exponential backoff with jitter and per-volume serialization; 30-second polling alone is not a substitute for readiness checks. Verify server/share identity and a short bounded read before declaring connected. Preserve a responsive active mount; never globally restart FSKit, force-unmount an unknown/busy volume, or drop a healthy peer while repairing another. Log one concise attempt/final state per cycle and redact secrets. Keychain-unavailable or locked-session behavior must be tested explicitly.
5. **App and CLI**: app has per-workspace Doctor, Setup/Review Plan, Connect, Disconnect, and Repair actions. Setup is explicit; Connect/Repair never provision remote hosts. Status distinguishes SSH healthy, Tailscale/DNS ready, SMB share reachable, actual mount present, and verified mount I/O; checks must be bounded and must say when the sandbox or OS prevents a conclusion. Uninstall removes only RWS-owned LaunchAgent/local state and offers (does not silently perform) remote share rollback. A production one-artifact experience depends on configuring Developer ID signing and notarization; current project validation records that production signing is not configured, so the prototype must not claim zero-install distribution until this external dependency is resolved.
6. **Migration**: keep the current backend/config intact until SMB mounts pass acceptance. No automatic cutover or deletion of macFUSE/SSHFS. After explicit user approval, migrate one workspace at a time, keep original `/Volumes` path, and provide a one-click rollback to the previous backend. Do not run both backends at the same path simultaneously.

## Security and operational boundaries

- Remote provisioning is a separately confirmed action. This work does not make any change on Omarchy or Razer.
- Bind SMB access to Tailscale/private interfaces; do not open TCP/445 to the Internet.
- Generate SMB-only credentials, store them in Keychain on macOS and Samba's credential database remotely, and never reuse or reveal the Unix account password.
- Provision only the configured remote root and the minimum required account permissions. Preserve existing Unix ownership/modes as the actual authorization boundary.
- Treat timeouts as bounded for SSH, readiness polling, and health probes. SMB I/O on an unresponsive server may still block at the OS/filesystem layer; surface that limitation and never describe client-side deadlines as cancellation of kernel I/O.
- Preserve the dirty checkout's existing `README.md` and `docs/connection.md` edits.

## Acceptance criteria

1. Static/unit coverage for plan generation, shell argument safety, redaction, per-host state, retry policy, mount identity checks, and rollback-manifest validation.
2. Linux disposable VM acceptance for each explicitly supported distro: setup plan is read-only; confirmed setup installs a restricted share; rollback restores only unchanged RWS-managed prior service/share/firewall state without deleting user data. Concurrent/manual changes must make rollback stop safely.
3. macOS acceptance with disposable Linux share: prove mount and unmount at the exact `/Volumes/RWS-*` target as a normal user, including directory creation and reboot recovery, without macFUSE. Independently verify list/read/write, atomic rename/replace, symlinks, executable bits, case handling, NFC/NFD Unicode aliases, xattrs where the toolchain relies on them, advisory locks, and the editor save pattern used by RWS. Document a compatibility contract and unsupported operations. Recover from network loss without affecting the other workspace.
4. Reboot/login test: both paths reappear automatically, in either host response order, after Tailscale and DNS readiness. Verify actual filesystem identity and remote content, not just the mount directory or app state. Repeat multiple cycles.
5. App distribution acceptance: clean-machine install from one signed/notarized app artifact, guided permissions and host setup, no manually copied scripts/source code, safe update with mounted-volume guard, and uninstall/rollback.
6. Performance comparison against current SSHFS on the same network and same test tree: cold connect latency, repeated directory listing, small-file reads/writes, and large sequential transfer. Publish measurements and their limits; do not assume SMB is always faster.

## Phased implementation

A. Read-only local feasibility: inspect the macOS NetFS contract and signature setup; prototype target-path mount/unmount behavior with a disposable share if one is already available. No production remote mutation and no migration. Stop and return with evidence if exact `/Volumes` mounting requires a privileged helper.
B. After approval of a revised design, reversible Samba provisioner for disposable Linux VMs, initially limited to an explicit tested distro matrix, with plan/apply/rollback.
C. After separate approval, native macOS mount and serialized login reconciler at unchanged paths; solve Keychain access and mountpoint ownership before integration.
D. RWS app UI, signing/notarization, updates, diagnostics, and migration/rollback workflow.
E. End-to-end Mac plus both real hosts only after explicit host-by-host setup approval.

## Review findings / gates before backend implementation

The local macOS SDK exposes NetFS and `kNetFSMountAtMountDirKey`, but this header alone does not prove that a user LaunchAgent can mount into `/Volumes` or create the target directory. Phase A must establish the exact privilege/path contract experimentally before choosing whether a helper is needed. The project also has no configured production Developer ID/notarization credentials (`docs/validation.md` records this limitation).

The two production Linux hosts' distributions, firewall policy, existing Samba state, and whether SMB packages are available remain unestablished. No provisioning command may run against them until a read-only inventory has been reviewed and the user explicitly approves each host. The next deliverable should be the phase-A feasibility evidence; this document does not yet authorize backend implementation or host changes.
