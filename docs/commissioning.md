# Commissioning and recovery

This runbook describes the first x86 deployment with an external Ethernet
access point. See the [operator runbook](operator-runbook.md) for live-event response.
Physical installation and radio acceptance remain unverified;
record results in [acceptance.md](acceptance.md). Prepare installation media,
source checkout and required Nix closures before entering an offline venue.
Internet may be used for preparation; event operation must work with WAN
unplugged. The installer is not an unattended disk installer.

## Kit and network

Use one x86_64 mini-PC, reliable local storage and power, a USB Meshtastic
radio, two staff radios, an external Wi-Fi AP and two staff browser devices.
Provide a console/keyboard for first boot and recovery. Reserve a coordinator
LAN address in the AP and configure local DNS `mish.home.arpa` to that address.
Connect the coordinator by Ethernet and staff/attendee browsers by Wi-Fi.
Disable WAN for the acceptance procedure. Ensure devices can resolve local DNS
and reach TCP 443 even when internet connectivity is absent.

The shared image contains no operator token or private staff channel key.
Configure all staff radios with the same appropriate regional radio settings,
modem preset and private channel in the Meshtastic client. Choose the staff
channel index deliberately; the adapter uses an index, not a channel key. Do
not commit channel keys to the repository or put them in Nix expressions.

## Prepare and install x86

```bash
nix build .#packages.x86_64-linux.mish-mini-pc-installer
nix build .#packages.x86_64-linux.mish-mini-pc-image
```

The first output is an ISO for installation/rescue tooling; the second is a
QEMU VM for development. Verify the artifact checksum against its build
manifest, write the ISO to installation media and boot it on the intended
mini-PC. Identify the intended target disk at the console. Follow the normal
NixOS installation process to partition/format that disk and mount the target
root at `/mnt` (and its EFI system partition under `/mnt/boot` for UEFI). Disk
choice and partitioning are machine-specific and intentionally explicit.

Generate the physical machine's hardware configuration:

```bash
nixos-generate-config --root /mnt
```

The ISO includes the complete repository at `/etc/mish-source`. Copy
that source into the installed configuration, keeping its committed lockfile:

```bash
cp -r /etc/mish-source /mnt/etc/nixos/source
chmod -R u+w /mnt/etc/nixos/source
```

Create `/mnt/etc/nixos/flake.nix` using the generated physical hardware file.
The example assumes UEFI and a mounted EFI system partition; adjust its
bootloader for BIOS hardware. Set the actual USB path and staff channel index.
The QEMU mini-PC host is a VM definition and is deliberately not imported here.

```nix
{
  inputs.mish.url = "path:./source";
  inputs.nixpkgs.follows = "mish/nixpkgs";
  outputs = { self, nixpkgs, mish }: {
    nixosConfigurations.coordinator = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./hardware-configuration.nix
        mish.nixosModules.mish
        ({ ... }: {
          nixpkgs.overlays = [ mish.overlays.default ];
          nix.settings.experimental-features = [ "nix-command" "flakes" ];
          boot.loader.systemd-boot.enable = true;
          boot.loader.efi.canTouchEfiVariables = true;
          networking.hostName = "mish-coordinator";
          networking.useDHCP = true;
          services.mish = {
            enable = true;
            profile = "mini-pc";
            coordinatorId = "coordinator-1";
            retentionDays = 7;
            localAccess = {
              enable = true;
              hostName = "mish.home.arpa";
            };
            radio.providers = [ "meshtastic-usb" ];
            radio.meshtastic = {
              enable = true;
              device = "/dev/serial/by-id/REPLACE-WITH-THE-RADIO";
              channel = 0;
            };
          };
          system.stateVersion = "26.05";
        })
      ];
    };
  };
}
```

Enable no legacy simulations unless deliberately testing them. Build the
physical configuration during preparation, then install it:

```bash
nix build /mnt/etc/nixos#nixosConfigurations.coordinator.config.system.build.toplevel
nixos-install --flake /mnt/etc/nixos#coordinator
```

Set the local root administrator's password at the console when prompted. A
venue with no WAN needs every dependency already in the installer's Nix store
or a prepared local cache. The stock ISO does not by itself certify that a
customized hardware closure is available offline. Reboot into the installation
and verify console login works.

Pi 4 qualification follows x86 acceptance. The shared SD profile has no
preset administrative password; prepare an operator-specific build with your
public SSH key and trusted local SSH access before flashing. Add a module to
the Pi 4 host configuration for that qualification build, for example:

