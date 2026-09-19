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
