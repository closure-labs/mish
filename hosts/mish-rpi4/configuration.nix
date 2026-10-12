{
  lib,
  modulesPath,
  ...
}:

{
  imports = [
    "${modulesPath}/installer/sd-card/sd-image-aarch64.nix"
  ];

  networking.hostName = "mish-rpi4";

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

  hardware.enableRedistributableFirmware = lib.mkForce false;

  services.mish = {
    profile = "sbc-rpi4";
  };

}
