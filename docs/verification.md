# Implementation verification evidence

These results concern the implementation working tree on 2026-10-11. Release
automation must repeat checks against its tagged revision and record artifact
checksums. Physical acceptance remains separate.

| Evidence | Result | Scope |
|---|---|---|
| Python application tests in `nix develop` | 30 passed | Legacy behavior, event authorization/privacy/durability and radio simulation/failure regressions |
| All-system flake evaluation | Passed | Declared module/package/check expressions evaluate |
| Native Nix checks | Final run passed on x86 | Package, schema, formatting, CLI and service/module checks |
| x86 `vm-smoke` application test | Final run passed | Two staff sessions, messages, alert ACK, restart durability and protected/public reads in a booted appliance |
| Caddy local HTTPS in VM | Passed | Generated root certificate at `/var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt`; HTTPS verified with that trusted CA, without bypassing certificate validation |
| x86 QEMU appliance | Built | VM output realized; physical installation unverified |
| x86 installer ISO | Built | Final application/catalog snapshot realized, about 1.5 GB; physical installation unverified |
| Pi 4 SD image | Native ARM CI build passed on `a42ca31` | [Images run](https://github.com/closure-labs/mish/actions/runs/38170625479) also built the x86 VM and installer; physical boot and radio qualification remain unverified |
| Native GitHub x86 and ARM checks | Passed on `fabce7e` | [Checks run](https://github.com/closure-labs/mish/actions/runs/38171282570) validated the combined Dependabot updates and merge-queue trigger |
| Physical Meshtastic USB/RF | Unperformed | Requires mini-PC, two radios and WAN-disconnected acceptance |
| Eight-hour rehearsal | Unperformed | Requires resource/RF measurements and operator intervention record |

The automated checks establish behavior in their stated test environments.
They do not qualify hardware, venue coverage, power-loss recovery or event
operation. Use [acceptance.md](acceptance.md) to record those results.

The locally built installer is:

```text
/nix/store/bg485f8qvpflwhjph70wybrsz3bijgx8-nixos-minimal-26.05.20261010.6fcc160-x86_64-linux.iso/iso/nixos-minimal-26.05.20261010.6fcc160-x86_64-linux.iso
SHA-256: 752c8325b07b8577249acd3c937d5564acb000f99739993166e73892dc7463da
```

That source snapshot includes the final application and catalog; later evidence
annotations and maintenance-workflow documentation were added after its build.
Tagged release builds must generate their own manifests and checksums.

## Snapshot recovery continuation

The recovery implementation based on `fabce7e` was verified locally on
2026-10-11 using:

```bash
nix develop --command env PYTHONPATH=apps/vfctl/src python -m unittest discover -s tests
nix develop --command nix flake check --all-systems --no-build --accept-flake-config
nix develop --command nix flake check -L --accept-flake-config
```

All 38 Python tests passed, all-system evaluation passed, and all native x86
checks passed. The booted VM captured a pending message in a backup, allowed
the original coordinator to transmit it, restored the backup into fresh
state, and verified that the message remained uncertain while new traffic
transmitted. Staff authentication, alert ACK history, the explicit public
projection and trusted local HTTPS worked after restore. Independent security
and QA review approved the revised implementation after strengthening schema
compatibility checks. See [the recovery QA record](qa-reports/snapshot-recovery.md).

The Checks workflow repeats these gates natively on x86 and ARM and retains
evaluation/build logs with the tested source revision and architecture.
Recovery revision `c91498b` passed both native jobs in
[Checks run 38172407182](https://github.com/closure-labs/mish/actions/runs/38172407182).
Its image realization and physical acceptance remain separate.
