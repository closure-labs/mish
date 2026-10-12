{ modulesPath, ... }:

{
  imports = [
    "${modulesPath}/virtualisation/qemu-vm.nix"
  ];

  networking.hostName = "mish-mini-pc";

  services.mish = {
    profile = "mini-pc";
    matrix.provider = "tuwunel";
  };

  virtualisation.vmVariant = {
    virtualisation.memorySize = 2048;
    virtualisation.cores = 2;
  };

}
