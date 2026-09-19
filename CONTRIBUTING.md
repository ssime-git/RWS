# Contributing

RWS is an early prototype. Start with [the roadmap](ROADMAP.md) and [prototype guide](docs/prototype.md). Keep changes focused on the first working remote-workspace workflow.

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Core tests run without network access or an SSH server. CLI tests use temporary configuration and, where necessary, a temporary SSH executable to test process boundaries. Actual filesystem integration requires macOS, SSHFS/macFUSE, and your own test host; describe those manual checks separately in a pull request.

Keep workspace modeling, path resolution, and transport/process integration separate. Add regression coverage for path confusion, quoting, configuration corruption, and errors when changing these areas.

Do not submit private host configuration, credentials, personal paths, terminal logs, or real project data. Use fictitious examples. License selection is pending; the repository has not yet been published as an open-source release.
