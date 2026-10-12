# Mish event appliance

primary: iot-embedded
phase: implementation

The user-approved architecture and milestone dependencies are in
`docs/roadmap.md`. Continue reversible implementation and GitHub validation
under that approval. Production deployment and publication remain subject to
the physical acceptance record for stable releases. The owner waived physical
acceptance as a publication gate for non-stable prereleases (such as alpha,
beta and release candidates). These use a prerelease version and GitHub's
prerelease flag; software and artifact-integrity gates still apply. Hardware/RF
status remains unverified until tested. Production deployment retains physical
acceptance, including deployment of a prerelease.

Use `nix develop` to execute application code and project scripts. GitHub
checks run natively on x86 and ARM and include the x86 appliance VM. Merge
queue validation uses `merge_group.checks_requested`.

Keep the existing merge-queue ruleset unchanged. Use native per-PR auto-merge
with the maintainer's GitHub session for now. App-based unattended admission
is deferred and opt-in; no App registration or credentials are required for
the current workflow. The optional App admission helper is manual-dispatch
only; required merge-group validation continues under Checks.
