# Import inputs.mish.nixosModules.mesh-services alongside this file.
{ lib, ... }: {
  nixpkgs.config.allowUnfreePredicate = pkg: builtins.elem (lib.getName pkg) [ "supply-drop-bbs" ];
  services.mesh-services.supplyDrop = {
    enable = true;
    transport = "meshtastic";
    device = "/dev/serial/by-id/REPLACE-WITH-DEDICATED-BBS-RADIO";
    adminEnable = true; # Loopback only; use an operator SSH tunnel.
  };
}
