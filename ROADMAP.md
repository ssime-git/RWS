# Roadmap

This roadmap records direction, not delivery dates or claims of available functionality.

## 1. macOS feasibility prototype — immediate priority

- Existing SSH host and one remote workspace.
- Mount, edit, save, and verify remotely.
- Explicit remote command execution and interactive shell with correct path mapping.
- Clear diagnostics for unavailable hosts and missing local prerequisites.
- Validate a remote coding-agent CLI and a second host after the first workflow works.

## 2. Usable CLI and stronger reliability

- Host/workspace configuration and lifecycle management.
- Automated checks for path mapping, execution, and failure behavior.
- Document measured mount, cache, and disconnection limitations.
- Add forwarding, refresh, and transfers incrementally as justified by actual use.
- Reproducible contributor setup, CI, and release documentation.

## 3. Persistent remote sessions

- Choose a session mechanism after prototype feedback.
- Keep commands and agents running through client disconnections.
- Reattach from another authorized device.
- Make any remote component installation explicit.

## 4. Graphical desktop application

- macOS first, using the established core.
- Windows and Linux later, with platform-specific integrations.

## 5. iOS application

- Independent access to remote hosts, with no Mac relay required.
- Browse and transfer remote files.
- Launch, monitor, and reattach to persistent remote commands and coding-agent sessions.
- Define mobile file caching, authentication, and integration behavior separately.

Stages 4 and 5 remain future work. Their requirements inform core boundaries without delaying the first prototype.

## Next acceptance steps after the first Finder corrections

In priority order, using disposable data and recording results in `docs/validation.md`:

1. **Editor save workflow:** open a file through the mount in a local editor, change/save/reopen it, and compare remote bytes. Include Unicode names, replacement saves, and error reporting. Finder folder creation and copy/paste are verified; this editor workflow is not.
2. **Mount and Finder lifecycle:** repeat mount/browse/write/unmount cycles, verify process cleanup, and investigate stable sidebar identity. Add observed failures to the live acceptance checks. Resolve/document the outstanding diagnostic OS processes before broader fault testing.
3. **Controlled failure behavior:** on a dedicated remote test directory, test interrupted connections/writes and recovery. Measure delays and surface errors; do not perform these tests on valuable Documents data.
4. **Remote development workflow:** validate a coding-agent CLI already installed/authenticated on the remote host, working-directory mapping, PTY resize and Ctrl+C. Then test a second host and a fresh Mac using the repository skill.
5. **Contributor readiness:** persistent toolchain/setup instructions, reviewed anonymized README screenshots, license selection and release prerequisites. Existing macOS/Linux CI covers the Rust suite, not live macFUSE/Finder acceptance.

Desktop and iOS remain deferred until this core workflow and its failure behavior are dependable. These steps do not authorize future package installation, account authentication, fault injection, or application-framework choices by themselves.
