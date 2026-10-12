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
        self.nixosModules.mesh-services
        {
          nixpkgs.config.allowUnfreePredicate =
            pkg:
            builtins.elem (lib.getName pkg) [
              "supply-drop-bbs"
              "rns"
              "lxmf"
            ];
          services.mesh-services = settings;
          system.stateVersion = "26.05";
        }
      ];
    }).config;
  enabled = evaluate {
    supplyDrop = {
      enable = true;
      device = "/dev/serial/by-id/bbs";
      transport = "meshcore";
    };
    reticulum.enable = true;
    retiBBS.enable = true;
  };
  conflict = evaluate {
    supplyDrop = {
      enable = true;
      device = "/dev/serial/by-id/bbs";
    };
    reservedDevices = [ "/dev/serial/by-id/bbs" ];
  };
  missingDaemon = evaluate { retiBBS.enable = true; };
  rejects = cfg: message: lib.any (a: !a.assertion && a.message == message) cfg.assertions;
in
assert rejects conflict
  "Mesh services require distinct physical devices; Supply Drop needs a dedicated radio.";
assert rejects missingDaemon "RetiBBS requires the shared Reticulum daemon.";
assert
  enabled.systemd.services.supply-drop.serviceConfig.User
  != enabled.systemd.services.retibbs.serviceConfig.User;
assert enabled.systemd.services.retibbs.requires == [ "mesh-reticulum.service" ];
assert !(enabled.services ? mish);
pkgs.runCommand "mesh-services-module-check" { } ''touch "$out"''
