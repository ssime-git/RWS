# Documentation

[← Project overview](../README.md)

Start with the path that matches your goal. User guides describe the current
implementation; the feature backlog describes the desired product, not shipped behavior.

## Use RWS

| Guide | What you will find |
| --- | --- |
| [Install and open the app](install.md) | Existing bundle, development download, prerequisites, first workspace |
| [Troubleshooting](troubleshooting.md) | Symptom → check → next action |
| [Finder behavior](finder-macos.md) | Favorites, remounts, manual pinning and file checks |
| [CLI reference and quick start](prototype.md) | Register, connect, exec, shell and agents |
| [Connection and Delta](connection.md) | Receipts, shortcuts, current instruction-based forwarding |

## Build and understand

| Guide | What you will find |
| --- | --- |
| [Developer quick start](development.md) | Toolchain, build, test, run and rebuild |
| [Architecture](architecture.md) | Conceptual diagrams, current execution boundaries, code map |
| [Contributing](../CONTRIBUTING.md) | Focused changes, validation and privacy |
| [App internals and releases](macos-app.md) | Config discovery, updater, signing and release secrets |
| [Swift package](../macos/README.md) | App-specific development details |
| [Experimental SSHFS](sshfs-fskit.md) | Reproducible patched build and filename contract |

## Scope and evidence

- [Features](../FEATURES.md) and [GitHub issue index](https://github.com/ssime-git/RWS/issues/1).
- [Roadmap](../ROADMAP.md): order of work, including the missing transparent execution.
- [Validation journal](validation.md): dated evidence, failed attempts and later corrections.
- [Decisions](decisions.md): historical scope and the correction restoring the original goal.
- [FSKit diagnosis](fskit-debugging.md): historical investigation, not an installation recipe.
- [Initial CLI plan](implementation-plan.md) and [historical implementation plans](superpowers/plans/):
  development records, not current user instructions.

## How to read status claims

**Implemented** means the code exists. **Validated** names an actual tested route
and environment. **Planned** means an issue remains open. Successful CLI tests do
not prove that an editor launches remotely, that an update can be installed, or
that a new Mac is ready to mount.

The desktop bundle targets macOS 13+ for compilation; this is **not** the support
matrix of its FSKit backend. The recorded live mount setup used Apple silicon,
macOS 27.0, macFUSE 5.4.0 and the RWS `3.7.5-rws-fskit3` SSHFS build. Consult the
[validation journal](validation.md) before generalizing those results.

## Documentation maintenance

Use fictitious hosts and paths in examples. Keep screenshots, logs, credentials
and real configuration in ignored `.rws-local/` until reviewed for publication.
Do not publish a screenshot of an old UI as if it showed the current build.
Every public screenshot must state what it demonstrates; diagrams must distinguish
current components from proposed architecture.
