# Contributing

Mish contributions use AGPLv3 only (`AGPL-3.0-only`); see [LICENSE](../LICENSE).
Keep the repository and Rust crate license copies identical when updating
license text. Preserve third-party license notices.

The [CI qualification policy](ci-qualification.md) documents required checks,
change qualifiers, workflow dependencies and the separate physical gates.

Use the committed lockfile and the same flake checks as CI. Changes should
explain the concrete user behavior, validation performed and material remaining
limits. Keep simulated transport, built artifacts, VM behavior and physical
qualification separate in reports.

The flake uses [Den composition](den-architecture.md). Add reusable service
features to its feature aspects, select behavior through mode aspects and keep
board-specific settings in architecture aspects. Host declarations select those
aspects. Preserve exported package/module/image names and run the composition
check when changing their wiring. The [product status](product-status.md) records
the next acceptance gates.

The [public binary cache](binary-cache.md) shares exact package, check and
developer-shell outputs. Use --accept-flake-config when accepting its signed
substituter. PRs read the cache; trusted main and release jobs upload.

On the workstation, serialize Nix evaluation/builds with the bounded wrapper:

    bash scripts/local-nix.sh build .#checks.x86_64-linux.workflows --accept-flake-config

The launcher enters nix develop inside its scope, shares one lock across worktrees,
permits one Nix build job and
one build core, and bounds the evaluator/client to 2 GiB with swap disabled.
Nix daemon builds run outside that user scope. Run full VM/image and
all-architecture gates on GitHub runners when workstation memory is tight;
do not start concurrent local checks. Set CARGO_BUILD_JOBS=1 for direct Cargo
commands. Shell startup is included in the lock and memory limit.

```bash
nix develop --command cargo fmt --manifest-path apps/mish/Cargo.toml --all -- --check
nix develop --command cargo clippy --manifest-path apps/mish/Cargo.toml --all-targets --locked -- -D warnings
nix develop --command cargo test --manifest-path apps/mish/Cargo.toml --all-targets --locked
nix develop --command cargo run --manifest-path apps/mish/Cargo.toml --locked -- internal-check-schemas --schemas schemas --examples examples --catalog catalog/mish-catalog.json
nix develop --command nix flake check --no-build --all-systems
nix develop --command nix flake check
```

On Linux, `nix flake check` builds native checks; the x86 VM integration check
requires virtualization support. ARM packages and SD images use an ARM runner
or builder. A non-Linux development host can evaluate all systems, then use
native Linux builders to realize outputs. See [building-alpha.md](building-alpha.md).
Format Nix files with `nix fmt`; CI runs the formatter in check mode and must
never rewrite contributions. `apps/mish/Cargo.lock` pins the Rust dependencies;
Nix packages the application with `buildRustPackage` and runs formatting,
Clippy and Cargo tests before accepting the package. Add meaningful regressions
for authorization, privacy, durability and transport state changes. The
[Rust architecture decision](rust-architecture.md) describes the typed domain,
bounded coordinator and native serial adapter.

Python is retained for repository CI automation tests, not the appliance
runtime. Test that helper separately when changing queue automation:

```bash
nix develop --command python -m unittest discover -s tests -p test_merge_queue.py
```

## jj and GitHub PRs

This repository has a colocated jj/Git checkout. Use a bookmark for each PR and
keep unrelated work in separate changes.

```bash
jj status
jj new main
# Edit and run relevant checks.
jj describe -m 'Preserve queued alerts across reconnects'
jj bookmark create alert-reconnect -r @
jj git push --bookmark alert-reconnect --allow-new
gh pr create --head alert-reconnect --base main
```

If `main` is not a local bookmark, fetch first with `jj git fetch` and start
from `main@origin`. When making follow-up changes, move the bookmark to the
reviewed change using `jj bookmark set alert-reconnect -r @`, then push it.
Resolve conflicts locally and rerun affected checks before requesting review.
A normal Git branch/PR workflow is also supported.

CI checks on PRs have read-only permissions and no secrets. Image automation
builds pinned revisions and publishes checksums/manifests. Maintenance runs
explicit native x86 and ARM checks before proposing a draft lockfile update.
Review its inputs before marking it ready; that action triggers required PR
Checks. Checks created by `GITHUB_TOKEN` may need workflow approval.

Main now requires both native Checks jobs and merge-queue validation. The
[automatic queue workflow](merge-queue.md) admits ready PRs from repository
branches after successful current-head Checks; draft maintenance proposals
remain pending operator review. Queue admission uses trusted main code and
keeps PR checks read-only. Version tags create draft releases; hardware
evidence is required before stable publication. The owner waived the physical
publication gate for non-stable prereleases; software and integrity gates still
apply. Workflow YAML alone does not set
repository merge policy: the active ruleset configuration is also recorded
in `.github/merge-queue-rules.json`.
