{
  pkgs,
  lib,
  self,
  system,
}:
let
  evaluate =
    settings:
    (lib.nixosSystem {
      inherit system;
      modules = [
        self.nixosModules.mish
        {
          nixpkgs.overlays = [ self.overlays.default ];
          services.mish = {
            enable = true;
          }
          // settings;
          system.stateVersion = "26.05";
        }
      ];
    }).config;
  external = evaluate {
    localAccess = {
      enable = true;
      hostName = "event.test";
      tlsMode = "external";
      certificateFile = "/run/event/cert.pem";
      keyFile = "/run/event/key.pem";
    };
    radio.meshtastic = {
      enable = true;
      tcpEndpoint = "127.0.0.1:4403";
    };
  };
  invalid = evaluate {
    localAccess = {
      tlsMode = "external";
      certificateFile = "/nix/store/cert.pem";
      keyFile = "/run/key.pem";
    };
  };
in
assert lib.hasInfix ''tls "/run/event/cert.pem" "/run/event/key.pem"''
  external.services.caddy.virtualHosts."event.test".extraConfig;
assert lib.any (
  a:
  !a.assertion
  && a.message == "External event TLS requires runtime certificate/key paths outside the Nix store."
) invalid.assertions;
assert
  !(lib.any (
    a: !a.assertion && a.message == "Meshtastic USB must use a stable /dev/serial/by-id path."
  ) external.assertions);
pkgs.runCommand "mish-event-connectivity-check" { } ''touch "$out"''
