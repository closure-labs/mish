# Build once, reuse the exact result

Mish uses the public [mish Cachix cache](https://mish.cachix.org) for its Rust
package and selected native check outputs with their complete runtime
dependency closures. Nix still evaluates each revision and requests its exact
derivations. A successful check output can be substituted when its build
inputs match; changes to the application, test, toolchain or relevant inputs
require the corresponding new result. Cache reuse does not establish physical
radio, installation or field-pilot acceptance.

## GitHub setup and trust

The repository is configured with:

- Actions variable CACHIX_PUBKEY:
  mish.cachix.org-1:TVqZ5PamlcnYEpNEJtBz7xNmS7H5X/gXQXYVeDBD4FY=
- Actions secret CACHIX_TOKEN: a token permitted to push to the Mish cache.
  Prefer a token scoped to this cache. Keep it out of source and chat.

The shared .github/actions/nix composite pins the Nix installer, GitHub store
cache and Cachix action to full commit SHAs. Every JavaScript action in the
workflows uses Node 24; the Nix installer is a shell composite. Dependabot covers
actions inside the composite as well as workflows.

PRs and merge groups receive public read access without the push token. Main
pushes, maintenance runs on main, image builds on main, and version tag builds
can upload. Manual workflows on other branches stay read-only. Administrative
queue and draft-release jobs also stay read-only for Cachix.
There is no pull_request_target workflow or upload of artifacts from a PR into
a privileged follow-up job.

The pinned Cachix action always uses `skipPush: true`: it configures existing
CLI authentication but starts neither a post-build daemon nor a store-scan
upload. Setting only `useDaemon: false` would enable its store-scan fallback.
After successful validation, `.github/scripts/cache-outputs.sh` invokes the
explicit policy in `cache-policy.py`. It selects only `mish-control`, `schema`,
`cli`, and `channel-selection` from the validation manifest. These retain the
compiled Rust package and useful check results. Whole images, image
intermediates, VM results, the validation aggregate (which links all checks),
and the developer shell are excluded as roots. New checks require deliberate
selection before upload. Every native check, including the x86 VM checks,
continues to be required by the existing workflows.

Before pushing, Nix reads the selected roots' recursive **local** closure.
`--offline --store auto` uses the runner's local Nix store or local daemon,
so metadata queries work with both single-user and multi-user installations.
The policy validates canonical top-level store paths, complete reference edges,
unique metadata, and positive integer NAR sizes. It deduplicates shared
references and bounds the entire closure at 256 paths and 1 GiB of uncompressed
NAR bytes per invocation. This is a runtime-output budget; it does not limit
Rust compilation's temporary build inputs or the VM/image builds. Over-budget
closures skip the entire push and record `over-budget`, while complete image
and validation evidence is still checksummed and archived. Missing, ambiguous,
or failed metadata fails the step before upload. Push failures also fail.
`artifacts/cache-policy.json` records roots, deduplicated count, NAR bytes,
budgets and decision, including on read-only PRs.

No upstream-availability filter is used: Cachix receives complete closures,
including dependencies that might also exist upstream. This avoids treating an
upstream timeout or an unsigned availability claim as permission to omit a
required reference. Existing signed Nix substitution remains unchanged.
There are no live cache inventory requests in the policy.

The [October 26 Images run](https://github.com/closure-labs/mish/actions/runs/40329760189)
logged successful pushes of an ISO, an SD image, an intermediate filesystem
image and `inputDerivation-nix-shell`. Those logs establish the old upload
scope, not unique uploaded bytes or account storage growth. The new per-run
NAR measurement is not compressed transfer size or newly consumed storage.
This policy bounds future selected closures; it does not delete historical
entries, bound cumulative cache growth, or claim measured storage savings.
Mish uses `mish.cachix.org`, separately from FINite's cache.

GitHub's supplementary store cache separates architectures and includes both
lockfiles, the source revision and workflow in its immutable key. Restore
prefixes reuse matching dependencies; a new revision can save newly built
outputs. Store snapshots are garbage-collected to a 5 GiB target before saving.
Cachix provides reuse across branches, workflows and developers.

The manual Checks workflow has a cold-cache option. It skips GitHub's store
snapshot on fresh hosted runners while retaining the signed Cachix substituter.
Use this after seeding main to verify that packages and check outputs really
restore from Mish Cachix. Manual Checks remain read-only for cache uploads.

## Validation evidence

On either native Linux architecture:

    nix develop --accept-flake-config --command nix build .#mish-validation
    cat result/manifest.json

The mish-validation package depends on the compiled package and every native
flake check. Its manifest records the architecture, lockfile hash, output paths
and derivation identities. Its links retain the check outputs in the closure.
The x86 aggregate includes the booted appliance VM; ARM has its native package
and configuration checks. These are reproducible check results, not a formal
verification claim.

CI retains validation-manifest.json, roots, source revision, checksums and
evaluation/build logs. Image artifacts include the same evidence alongside
images and commissioning instructions. The checksum list is finalized after
adding that evidence. Required PR and merge-group jobs continue to run; the
manifest does not replace branch protection or claim that a cached VM result
booted again during the current run.

To produce local evidence without uploading:

    nix develop --accept-flake-config --command bash .github/scripts/cache-outputs.sh

This realizes all native checks, including the x86 VM. On a memory-constrained
workstation use the bounded command in [Contributing](contributing.md) for
targeted checks, and let GitHub CI produce the full validation aggregate.
Do not run this aggregate concurrently with other local evaluations/builds.

The workflow sets `MISH_CACHE_AUTHENTICATED=true` only when its existing trusted
job receives a token. An authorized local operator must explicitly set both
`MISH_CACHE_UPLOAD=true` and `MISH_CACHE_AUTHENTICATED=true` after configuring
Cachix authentication. Neither flag by itself enables an upload. Upload failures fail the step.
Running this against the same committed inputs on a matching architecture
produces the same check outputs used by GitHub Actions. Direct Cargo tests
outside Nix derivations do not produce these reusable validation outputs.

## Developer environment

The existing nix develop shell pins Cargo, Rust, SQLite, Nix, GitHub CLI and
Cachix. The flake declares the public substituter and signing key; accept its
configuration when prompted, or pass --accept-flake-config. On a multi-user Nix
installation the daemon must also allow that cache/key. An untrusted user may
see an ignored-substituter warning and build locally; CI's installer configures
the daemon explicitly. No host trust settings are changed by this repository.

Devenv offers [Cachix integration](https://devenv.sh/binary-caching/), but sharing
validation results comes from building and uploading the same Nix checks.
The current shell shares the package's build inputs; its full closure is not
selected for Cachix upload. A devenv command interface could call these same checks
later without creating a second validation definition.

References: [Cachix action and push modes](https://github.com/cachix/cachix-action),
[explicit closure and shell uploads](https://docs.cachix.org/pushing),
[GitHub's Node 24 migration](https://github.blog/changelog/2026-09-23-node-20-is-no-longer-available-in-github-actions/).
