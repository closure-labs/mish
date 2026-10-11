# Build once, reuse the exact result

Mish uses the public [mish Cachix cache](https://mish.cachix.org) for its Rust
package, dependencies, native check outputs, VM results, image closures and
pinned developer shell. Nix still evaluates each revision and requests its exact
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

Cachix's daemon pushes newly built intermediate outputs. After successful
validation, .github/scripts/cache-outputs.sh explicitly pushes Mish, the native
validation aggregate, the developer shell and any image roots. This also seeds
outputs restored from GitHub's cache, because Nix post-build hooks do not run for
substituted paths. No application state, credentials, operator backups or staff
data are build inputs or cache artifacts.

GitHub's supplementary store cache separates architectures and includes both
lockfiles, the source revision and workflow in its immutable key. Restore
prefixes reuse matching dependencies; a new revision can save newly built
outputs. Store snapshots are garbage-collected to a 5 GiB target before saving.
Cachix provides reuse across branches, workflows and developers.

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

An authorized operator can set MISH_CACHE_UPLOAD=true for this command after
configuring Cachix authentication locally. Upload failures fail the step.
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
The current shell already shares the package's build inputs and is cached by
the same mechanism. A devenv command interface could call these same checks
later without creating a second validation definition.

References: [Cachix action and push modes](https://github.com/cachix/cachix-action),
[explicit closure and shell uploads](https://docs.cachix.org/pushing),
[GitHub's Node 24 migration](https://github.blog/changelog/2026-09-23-node-20-is-no-longer-available-in-github-actions/).
