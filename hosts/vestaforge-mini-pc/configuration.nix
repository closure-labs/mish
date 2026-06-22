{ modulesPath, pkgs, ... }:

{
  imports = [
    "${modulesPath}/virtualisation/qemu-vm.nix"
  ];

  networking.hostName = "vestaforge-mini-pc";
  networking.firewall.enable = true;

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  environment.systemPackages = [
    pkgs.curl
    pkgs.jq
  ];

  services.vestaforge = {
    enable = true;
    profile = "mini-pc";
    domain = "vestaforge.local";
    matrix.provider = "tuwunel";
    nostr.relays = [ "mock://local" ];
    openziti.enable = true;
    geopack.enable = true;
    radio.simulator.enable = true;
  };

  virtualisation.vmVariant = {
    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;
  };

  system.stateVersion = "25.05";
}