```nix
services.openssh.enable = true;
services.openssh.settings.PasswordAuthentication = false;
users.users.root.openssh.authorizedKeys.keys = [
  "ssh-ed25519 REPLACE-WITH-YOUR-PUBLIC-KEY operator"
];
```

A public key is safe to include in that operator-specific artifact; never
include the private key. Build
`packages.aarch64-linux.mish-rpi4-image`, verify its checksum and write the
uncompressed image to a correctly identified SD card. Connect Ethernet to the
trusted AP and verify the appliance SSH host-key fingerprint through a trusted
local method before enrollment. Confirm boot, firmware, SSH access, Ethernet,
USB and service behavior on Pi 4 with 4 GB RAM before advertising that target.
The shared SD image alone does not establish an accessible first-boot console
or physical readiness. Do not infer Pi 5 readiness from this procedure.

## USB radio and TLS

Find the actual stable device path:

```bash
ls -l /dev/serial/by-id/
systemctl status mish-control
journalctl -u mish-control -n 100
```

Replace the placeholder device in the physical configuration, rebuild and
activate it. The service user must have explicit serial device access; the
module's USB option supplies the service's serial group membership. Verify
that the configured radio is connected on the selected private staff channel.
Do not use `/dev/ttyUSB0` as a persistent commissioning identifier.

With local access enabled, Caddy serves `https://mish.home.arpa` using its
internal CA. At the console, locate the generated root certificate; the usual
NixOS Caddy data path is
`/var/lib/caddy/.local/share/caddy/pki/authorities/local/root.crt`. This path and trusted HTTPS have been verified in the x86 VM; verify the
actual path and certificate fingerprint again on the physical machine. Transfer only
that public root certificate to staff devices over a trusted local method and
install it in the trust store used by their browser. Confirm the hostname
resolves locally and the browser validates HTTPS. Never distribute the CA's
private key. Keep CA data for recovery; replacing the CA requires devices to
trust the new root.

## Enroll and send the first message

Run privileged local CLI commands as `mish` so state ownership remains
correct. Global `--config` and `--state-dir` flags precede the subcommand.

```bash
sudo -u mish mish --config /etc/mish/config.json commission --name 'Event operator'
sudo -u mish mish --config /etc/mish/config.json enroll --name 'Alice'
sudo -u mish mish --config /etc/mish/config.json enroll --name 'Bob'
```

Each successful enrollment prints an identity ID and a bearer token once.
Keep the operator credential private and deliver each staff token only to its
owner. Revoke a lost or exposed enrollment with
`mish --config /etc/mish/config.json revoke --id <staff-id>`, run as the
service user, then enroll a replacement. The last operator cannot be revoked;
enroll a replacement operator first. A CLI user with access to appliance state is trusted administrative
access; HTTP roles are enforced for browser requests. Do not share a staff
credential to simulate two identities.

Open `https://mish.home.arpa/staff` on both devices and enter their individual
tokens. Browser credentials are held in memory; re-enter after a reload.
Send a short message from Alice, verify Bob sees it, and reply. Create an alert
with an accountable responder; the browser currently sets expiration to one
hour. Acknowledge from Bob's browser.
Record its short ID and ask a radio user to send `ACK <ID>` on the private
channel. The journal must distinguish Bob's authenticated acknowledgement from
the radio-reported node/channel acknowledgement.

Operator CLI equivalents are available for local recovery:

```bash
sudo -u mish mish --config /etc/mish/config.json message --author 'Event operator' --text 'Staff check-in'
sudo -u mish mish --config /etc/mish/config.json messages
sudo -u mish mish --config /etc/mish/config.json announce --text 'Water station is open'
```

`alert` takes `--text`, `--author`, `--responder`, `--expires` (Unix epoch time
within the next 24 hours) and optional `--urgent`. `ack --id <ID> --token
<staff-token>` records an enrolled acknowledgement. Keep messages within 160
UTF-8 bytes for the initial radio path. Observe queue/submission/transport states, persisted delivery timestamps
and explicit human acknowledgements independently. Authenticated `/api/status`
reports the selected adapter and current radio connection state. The journal
also displays enrolled author names. Real-radio sends use a default 30-second
interval, configurable with `services.mish.radio.meshtastic.sendIntervalSeconds`;
that interval is engineering pacing, not promised RF delivery latency. An uncertain send requires
operator review; do not equate it with failure or automatically resend it.
There is no retry command for uncertain records: check radio receipt first, then
create a replacement message deliberately if needed.

