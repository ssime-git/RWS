# FSKit corrections implementation plan

Goal: make the diagnosed FSKit rename and mount lifecycle work reproducibly without modifying system SSHFS.

Architecture: build pinned official SSHFS sources with only the negotiated extended-rename capability disabled. Select an explicit binary with RWS_SSHFS. RWS starts SSHFS in foreground mode in a separate process group with null stdin and private file-backed diagnostics, checks mount readiness while the child lives, and kills/reaps a failed or timed-out child. Normal OS unmount ends SSHFS. No persistent RWS daemon or PID-based kill is introduced.

- Add failing regression tests for foreground arguments, custom binary selection, early exit and timeout cleanup, readiness while child stays alive.
- Implement the lifecycle module and integrate into mount; preserve default backend and dry-run behavior.
- Add a pinned-source build script with content verification and exact patch guard; keep outputs under ignored .rws-local.
- Run Rust tests, formatting and Clippy, build the patched dependency and exercise real mount/read/write/rename/replace/unmount.
- Review the change, update setup/validation documents, and commit validated work.

Startup is noninteractive: SSH credentials/host trust must be prepared in advance. File metadata readiness checks depend on OS responsiveness. Logs are private and retained for diagnosis. Mount failure must not be reported as success merely because a child exits zero. Generic local editors, connection recovery, and concurrent remote edits remain separately tested limitations.
