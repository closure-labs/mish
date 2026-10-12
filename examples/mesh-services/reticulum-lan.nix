# Experimental shared LAN daemon. Only the daemon may own an optional RNode.
{ lib, ... }: {
  nixpkgs.config.allowUnfreePredicate =
    pkg:
    builtins.elem (lib.getName pkg) [
      "rns"
      "lxmf"
    ];
  services.mesh-services = {
    reticulum = {
      enable = true;
      openFirewall = true;
    };
    retiBBS = {
      enable = true;
      webEnable = true;
    };
  };
}