Open `/public` with no token to verify the announcement. Publishing writes a
separate public text/metadata projection; it does not expose the private source
message, staff identity, location or unpublished alert. The operator can publish
chosen announcement wording independently of staff conversations.

## Backup, restore and spare activation

Make a consistent SQLite backup into a private writable directory owned by the
service user, then move the resulting file to protected removable storage:

```bash
sudo install -d -m 0700 -o mish -g mish /var/lib/mish/backups
sudo -u mish mish --config /etc/mish/config.json backup --output /var/lib/mish/backups/event.sqlite
```

Use a new output filename for every backup; existing files are never overwritten.
The SQLite backup contains event journal, acknowledgement history, publication
and enrolled credential hashes. It does not include the complete appliance:
separately back up `/etc/nixos` and its lockfile/hardware configuration,
`/var/lib/caddy` (including private CA material), any configured runtime
credential files and legacy Mish state/keys if used. Export the radio's
configuration separately and protect its private channel key. Protect all
backup media as private event data.

To restore, keep the old coordinator stopped before activating a spare. Copy
the standalone backup to protected local storage outside the active state
directory, readable by `mish`. Stop the application and preserve its
entire existing state directory, including any SQLite sidecars. Choose a new
recovery-copy name each time; the example requires that name to be absent:

```bash
sudo systemctl stop mish-control
sudo test ! -e /var/lib/mish-before-restore
sudo mv -T /var/lib/mish /var/lib/mish-before-restore
sudo install -d -m 0700 -o mish -g mish /var/lib/mish
sudo -u mish mish --config /etc/mish/config.json restore --input /protected-local-storage/event.sqlite
```

Check each command succeeded before continuing. On a fresh spare without an
existing state directory, omit the `mv` step. `restore` validates the database
and required journal schema, stages a private `0600` database, and installs it
atomically. It refuses existing event databases or SQLite sidecars and never
overwrites them. Supply a backup produced by `mish backup`, rather than
copying a live database. Invalid input leaves no installed event database.
Record the command's snapshot SHA-256, recovery time and quarantined count;
the database retains these in its recovery history.

A snapshot cannot establish whether queued messages were sent after its
capture. Restored `queued` and `sending` records therefore become `uncertain`
and are never transmitted automatically. Previously recorded submission,
transport confirmation and human acknowledgements are preserved. Review
uncertain records with staff before deliberately creating replacements. An
ordinary restart of the same database continues draining its pending queue.

Restore the corresponding local config and CA data while Caddy is stopped,
preserving their owners/modes. Start Caddy and `mish-control`, verify
authenticated staff history and public feed, and check radio status before
sending a new test message. Credentials from that backup remain valid;
revoke/re-enroll any exposed identity. A spare must reuse the LAN/DNS address
deliberately and only one coordinator is active at a time. Retain the previous
state and backup until recovery has been verified. Snapshot recovery restores
only data present when the backup was captured; later writes require recovery
of the original state or independent records.

## Updates, rollback, retention and wipe

Before updates, create a backup, record the active system generation and retain
the previous release closure. Rebuild the pinned configuration in preparation,
activate with `nixos-rebuild switch`, then exercise staff/public/radio behavior.
Use `nixos-rebuild switch --rollback` or select a previous generation from the
boot menu to recover software. NixOS rollback does not roll back SQLite data;
check schema compatibility and restore a matching backup if necessary. Test
that combination on a spare before a live event. Direct radio communication
must remain available when the coordinator is stopped.

The default retention setting is seven days. The service applies retention
hourly to journal, announcements, acknowledgement history and received-packet
deduplication records. Run the privileged `prune` command to apply it immediately
and verify retained records. Backups need their own
retention policy; pruning the active database does not delete old exports.

At event end, export the required records, stop the application, and run the
explicit wipe:

```bash
sudo systemctl stop mish-control
sudo -u mish mish --config /etc/mish/config.json wipe --confirm WIPE-EVENT
```

Review any legacy files, runtime credential files, Caddy CA material, radio
channel configuration, removable backups and staff device credentials
separately. The wipe command removes the configured Mish state directory contents,
including any backups stored inside it; the directory itself is retained for
recommissioning. It requires an existing event database. Move exports off the
appliance first.
Wipe is logical deletion, not certified forensic erasure of
flash storage. Regenerate credentials and, if the CA was removed, device trust
for the next event. Confirm public/private event records are gone before
returning the kit to service.
