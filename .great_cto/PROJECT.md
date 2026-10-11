# Mish event appliance

primary: iot-embedded
phase: implementation

The user-approved architecture and milestone dependencies are in
`docs/roadmap.md`. Continue reversible implementation and GitHub validation
under that approval. Production deployment and publication remain subject to
the physical acceptance record; immutable releases stay drafts until then.

Use `nix develop` to execute application code and project scripts. GitHub
checks run natively on x86 and ARM and include the x86 appliance VM. Merge
queue validation uses `merge_group.checks_requested`.
