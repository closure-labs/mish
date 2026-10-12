# Event HTTPS and native Meshtastic TCP

Provision an event-domain certificate and private key before disconnecting WAN.
Set `services.mish.localAccess` to:

```nix
{
  enable = true;
  hostName = "chat.event.example";
  tlsMode = "external";
  certificateFile = "/run/credentials/event/fullchain.pem";
  keyFile = "/run/credentials/event/key.pem";
}
```

Paths are strings referencing runtime files, never Nix path literals. Keep the
key readable by Caddy and root only. Do not put certificates, private keys or
session secrets in shared images or the Nix store. The existing explicit
`tlsMode = "internal"` alternative uses Caddy's local CA, which must be trusted
on each attendee device. Caddy's certificate-file syntax is documented at
https://caddyserver.com/docs/caddyfile/directives/tls .

Before the event, verify the certificate matches `hostName`, the entire chain
is supplied, expiry is beyond the event, the appliance clock is correct, and
the external AP's local DNS resolves the domain to the appliance with WAN
unplugged. Test both iOS Safari and Android Chrome without certificate warnings.
The API remains bound to loopback; serve all browser access through HTTPS.

USB remains the default. For an independently configured native Linux radio,
enable the nixpkgs `services.meshtasticd` module and set
`services.mish.radio.meshtastic.tcpEndpoint = "127.0.0.1:4403"` with
`services.mish.radio.meshtastic.enable = true`. Keep meshtasticd's TCP listener
on a trusted local interface; the client protocol is not TLS authenticated.
The configured TCP endpoint replaces the USB messaging endpoint. Do not enable
two clients for the same physical device. meshtasticd owns its hardware.

Explicit channel commissioning commands also accept
`--device tcp://127.0.0.1:4403`. Stop Mish's live worker before commissioning,
then inspect, apply and verify the channel plan as for USB. TCP reconnect runs
the same full channel fingerprint checks before routing. Partial writes remain
uncertain and are never automatically replayed. TCP connection attempts time out
after ten seconds. Endpoints require literal IP addresses and nonzero ports.

TCP transport tests and existing stream-based protocol fixtures are software
evidence only. Native meshtasticd VM startup, supported radio hardware, and
physical disconnect/reconnect must be qualified independently.

With WAN physically disconnected, run
`bash scripts/check-event-https.sh chat.event.example 192.168.50.2 /path/to/fullchain.pem`
from a client with `openssl`, `getent`, and `timeout`. This checks event DNS,
hostname, at least seven days of certificate lifetime, and a live TLS handshake
against the client's clock and trust store. Set a longer lifetime margin for
longer events. Browser trust must still be checked on the actual phones.
