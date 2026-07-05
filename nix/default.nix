{
  self,
  nixpkgs,
  nixos-raspberrypi,
}:
let
  lib = nixpkgs.lib;
  systems = [
    "aarch64-darwin"
    "x86_64-linux"
    "aarch64-linux"
  ];
  linuxSystems = [
    "x86_64-linux"
    "aarch64-linux"
  ];
  forAllSystems = lib.genAttrs systems;
  pkgsFor =
    system:
    import nixpkgs {
      inherit system;
      overlays = [ self.overlays.default ];
    };
in
{
  overlays.default = final: _prev: {
    vestaforge-control = final.callPackage ../pkgs/vestaforge-control { };
  };

  nixosModules.vestaforge = ../modules/nixos/vestaforge;

  nixosConfigurations.vestaforge-mini-pc = lib.nixosSystem {
    system = "x86_64-linux";
    modules = [
      (
        { ... }:
        {
          nixpkgs.overlays = [ self.overlays.default ];
        }
      )
      self.nixosModules.vestaforge
      ../hosts/vestaforge-mini-pc/configuration.nix
    ];
  };

  nixosConfigurations.vestaforge-rpi4 = lib.nixosSystem {
    system = "aarch64-linux";
    modules = [
      (
        { ... }:
        {
          nixpkgs.overlays = [ self.overlays.default ];
        }
      )
      self.nixosModules.vestaforge
      ../hosts/vestaforge-rpi4/configuration.nix
    ];
  };

  nixosConfigurations.vestaforge-rpi5-nvme = nixos-raspberrypi.lib.nixosSystem {
    specialArgs = { inherit nixos-raspberrypi; };
    modules = [
      (
        { ... }:
        {
          imports = [
            nixos-raspberrypi.nixosModules.raspberry-pi-5.base
            nixos-raspberrypi.nixosModules.raspberry-pi-5.bluetooth
            nixos-raspberrypi.nixosModules.trusted-nix-caches
          ];
          nixpkgs.overlays = [ self.overlays.default ];
        }
      )
      self.nixosModules.vestaforge
      ../hosts/vestaforge-rpi5-nvme/configuration.nix
    ];
  };

  packages = forAllSystems (
    system:
    let
      pkgs = pkgsFor system;
      images = {
        x86_64-linux = {
          vestaforge-image = self.nixosConfigurations.vestaforge-mini-pc.config.system.build.vm;
          vestaforge-mini-pc-image = self.nixosConfigurations.vestaforge-mini-pc.config.system.build.vm;
        };
        aarch64-linux = {
          vestaforge-image = self.nixosConfigurations.vestaforge-rpi4.config.system.build.sdImage;
          vestaforge-rpi4-image = self.nixosConfigurations.vestaforge-rpi4.config.system.build.sdImage;
          vestaforge-rpi5-installer-image = nixos-raspberrypi.installerImages.rpi5;
          vestaforge-rpi5-nvme-system =
            self.nixosConfigurations.vestaforge-rpi5-nvme.config.system.build.toplevel;
        };
      };
    in
    {
      default = pkgs.vestaforge-control;
      vestaforge-control = pkgs.vestaforge-control;
    }
    // (images.${system} or { })
  );

  apps = forAllSystems (system: {
    default = {
      type = "app";
      program = "${self.packages.${system}.vestaforge-control}/bin/vfctl";
    };
    vfctl = self.apps.${system}.default;
  });

  devShells = forAllSystems (
    system:
    let
      pkgs = pkgsFor system;
    in
    {
      default = pkgs.mkShell {
        packages = [
          pkgs.git
          pkgs.nix
          pkgs.python3
          pkgs.vestaforge-control
        ];
      };
    }
  );

  formatter = forAllSystems (
    system:
    let
      pkgs = pkgsFor system;
    in
    pkgs.writeShellScriptBin "vestaforge-fmt" ''
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

  checks =
    lib.genAttrs linuxSystems (
      system:
      let
        pkgs = pkgsFor system;
        miniConfig = self.nixosConfigurations.vestaforge-mini-pc.config;
        rpi4Config = self.nixosConfigurations.vestaforge-rpi4.config;
        rpi5Config = self.nixosConfigurations.vestaforge-rpi5-nvme.config;
        profileConfig = if system == "aarch64-linux" then rpi4Config else miniConfig;
        invalidMatrixConfig =
          (lib.nixosSystem {
            inherit system;
            modules = [
              (
                { ... }:
                {
                  nixpkgs.overlays = [ self.overlays.default ];
                }
              )
              self.nixosModules.vestaforge
              {
                system.stateVersion = "25.05";
                services.vestaforge = {
                  enable = true;
                  matrix = {
                    provider = "tuwunel";
                    providers = {
                      tuwunel.enable = true;
                      conduit.enable = true;
                    };
                  };
                };
              }
            ];
          }).config;
        failedMatrixAssertions = builtins.filter (
          assertion: !assertion.assertion
        ) invalidMatrixConfig.assertions;
        fakeRns = pkgs.writeShellScriptBin "rnsd" ''
          echo "fake rnsd for VestaForge module evaluation"
        '';
        reticulumConfig =
          (lib.nixosSystem {
            inherit system;
            modules = [
              (
                { ... }:
                {
                  nixpkgs.overlays = [ self.overlays.default ];
                }
              )
              self.nixosModules.vestaforge
              {
                system.stateVersion = "25.05";
                services.vestaforge = {
                  enable = true;
                  radio = {
                    providers = [ "reticulum-rnode" ];
                    reticulumRnode = {
                      package = fakeRns;
                      port = "/dev/ttyUSB0";
                      frequency = 915000000;
                    };
                  };
                };
              }
            ];
          }).config;
        profileReleaseGated =
          if profileConfig.den.quirks.meshServices.profile.releaseGated then "1" else "0";
        profilePrimaryAcceptance =
          if profileConfig.den.quirks.meshServices.profile.primaryAcceptanceTarget then "1" else "0";
        commonChecks = {
          module-eval =
            pkgs.runCommand "vestaforge-${profileConfig.services.vestaforge.profile}-module-eval-check"
              {
                profile = profileConfig.services.vestaforge.profile;
                matrixProvider = profileConfig.services.vestaforge.matrix.provider;
                matrixBackend = profileConfig.den.quirks.meshServices.matrix.backend;
                releaseGated = profileReleaseGated;
                primaryAcceptanceTarget = profilePrimaryAcceptance;
                apiBindHost = profileConfig.services.vestaforge.api.bindHost;
              }
              ''
                case "$profile" in
                  mini-pc)
                    test "$matrixProvider" = "tuwunel"
                    test "$matrixBackend" = "tuwunel"
                    ;;
                  sbc-rpi4)
                    test "$matrixProvider" = "conduit"
                    test "$matrixBackend" = "conduit"
                    ;;
                  *)
                    echo "unexpected VestaForge alpha profile: $profile" >&2
                    exit 1
                    ;;
                esac
                test "$releaseGated" = "1"
                test "$primaryAcceptanceTarget" = "1"
                test "$apiBindHost" = "127.0.0.1"
                touch "$out"
              '';

          matrix-dual-provider-policy =
            pkgs.runCommand "vestaforge-matrix-dual-provider-policy-check"
              {
                failedCount = toString (builtins.length failedMatrixAssertions);
              }
              ''
                test "$failedCount" -gt 0
                touch "$out"
              '';

          reticulum-rnode-provider =
            pkgs.runCommand "vestaforge-reticulum-rnode-provider-check"
              {
                providers = builtins.concatStringsSep "," reticulumConfig.services.vestaforge.radio.providers;
                simulatorEnabled =
                  if reticulumConfig.den.quirks.meshServices.radio.simulatorEnabled then "1" else "0";
                reticulumEnabled =
                  if reticulumConfig.den.quirks.meshServices.radio.reticulumRnodeEnabled then "1" else "0";
                reticulumServiceUser =
                  reticulumConfig.systemd.services.vestaforge-reticulum-rnode.serviceConfig.User;
                reticulumPort = reticulumConfig.den.quirks.meshServices.radio.reticulumRnode.port;
                reticulumMode = reticulumConfig.den.quirks.meshServices.radio.reticulumRnode.mode;
              }
              ''
                test "$providers" = "reticulum-rnode"
                test "$simulatorEnabled" = "0"
                test "$reticulumEnabled" = "1"
                test "$reticulumServiceUser" = "vestaforge"
                test "$reticulumPort" = "/dev/ttyUSB0"
                test "$reticulumMode" = "access_point"
                touch "$out"
              '';

          experimental-rpi5-profile =
            pkgs.runCommand "vestaforge-experimental-rpi5-profile-check"
              {
                profile = rpi5Config.services.vestaforge.profile;
                matrixProvider = rpi5Config.services.vestaforge.matrix.provider;
                releaseGated = if rpi5Config.den.quirks.meshServices.profile.releaseGated then "1" else "0";
                primaryAcceptanceTarget =
                  if rpi5Config.den.quirks.meshServices.profile.primaryAcceptanceTarget then "1" else "0";
                nixDistribution = rpi5Config.services.vestaforge.nixDistribution;
              }
              ''
                test "$profile" = "sbc-rpi5-nvme"
                test "$matrixProvider" = "conduit"
                test "$releaseGated" = "0"
                test "$primaryAcceptanceTarget" = "0"
                test "$nixDistribution" = "nixos"
                touch "$out"
              '';

          schema =
            pkgs.runCommand "vestaforge-schema-check"
              {
                nativeBuildInputs = [ pkgs.vestaforge-control ];
              }
              ''
                vfctl internal-check-schemas \
                  --schemas ${../schemas} \
                  --examples ${../examples} \
                  --catalog ${../catalog/vestaforge-catalog.json}
                touch "$out"
              '';

          cli =
            pkgs.runCommand "vestaforge-cli-check"
              {
                nativeBuildInputs = [
                  pkgs.python3
                  pkgs.vestaforge-control
                ];
              }
              ''
                export VESTAFORGE_STATE_DIR="$TMPDIR/state"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" status > "$TMPDIR/status.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp1.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp2.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" waypoint list > "$TMPDIR/waypoints.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" geopack create \
                  --id pack-cli \
                  --name "CLI Pack" \
                  --waypoint wp-cli \
                  --visibility public > "$TMPDIR/pack.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" geopack publish \
                  --pack pack-cli \
                  --relay mock://check > "$TMPDIR/publication.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" identity bootstrap-operator > "$TMPDIR/operator.json"
                export VESTAFORGE_CATALOG_PATH=${../catalog/vestaforge-catalog.json}
                PYTHONPATH=${../apps/vfctl/src} python3 -m unittest discover -s ${../tests}
                touch "$out"
              '';

          service-config =
            pkgs.runCommand "vestaforge-service-config-check"
              {
                controlUser = profileConfig.systemd.services.vestaforge-control.serviceConfig.User;
                controlReadWritePath = builtins.elemAt profileConfig.systemd.services.vestaforge-control.serviceConfig.ReadWritePaths 0;
                matrixService =
                  if profileConfig.services.vestaforge.matrix.provider == "conduit" then
                    profileConfig.systemd.services.vestaforge-matrix-conduit.serviceConfig.User
                  else
                    profileConfig.systemd.services.vestaforge-matrix-tuwunel.serviceConfig.User;
                matrixReleaseGated =
                  if profileConfig.den.quirks.meshServices.control.matrixReleaseGated then "1" else "0";
                openzitiMfa =
                  if profileConfig.den.quirks.meshServices.openziti.privilegedMfaRequired then "1" else "0";
                publicationPublicOnly =
                  if profileConfig.den.quirks.meshServices.geopack.publication.publicOnly then "1" else "0";
              }
              ''
                test "$controlUser" = "vestaforge"
                test "$matrixService" = "vestaforge"
                test "$matrixReleaseGated" = "1"
                test "$controlReadWritePath" = "/var/lib/vestaforge"
                test "$openzitiMfa" = "1"
                test "$publicationPublicOnly" = "1"
                touch "$out"
              '';
        };
        x86Checks = lib.optionalAttrs (system == "x86_64-linux") {
          vm-smoke = pkgs.nixosTest {
            name = "vestaforge-mini-pc-smoke";

            nodes.machine =
              { pkgs, ... }:
              {
                imports = [ self.nixosModules.vestaforge ];
                nixpkgs.overlays = [ self.overlays.default ];

                environment.systemPackages = [ pkgs.curl ];

                services.vestaforge = {
                  enable = true;
                  profile = "mini-pc";
                  domain = "vestaforge.local";
                  matrix.provider = "tuwunel";
                  nostr.relays = [ "mock://vm" ];
                  openziti.enable = true;
                  geopack.enable = true;
                  radio.simulator.enable = true;
                };

                system.stateVersion = "25.05";
              };

            testScript = ''
              machine.start()
              machine.wait_for_unit("vestaforge-matrix-tuwunel.service")
              machine.wait_for_unit("vestaforge-openziti-controller.service")
              machine.wait_for_unit("vestaforge-openziti-edge-router.service")
              machine.wait_for_unit("vestaforge-control.service")
              machine.succeed("curl --fail --silent http://127.0.0.1:8787/health")
              machine.succeed("vfctl --state-dir /var/lib/vestaforge waypoint add-simulated --id vm-wp --lat 44.0521 --lon -123.0868 --label VM --visibility public")
              machine.succeed("vfctl --state-dir /var/lib/vestaforge geopack create --id vm-pack --name VM --waypoint vm-wp --visibility public")
              machine.succeed("vfctl --state-dir /var/lib/vestaforge geopack publish --pack vm-pack")
            '';
          };

          vm = self.nixosConfigurations.vestaforge-mini-pc.config.system.build.vm;
        };
      in
      commonChecks // x86Checks
    )
    // {
      aarch64-darwin =
        let
          pkgs = pkgsFor "aarch64-darwin";
        in
        {
          schema =
            pkgs.runCommand "vestaforge-schema-check"
              {
                nativeBuildInputs = [ pkgs.vestaforge-control ];
              }
              ''
                vfctl internal-check-schemas \
                  --schemas ${../schemas} \
                  --examples ${../examples} \
                  --catalog ${../catalog/vestaforge-catalog.json}
                touch "$out"
              '';

          cli =
            pkgs.runCommand "vestaforge-cli-check"
              {
                nativeBuildInputs = [
                  pkgs.python3
                  pkgs.vestaforge-control
                ];
              }
              ''
                export VESTAFORGE_STATE_DIR="$TMPDIR/state"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" status > "$TMPDIR/status.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp1.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp2.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" waypoint list > "$TMPDIR/waypoints.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" geopack create \
                  --id pack-cli \
                  --name "CLI Pack" \
                  --waypoint wp-cli \
                  --visibility public > "$TMPDIR/pack.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" geopack publish \
                  --pack pack-cli \
                  --relay mock://check > "$TMPDIR/publication.json"
                vfctl --state-dir "$VESTAFORGE_STATE_DIR" identity bootstrap-operator > "$TMPDIR/operator.json"
                export VESTAFORGE_CATALOG_PATH=${../catalog/vestaforge-catalog.json}
                PYTHONPATH=${../apps/vfctl/src} python3 -m unittest discover -s ${../tests}
                touch "$out"
              '';
        };
    };
}
