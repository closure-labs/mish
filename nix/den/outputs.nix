{ inputs, den, ... }:
let
  outputs = import ../outputs.nix inputs;
  modes = den.aspects.mish.provides.modes.provides;
  features = den.aspects.mish.provides.features.provides;
  architectures = den.aspects.mish.provides.architectures.provides;
in
{
  flake = {
    inherit (outputs) overlays formatter;
    nixosModules = outputs.nixosModules // {
      mish-coordinator = den.lib.aspects.resolve "nixos" modes.coordinator;
      mish-simulation = den.lib.aspects.resolve "nixos" modes.simulation;
      mish-x86_64 = den.lib.aspects.resolve "nixos" architectures.x86_64;
      mish-aarch64 = den.lib.aspects.resolve "nixos" architectures.aarch64;
      mish-meshtastic = den.lib.aspects.resolve "nixos" {
        includes = [
          modes.coordinator
          features.meshtastic-usb
        ];
      };
    };
  };
  den.schema.flake-system.includes = [ den.aspects.mish-outputs ];
  den.aspects.mish-outputs = {
    packages = { pkgs, ... }: outputs.packages.${pkgs.stdenv.hostPlatform.system};
    apps = { pkgs, ... }: outputs.apps.${pkgs.stdenv.hostPlatform.system};
    checks = { pkgs, ... }: outputs.checks.${pkgs.stdenv.hostPlatform.system};
    devShells = { pkgs, ... }: outputs.devShells.${pkgs.stdenv.hostPlatform.system};
  };
}
