inputs:
let
  context = import ./context.nix { inherit inputs; };
  inherit (context)
    self
    lib
    systems
    linuxSystems
    forAllSystems
    pkgsFor
    ;
  inherit (inputs) nixos-raspberrypi;
in
{
  overlays.default = context.overlay;
  nixosModules.mish = ../modules/nixos/mish;
  nixosModules.mesh-services = ../modules/nixos/mesh-services;

  packages = forAllSystems (
    system:
    let
      pkgs = pkgsFor system;
      images = {
        x86_64-linux = {
          mish-mini-pc-installer =
            self.nixosConfigurations.mish-mini-pc-installer.config.system.build.isoImage;
          mish-image = self.nixosConfigurations.mish-mini-pc.config.system.build.vm;
          mish-mini-pc-image = self.nixosConfigurations.mish-mini-pc.config.system.build.vm;
        };
        aarch64-linux = {
          mish-image = self.nixosConfigurations.mish-rpi4.config.system.build.sdImage;
          mish-rpi4-image = self.nixosConfigurations.mish-rpi4.config.system.build.sdImage;
          mish-rpi5-installer-image = nixos-raspberrypi.installerImages.rpi5;
          mish-rpi5-nvme-system = self.nixosConfigurations.mish-rpi5-nvme.config.system.build.toplevel;
        };
      };
    in
    {
      default = pkgs.mish-control;
      mish-control = pkgs.mish-control;
    }
    // lib.optionalAttrs (builtins.elem system linuxSystems) {
      supply-drop-bbs = pkgs.supply-drop-bbs;
      retibbs = pkgs.retibbs;
      reticulum = pkgs.rns;
      mish-validation = import ./validation.nix {
        inherit pkgs system;
        checks = self.checks.${system};
        package = pkgs.mish-control;
        lockHash = builtins.hashFile "sha256" ../flake.lock;
      };
    }
    // (images.${system} or { })
  );

  apps = forAllSystems (system: {
    default = {
      type = "app";
      program = "${self.packages.${system}.mish-control}/bin/mish";
    };
    mish = self.apps.${system}.default;
  });

  devShells = forAllSystems (
    system:
    let
      pkgs = pkgsFor system;
    in
    {
      default = pkgs.mkShell {
        inputsFrom = [ pkgs.mish-control ];
        packages = [
          pkgs.git
          pkgs.gh
          pkgs.nix
          pkgs.nixfmt
          pkgs.cargo
          pkgs.rustc
          pkgs.rustfmt
          pkgs.clippy
          pkgs.pkg-config
          pkgs.sqlite
          pkgs.cachix
          pkgs.nodejs_24
          (pkgs.python3.withPackages (ps: [ ps.pyyaml ]))
        ];
        env.PKG_CONFIG_PATH = "${pkgs.sqlite.dev}/lib/pkgconfig";
      };
    }
  );

  formatter = forAllSystems (
    system:
    let
      pkgs = pkgsFor system;
    in
    pkgs.writeShellScriptBin "mish-fmt" ''
      set -euo pipefail

      if [ "$#" -gt 0 ]; then
        exec ${pkgs.nixfmt-rfc-style}/bin/nixfmt "$@"
      fi

      find . \
        -path ./.git -prune -o \
        -name '*.nix' -print0 \
        | xargs -0 ${pkgs.nixfmt-rfc-style}/bin/nixfmt
    ''
  );

  checks = import ./checks.nix inputs;
}
