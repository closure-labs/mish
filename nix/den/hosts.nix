{ inputs, den, ... }:
let
  modes = den.aspects.mish.provides.modes.provides;
  architectures = den.aspects.mish.provides.architectures.provides;
in
{
  den.hosts = {
    x86_64-linux = {
      mish-mini-pc = { };
      mish-mini-pc-installer = { };
    };
    aarch64-linux = {
      mish-rpi4 = { };
      mish-rpi5-nvme.instantiate =
        args:
        inputs.nixos-raspberrypi.lib.nixosSystem (
          args
          // {
            specialArgs = (args.specialArgs or { }) // {
              nixos-raspberrypi = inputs.nixos-raspberrypi;
            };
          }
        );
    };
  };
  den.aspects = {
    mish-mini-pc.includes = [
      modes.coordinator
      architectures.mini-pc
    ];
    mish-mini-pc-installer.includes = [
      modes.installer
      architectures.x86_64
    ];
    mish-rpi4.includes = [
      modes.coordinator
      architectures.rpi4
    ];
    mish-rpi5-nvme.includes = [
      modes.coordinator
      architectures.rpi5-nvme
    ];
  };
}
