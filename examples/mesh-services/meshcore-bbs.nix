# Standalone BBS only: this does not add Mish room or alert transport over MeshCore.
{ lib, ... }: {
  nixpkgs.config.allowUnfreePredicate = pkg: builtins.elem (lib.getName pkg) [ "supply-drop-bbs" ];
  services.mesh-services.supplyDrop = {
    enable = true;
    transport = "meshcore";
    device = "/dev/serial/by-id/REPLACE-WITH-MESHCORE-COMPANION";
    adminEnable = true;
  };
}
