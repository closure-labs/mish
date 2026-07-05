{ lib, pkgs, ... }:

{
  networking.hostName = "vestaforge-rpi5-nvme";
  networking.firewall.enable = true;
  services.openssh.enable = true;

  boot.initrd.availableKernelModules = [
    "nvme"
    "pcie-brcmstb"
    "uas"
    "usbhid"
    "xhci_pci"
  ];
  boot.loader.raspberry-pi.bootloader = "kernel";
  boot.supportedFilesystems = lib.mkForce [
    "ext4"
    "vfat"
  ];
  boot.initrd.supportedFilesystems = lib.mkForce [
    "ext4"
    "vfat"
  ];

  fileSystems = {
    "/boot/firmware" = {
      device = "/dev/disk/by-label/FIRMWARE";
      fsType = "vfat";
      options = [
        "noatime"
        "nofail"
        "x-systemd.automount"
        "x-systemd.idle-timeout=1min"
      ];
    };

    "/" = {
      device = "/dev/disk/by-label/VESTAFORGE";
      fsType = "ext4";
      options = [ "noatime" ];
    };
  };

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

  hardware.enableRedistributableFirmware = lib.mkDefault true;

  services.vestaforge = {
    enable = true;
    profile = "sbc-rpi5-nvme";
    domain = "vestaforge.local";
    nixDistribution = "nixos";
    matrix.provider = "conduit";
    nostr.relays = [ "mock://local" ];
    openziti.enable = true;
    geopack.enable = true;
    radio.simulator.enable = true;
  };

  system.stateVersion = "25.05";
}
