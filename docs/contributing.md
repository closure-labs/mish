# Contributing

Use the committed lockfile and the same flake checks as CI. Changes should
explain the concrete user behavior, validation performed and material remaining
limits. Keep simulated transport, built artifacts, VM behavior and physical
qualification separate in reports.

```bash
nix develop
PYTHONPATH=apps/vfctl/src python -m unittest discover -s tests
nix run .# -- internal-check-schemas --schemas schemas --examples examples --catalog catalog/vestaforge-catalog.json
nix flake check --no-build --all-systems
nix flake check
```

On Linux, `nix flake check` builds native checks; the x86 VM integration check
requires virtualization support. ARM packages and SD images use an ARM runner
or builder. A non-Linux development host can evaluate all systems, then use
native Linux builders to realize outputs. See [building-alpha.md](building-alpha.md).
Format Nix files with `nix fmt`; CI runs the formatter in check mode and must
never rewrite contributions. Add meaningful regressions for authorization,
privacy, durability and transport state changes.

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
builds pinned revisions and publishes checksums/manifests. Maintenance runs explicit native x86 and ARM checks before proposing
reviewed lockfile updates and never auto-merges. PRs created with `GITHUB_TOKEN`
do not automatically trigger PR checks; dispatch `ci.yml` manually on the
maintenance branch to produce the required CI evidence for branch protection. Version tags create draft
releases; hardware evidence is required before publishing them. Configure
repository branch protection to require CI after the workflows are first run;
workflow YAML alone cannot set repository merge policy.
