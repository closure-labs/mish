# Snapshot recovery QA record — 2026-10-11

Baseline: `fabce7e`, validated by native GitHub x86 and ARM Checks.
Scope: M3 software recovery within the approved single-coordinator design.

## Defect and resulting behavior

A live backup can contain queued records that the original coordinator sends
after backup capture. Starting directly from that snapshot previously replayed
those records. A read-only project audit reproduced that behavior through
`nix develop`.

`mish restore --input <backup>` now restores into fresh state, validates a
standalone snapshot and installs a staged `0600` database atomically without
overwriting any event database. Snapshot `queued` and `sending` records become
`uncertain`; staff must review them before deliberately creating replacements.
Normal restart retains its pending queue. Source SHA-256, restore time and
quarantined count are recorded in `recovery_history` and CLI output.

## Verification

| Gate | Result | Evidence |
|---|---|---|
| Regression before implementation | Failed as expected | CLI had no restore operation; audit independently reproduced snapshot replay |
| Python suite | 38 passed | `nix develop --command env PYTHONPATH=apps/mish/src python -m unittest discover -s tests` |
| All-system evaluation | Passed | `nix develop --command nix flake check --all-systems --no-build --accept-flake-config` |
| Native x86 checks | Passed | `nix develop --command nix flake check -L --accept-flake-config` |
| Booted appliance recovery | Passed | `tests/appliance.nix`: backup, original send, fresh restore, no replay, staff auth/ACK/public history, new sends and trusted HTTPS |
| Independent security review | Approved | Ordered schema metadata and uniqueness checks resolved the compatibility finding |
| Independent QA review | Passed | Revised recovery regressions passed; no remaining blocking findings |
| Native GitHub x86 and ARM checks | Passed on `c91498b` | [Checks run 38172407182](https://github.com/closure-labs/mish/actions/runs/38172407182) |

Eight recovery tests cover private/public history, browser/radio ACK
provenance, source preservation, permissions, repeated recovery history,
corrupt/missing/incompatible input, existing database/sidecars, concurrent
destination creation and failed installation. Schema regressions include
extra/generated/reordered columns, triggers and missing deduplication
constraints. The checker derives its reference from the runtime schema.

CI records evaluation and native-check outcomes separately, preserves failure
status through log capture, and uploads logs per architecture/revision/attempt.
PR and merge-group checks use the same flake checks.

## Limits and next gate

Restore is a privileged offline operation using an operator-produced backup
and a private state directory. Stop the service and preserve the old state
before restoring. Keep one coordinator active. It restores only records in
the snapshot and requires separate configuration/CA/radio-key recovery.

The merged recovery revision `9c5d002` passed x86 VM/installer and Pi 4 SD image
realization in [Images run 38173136571](https://github.com/closure-labs/mish/actions/runs/38173136571).
Physical M2–M5 acceptance remains separate. No release was published and no
physical commissioning, USB/RF, power-loss or eight-hour rehearsal result is
claimed by this software gate.
