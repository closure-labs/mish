{ inputs, den, ... }:
let
  context = import ../context.nix { inherit inputs; };
  features = den.aspects.mish.provides.features.provides;
  modes = den.aspects.mish.provides.modes.provides;
  architectures = den.aspects.mish.provides.architectures.provides;
  architecture = system: {
    nixos = { config, ... }: {
      assertions = [
        {
          assertion = config.nixpkgs.hostPlatform.system == system;
          message = "Mish architecture aspect requires ${system}.";
        }
      ];
    };
  };
in
{
  den.aspects.mish.provides = {
    features.provides = {
      package.nixos.nixpkgs.overlays = [ context.overlay ];
      control = {
        includes = [ features.package ];
        nixos = {
          imports = [ ../../modules/nixos/mish ];
          services.mish.enable = true;
        };
      };
      local-https.nixos.services.mish.localAccess.enable = true;
      meshtastic-usb.nixos = { config, ... }: {
        services.mish.radio = {
          meshtastic.enable = true;
          providers = [ "meshtastic-usb" ];
        };
        assertions = [
          {
            assertion =
              config.services.mish.radio.meshtastic.device != "/dev/serial/by-id/REPLACE-WITH-RADIO-ID";
            message = "Mish Meshtastic aspect requires an operator-supplied stable device path; commission and verify the runtime channel plan separately.";
          }
        ];
      };
      event-defaults.nixos = { lib, pkgs, ... }: {
        networking.firewall.enable = true;
        nix.settings.experimental-features = [
          "nix-command"
          "flakes"
        ];
        environment.systemPackages = [
          pkgs.curl
          pkgs.jq
        ];
        system.stateVersion = lib.mkDefault "26.05";
        services.mish = {
          domain = lib.mkDefault "mish.local";
          nostr.relays = lib.mkDefault [ "mock://local" ];
          openziti.enable = lib.mkDefault false;
          geopack.enable = lib.mkDefault false;
          radio.simulator.enable = lib.mkDefault false;
        };
      };
    };
    modes.provides = {
      coordinator.includes = [
        features.control
        features.event-defaults
        features.local-https
      ];
      simulation = {
        includes = [ modes.coordinator ];
        nixos.services.mish.radio = {
          simulator.enable = true;
          providers = [ "meshtastic-simulated" ];
        };
      };
      installer = {
        includes = [ features.package ];
        nixos = { modulesPath, pkgs, ... }: {
          imports = [ "${modulesPath}/installer/cd-dvd/installation-cd-minimal.nix" ];
          environment.systemPackages = [
            pkgs.mish-control
            pkgs.git
            pkgs.gh
          ];
          environment.etc."mish-source".source = inputs.self;
          system.stateVersion = "26.05";
        };
      };
    };
    architectures.provides = {
      x86_64 = architecture "x86_64-linux";
      aarch64 = architecture "aarch64-linux";
      mini-pc = {
        includes = [ architectures.x86_64 ];
        nixos.imports = [ ../../hosts/mish-mini-pc/configuration.nix ];
      };
      rpi4 = {
        includes = [ architectures.aarch64 ];
        nixos.imports = [ ../../hosts/mish-rpi4/configuration.nix ];
      };
      rpi5-nvme = {
        includes = [ architectures.aarch64 ];
        nixos.imports = [
          inputs.nixos-raspberrypi.nixosModules.raspberry-pi-5.base
          inputs.nixos-raspberrypi.nixosModules.raspberry-pi-5.bluetooth
          inputs.nixos-raspberrypi.nixosModules.trusted-nix-caches
          ../../hosts/mish-rpi5-nvme/configuration.nix
        ];
      };
    };
  };
}
