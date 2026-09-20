# Roadmap

The authoritative gap list is [FEATURES.md](FEATURES.md). It preserves the initial
product goal, evidence, acceptance criteria and dependencies. This roadmap gives
an order, not delivery dates or claims of feasibility.

## 1. Restore the central experience — P0

1. [RWS-001](FEATURES.md#rws-001): prove an architecture for remote command execution
   from project context. Explicit wrappers must not silently redefine the goal.
2. [RWS-004](FEATURES.md#rws-004), [RWS-005](FEATURES.md#rws-005),
   [RWS-007](FEATURES.md#rws-007): exact context, no silent local fallback, and
   executable acceptance tests are prerequisites to declaring transparency.
3. [RWS-002](FEATURES.md#rws-002): Delta without special agent instructions, if
   technically feasible. Test internal/noninteractive command paths separately.
4. [RWS-003](FEATURES.md#rws-003): terminal opened in a mount and `cd` into a mount
   enter the correct remote environment. A shell integration alone does not close RWS-002.
5. [RWS-006](FEATURES.md#rws-006): arbitrary remotely installed agents and tools
   work through those paths without a maintained executable whitelist.

If technical evidence rules out full transparency, document exact boundaries and
obtain a product decision. Do not relabel the explicit prototype as complete.

## 2. Reliable everyday macOS use — P1

- [RWS-008](FEATURES.md#rws-008): consistent mount/sidebar lifecycle.
- [RWS-009](FEATURES.md#rws-009) and [RWS-010](FEATURES.md#rws-010): fresh-Mac setup
  and management of multiple workspaces/hosts.
- [RWS-011](FEATURES.md#rws-011): complete app acceptance, beyond helper tests.
- [RWS-012](FEATURES.md#rws-012) and [RWS-013](FEATURES.md#rws-013): failures,
  recovery and editor saves, tested with disposable data.
- [RWS-014](FEATURES.md#rws-014) and [RWS-015](FEATURES.md#rws-015): signed release,
  real updates and a stable installed app. The native macOS app and local build
  replacement already exist; production delivery is not yet operational.
- [RWS-020](FEATURES.md#rws-020): keep scope, evidence and distribution documents aligned.

## 3. Previously recorded future direction — P2

- [RWS-016](FEATURES.md#rws-016): persistent remote sessions and cross-device reattachment.
- [RWS-017](FEATURES.md#rws-017): iOS files/commands/agents without an awake Mac relay.
- [RWS-018](FEATURES.md#rws-018): wider desktop and filesystem compatibility.
- [RWS-019](FEATURES.md#rws-019): clarify advanced port/transfer/watch/cache needs
  before choosing mechanisms or expanding scope.

These items do not authorize package installation, credential changes, destructive
failure tests or a remote service deployment. Existing results and limitations
remain recorded in [docs/validation.md](docs/validation.md).
