# Rust architecture decision

Date: 2026-10-11. Status: selected for implementation under the user's request
to replace the Python runtime with a Rust-native package.

## Baseline

The previous implementation couples JSON-shaped storage methods, HTTP handlers
and a background radio thread. Rewriting each method in Rust would preserve
that structure while missing Rust's type and ownership guarantees.

## Overkill comparison

This review used Codex with the Overkill workflow, operator costs and current
primary sources. Complexity and first-commit estimates below are planning
judgments for a focused spike, not completion or performance promises.

| Approach | Complexity | First commit | Operations burden | Specialist difficulty | When it earns its cost |
|---|---|---|---|---|---|
| Typed domain, ports/adapters and bounded Tokio actors | 4/10 | 2–4 days | One process; explicit task/queue lifecycle | Moderate Rust ownership/async knowledge | Current coordinator: concurrency, radio partial failure and private/public boundaries |
| CQRS with immutable events and replayed projections | 6/10 | 1–2 weeks | Event versioning, replay, projection recovery and retention | High data-model expertise | Multiple changing views or an audit/reconstruction requirement beyond the current journal |
| Supervised actor framework with Ractor | 5/10 | 3–7 days | Supervision policy, mailbox limits and restart contracts | High actor-system expertise | Many independently failing adapters and a substantial supervision tree |
| Model-based verification of timed delivery and recovery | 7/10 | 1–2 weeks | Low runtime cost; continuing model/test maintenance | High verification expertise | Protocol changes, takeover or concurrency make retry/ACK rules difficult to reason about |

### Typed domain and bounded actors

Commands use validated text and typed message kinds, roles, transmission
transitions and acknowledgement sources. Public announcements have their own
response type, which contains no staff fields. A bounded coordinator mailbox
connects async request handlers and radio tasks to a dedicated storage thread;
successful replies follow transaction commit. Native serial I/O uses bounded
framing and explicit packet identifiers.

This adds explicit ownership, overload behavior and exhaustive state handling.
It develops type-driven API design, async lifecycle management and transactional
ports/adapters. The current single coordinator already benefits from these
properties. [Rust enums](https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html),
[Tokio channels](https://tokio.rs/tokio/tutorial/channels) and
[blocking workload guidance](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)
describe the building blocks.

### CQRS and event sourcing

Store immutable events and rebuild separate staff/public projections from
them. This extends replay and auditability beyond mutable journal rows, but
introduces event schema evolution and makes seven-day retention and deliberate
event wipe more involved. It develops aggregate design, event versioning and
projection recovery. [Rust CQRS](https://github.com/serverlesstechnology/cqrs)
is an implementation reference; its repository has no GitHub releases, so
adoption would require a separate crate/version and maintenance audit.

### Supervised actors

Ractor supplies actor lifecycle and supervision primitives rather than a
small application-specific task tree. It becomes useful if independent
transports and services multiply. It develops failure isolation, supervision
strategies and actor messaging. Its
[v0.16.5 tag](https://github.com/slawlor/ractor/tags) dates to August 2026;
the [project](https://github.com/slawlor/ractor) explicitly says its distributed
cluster component is not production-ready. The first coordinator does not
need that distributed surface.

### Model-based verification

Compare generated delivery, expiry, disconnect and restart sequences against
a reference state machine; later model takeover in a formal specification.
This develops invariant design, shrinking counterexamples and concurrency
reasoning. [Proptest](https://github.com/proptest-rs/proptest) describes itself
as feature-complete with passive maintenance;
[state-machine testing](https://proptest-rs.github.io/proptest/proptest/state-machine.html)
and [Loom](https://github.com/tokio-rs/loom) provide distinct testing techniques.
Property tests of input and transition boundaries belong in this implementation;
formal verification and takeover remain separate future gates.

## Selected implementation

- One Rust library and executable package, `mish`, with explicit domain,
  coordinator, SQLite, HTTP, CLI, file-content and radio modules.
- Axum and Clap are input adapters. Serde converts typed request/response
  structures at boundaries; JSON maps are reserved for legacy content/config
  formats rather than being the communication model inside the coordinator.
- SQLite remains the durable authority. A dedicated thread handles a bounded
  command mailbox; synchronous database work does not occupy Tokio workers.
  CLI maintenance uses the same repository directly while the service is
  stopped. SQLite transactions still guard concurrent local access.
- Radio submission is a separate Tokio task. Claim before write; completed
  USB write, RF transport confirmation and human acknowledgement stay distinct.
  Ambiguous writes and snapshot queue entries remain uncertain.
- Keep the existing schema and JSON/API contract where they encode useful
  behavior. Preserve authentication, enrollment, retention, backup/restore,
  explicit announcements and development-only content operations.
- Use native serial I/O and independently implemented minimal wire fields from
  the [Meshtastic client protocol](https://meshtastic.org/docs/development/device/client-api/).
  No Python subprocess or imported GPL-generated SDK/schema code is required.
- Pin Cargo dependencies and Nix inputs. Run Rust format checks, Clippy,
  behavioral/property tests, native x86/ARM checks and the booted VM through
  `nix develop` and the same flake checks used by GitHub.

This decision changes implementation structure, not the hardware evidence.
Physical USB/RF, Pi 4 commissioning and field rehearsal remain unverified.
