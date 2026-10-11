{
  lib,
  modulesPath,
  pkgs,
  ...
}:

{
  imports = [
    "${modulesPath}/installer/sd-card/sd-image-aarch64.nix"
  ];

  networking.hostName = "vestaforge-rpi4";
  networking.firewall.enable = true;

  boot.supportedFilesystems = lib.mkForce [
    "ext4"
    "vfat"
  ];
  boot.initrd.supportedFilesystems = lib.mkForce [
    "ext4"
    "vfat"
  ];

  documentation.enable = false;
  documentation.nixos.enable = false;
  documentation.man.enable = false;

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  environment.systemPackages = [
    pkgs.curl
    pkgs.jq
  ];

  hardware.enableRedistributableFirmware = lib.mkForce false;

  services.vestaforge = {
    enable = true;
    profile = "sbc-rpi4";
    domain = "vestaforge.local";
    nostr.relays = [ "mock://local" ];
    openziti.enable = false;
    geopack.enable = false;
    radio.simulator.enable = false;
    localAccess.enable = true;
  };

  system.stateVersion = "26.05";
}
