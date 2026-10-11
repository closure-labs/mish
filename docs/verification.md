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
| Pi 4 SD image | Local attempt blocked | No local `aarch64-linux` builder; native ARM CI route unexecuted, physical boot and radio qualification unverified |
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
