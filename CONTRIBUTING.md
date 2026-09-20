# Contributing to RWS

[Overview](README.md) · [Documentation](docs/README.md) · [Developer setup](docs/development.md)

RWS is a development preview. Read the [architecture](docs/architecture.md) and
[linked feature backlog](FEATURES.md) before changing behavior. The original goal
of transparent remote execution is still open; explicit SSH launchers must not be
presented as completing it.

## Start and test

Follow [Build from source](docs/development.md) for prerequisites, a packaged app
and rebuild instructions. Select the checks relevant to your change:

| Change | Checks |
| --- | --- |
| Rust CLI/core | `cargo fmt --check`, `cargo test --locked`, `cargo clippy --locked --all-targets -- -D warnings` |
| Swift app | `swift test --package-path macos`; assemble with `scripts/build-macos-app.sh development` |
| Packaging/releases | `python3 -m unittest discover -s tests/release` and development packaging |
| Documentation | `python3 scripts/check-docs.py`, review examples against CLI/UI, inspect rendered diagrams and images |

Ordinary tests require no remote host. Live Finder/mount tests need explicit
opt-in and a disposable workspace; some disconnect and reconnect it. Read their
source and the [validation journal](docs/validation.md) first. Never infer a
successful live workflow from unit tests or a mounted-volume icon alone.

## Propose a focused change

- Link the issue and explain the visible before/after behavior.
- Keep workspace modeling, path routing and transport/process boundaries separate.
- Cover meaningful failure cases when changing quoting, path ownership,
  configuration, mount identity or local/remote execution.
- State which checks ran, their environment and what remains untested.
- Update the relevant user guide and scope/evidence claims together.

For bug reports, include the app revision, OS/dependency versions, entry point,
expected behavior and sanitized error. Use [troubleshooting](docs/troubleshooting.md)
to distinguish file access from command execution.

## Privacy and documentation

Use fictitious hosts and paths. Keep personal configuration, screenshots and logs
in ignored `.rws-local/`. Never submit SSH credentials, real project data or private
hostnames. Review images for personal information and label historical captures
accurately. Local link checks do not verify external sites or Mermaid rendering.

Historical plans and diagnostic logs preserve earlier findings; new-user
instructions belong in the current guides. Keep the [documentation map](docs/README.md)
updated so readers do not need to follow the development history to install RWS.

## Publication status

License selection is pending. Source availability alone is not an open-source
license. A project license and third-party distribution review are needed before
publishing a licensed open-source release. Production signing and update delivery
also remain separate [release-readiness work](https://github.com/ssime-git/RWS/issues/14).
