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
    mish-control = final.callPackage ../pkgs/mish-control { };
  };

  nixosModules.mish = ../modules/nixos/mish;

  nixosConfigurations.mish-mini-pc-installer = lib.nixosSystem {
    system = "x86_64-linux";
    modules = [
      ({ modulesPath, pkgs, ... }: {
        imports = [ "${modulesPath}/installer/cd-dvd/installation-cd-minimal.nix" ];
        nixpkgs.overlays = [ self.overlays.default ];
        environment.systemPackages = [
          pkgs.mish-control
          pkgs.git
          pkgs.gh
        ];
        environment.etc."mish-source".source = self;
        system.stateVersion = "26.05";
      })
    ];
  };
  nixosConfigurations.mish-mini-pc = lib.nixosSystem {
    system = "x86_64-linux";
    modules = [
      (
        { ... }:
        {
          nixpkgs.overlays = [ self.overlays.default ];
        }
      )
      self.nixosModules.mish
      ../hosts/mish-mini-pc/configuration.nix
    ];
  };

  nixosConfigurations.mish-rpi4 = lib.nixosSystem {
    system = "aarch64-linux";
    modules = [
      (
        { ... }:
        {
          nixpkgs.overlays = [ self.overlays.default ];
        }
      )
      self.nixosModules.mish
      ../hosts/mish-rpi4/configuration.nix
    ];
  };

  nixosConfigurations.mish-rpi5-nvme = nixos-raspberrypi.lib.nixosSystem {
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
      self.nixosModules.mish
      ../hosts/mish-rpi5-nvme/configuration.nix
    ];
  };

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

  checks =
    lib.genAttrs linuxSystems (
      system:
      let
        pkgs = pkgsFor system;
        miniConfig = self.nixosConfigurations.mish-mini-pc.config;
        rpi4Config = self.nixosConfigurations.mish-rpi4.config;
        rpi5Config = self.nixosConfigurations.mish-rpi5-nvme.config;
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
              self.nixosModules.mish
              {
                system.stateVersion = "26.05";
                services.mish = {
                  enable = true;
                  matrix = {
                    enable = true;
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
          echo "fake rnsd for Mish module evaluation"
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
              self.nixosModules.mish
              {
                system.stateVersion = "26.05";
                services.mish = {
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
          channel-selection =
            pkgs.runCommand "mish-channel-selection-check"
              {
                nativeBuildInputs = [ pkgs.nodejs_24 ];
              }
              ''
                export MISH_CHANNEL_SELECTION_PATH=${../apps/mish/static/channel-selection.js}
                node --test ${../tests/channel-selection.test.cjs}
                touch "$out"
              '';
          workflows =
            pkgs.runCommand "mish-workflows-check"
              {
                nativeBuildInputs = [ pkgs.actionlint ];
              }
              ''
                actionlint ${../.github}/workflows/*.yml
                touch "$out"
              '';
          formatting =
            pkgs.runCommand "mish-format-check" { nativeBuildInputs = [ pkgs.nixfmt-rfc-style ]; }
              ''
                find ${../.} -name '*.nix' -print0 | xargs -0 nixfmt --check
                touch "$out"
              '';

          module-eval =
            pkgs.runCommand "mish-${profileConfig.services.mish.profile}-module-eval-check"
              {
                profile = profileConfig.services.mish.profile;
                matrixProvider = profileConfig.services.mish.matrix.provider;
                matrixBackend = profileConfig.den.quirks.meshServices.matrix.backend or "disabled";
                releaseGated = profileReleaseGated;
                primaryAcceptanceTarget = profilePrimaryAcceptance;
                apiBindHost = profileConfig.services.mish.api.bindHost;
              }
              ''
                case "$profile" in
                  mini-pc)
                    test "$matrixProvider" = "tuwunel"
                    test "$matrixBackend" = "disabled"
                    ;;
                  sbc-rpi4)
                    test "$matrixProvider" = "conduit"
                    test "$matrixBackend" = "disabled"
                    ;;
                  *)
                    echo "unexpected Mish alpha profile: $profile" >&2
                    exit 1
                    ;;
                esac
                test "$releaseGated" = "1"
                test "$primaryAcceptanceTarget" = "1"
                test "$apiBindHost" = "127.0.0.1"
                touch "$out"
              '';

          matrix-dual-provider-policy =
            pkgs.runCommand "mish-matrix-dual-provider-policy-check"
              {
                failedCount = toString (builtins.length failedMatrixAssertions);
              }
              ''
                test "$failedCount" -gt 0
                touch "$out"
              '';

          reticulum-rnode-provider =
            pkgs.runCommand "mish-reticulum-rnode-provider-check"
              {
                providers = builtins.concatStringsSep "," reticulumConfig.services.mish.radio.providers;
                simulatorEnabled =
                  if reticulumConfig.den.quirks.meshServices.radio.simulatorEnabled then "1" else "0";
                reticulumEnabled =
                  if reticulumConfig.den.quirks.meshServices.radio.reticulumRnodeEnabled then "1" else "0";
                reticulumServiceUser = reticulumConfig.systemd.services.mish-reticulum-rnode.serviceConfig.User;
                reticulumPort = reticulumConfig.den.quirks.meshServices.radio.reticulumRnode.port;
                reticulumMode = reticulumConfig.den.quirks.meshServices.radio.reticulumRnode.mode;
              }
              ''
                test "$providers" = "reticulum-rnode"
                test "$simulatorEnabled" = "0"
                test "$reticulumEnabled" = "1"
                test "$reticulumServiceUser" = "mish"
                test "$reticulumPort" = "/dev/ttyUSB0"
                test "$reticulumMode" = "access_point"
                touch "$out"
              '';

          experimental-rpi5-profile =
            pkgs.runCommand "mish-experimental-rpi5-profile-check"
              {
                profile = rpi5Config.services.mish.profile;
                matrixProvider = rpi5Config.services.mish.matrix.provider;
                releaseGated = if rpi5Config.den.quirks.meshServices.profile.releaseGated then "1" else "0";
                primaryAcceptanceTarget =
                  if rpi5Config.den.quirks.meshServices.profile.primaryAcceptanceTarget then "1" else "0";
                nixDistribution = rpi5Config.services.mish.nixDistribution;
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
            pkgs.runCommand "mish-schema-check"
              {
                nativeBuildInputs = [ pkgs.mish-control ];
              }
              ''
                mish internal-check-schemas \
                  --schemas ${../schemas} \
                  --examples ${../examples} \
                  --catalog ${../catalog/mish-catalog.json}
                touch "$out"
              '';

          cli =
            pkgs.runCommand "mish-cli-check"
              {
                nativeBuildInputs = [
                  (pkgs.python3.withPackages (ps: [ ps.pyyaml ]))
                  pkgs.mish-control
                ];
              }
              ''
                export MISH_STATE_DIR="$TMPDIR/state"
                mish --state-dir "$MISH_STATE_DIR" status > "$TMPDIR/status.json"
                mish --state-dir "$MISH_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp1.json"
                mish --state-dir "$MISH_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp2.json"
                mish --state-dir "$MISH_STATE_DIR" waypoint list > "$TMPDIR/waypoints.json"
                mish --state-dir "$MISH_STATE_DIR" geopack create \
                  --id pack-cli \
                  --name "CLI Pack" \
                  --waypoint wp-cli \
                  --visibility public > "$TMPDIR/pack.json"
                mish --state-dir "$MISH_STATE_DIR" geopack publish \
                  --pack pack-cli \
                  --relay mock://check > "$TMPDIR/publication.json"
                mish --state-dir "$MISH_STATE_DIR" identity bootstrap-operator > "$TMPDIR/operator.json"
                export MISH_CATALOG_PATH=${../catalog/mish-catalog.json}
                export MISH_QUEUE_SCRIPT_PATH=${../.github/scripts/queue_prs.py}
                export MISH_CI_ROOT=${../.}
                export MISH_CACHE_SCRIPT_PATH=${../.github/scripts/cache-outputs.sh}
                python3 -m unittest discover -s ${../tests} -p 'test_*.py'
                touch "$out"
              '';

          service-config =
            pkgs.runCommand "mish-service-config-check"
              {
                controlUser = profileConfig.systemd.services.mish-control.serviceConfig.User;
                controlReadWritePath = builtins.elemAt profileConfig.systemd.services.mish-control.serviceConfig.ReadWritePaths 0;
                matrixService =
                  if !profileConfig.services.mish.matrix.enable then
                    "disabled"
                  else if profileConfig.services.mish.matrix.provider == "conduit" then
                    profileConfig.systemd.services.mish-matrix-conduit.serviceConfig.User
                  else
                    profileConfig.systemd.services.mish-matrix-tuwunel.serviceConfig.User;
                matrixReleaseGated =
                  if profileConfig.den.quirks.meshServices.control.matrixReleaseGated then "1" else "0";
                openzitiMfa =
                  if (profileConfig.den.quirks.meshServices.openziti.privilegedMfaRequired or false) then
                    "1"
                  else
                    "0";
                publicationPublicOnly =
                  if (profileConfig.den.quirks.meshServices.geopack.publication.publicOnly or false) then
                    "1"
                  else
                    "0";
              }
              ''
                test "$controlUser" = "mish"
                test "$matrixService" = "disabled"
                test "$matrixReleaseGated" = "1"
                test "$controlReadWritePath" = "/var/lib/mish"
                test "$openzitiMfa" = "0"
                test "$publicationPublicOnly" = "0"
                touch "$out"
              '';
        };
        x86Checks = lib.optionalAttrs (system == "x86_64-linux") {
          vm-smoke = import ../tests/appliance.nix { inherit pkgs self; };
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
            pkgs.runCommand "mish-schema-check"
              {
                nativeBuildInputs = [ pkgs.mish-control ];
              }
              ''
                mish internal-check-schemas \
                  --schemas ${../schemas} \
                  --examples ${../examples} \
                  --catalog ${../catalog/mish-catalog.json}
                touch "$out"
              '';

          cli =
            pkgs.runCommand "mish-cli-check"
              {
                nativeBuildInputs = [
                  (pkgs.python3.withPackages (ps: [ ps.pyyaml ]))
                  pkgs.mish-control
                ];
              }
              ''
                export MISH_STATE_DIR="$TMPDIR/state"
                mish --state-dir "$MISH_STATE_DIR" status > "$TMPDIR/status.json"
                mish --state-dir "$MISH_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp1.json"
                mish --state-dir "$MISH_STATE_DIR" waypoint add-simulated \
                  --id wp-cli \
                  --lat 44.0521 \
                  --lon -123.0868 \
                  --label "CLI waypoint" \
                  --visibility public > "$TMPDIR/wp2.json"
                mish --state-dir "$MISH_STATE_DIR" waypoint list > "$TMPDIR/waypoints.json"
                mish --state-dir "$MISH_STATE_DIR" geopack create \
                  --id pack-cli \
                  --name "CLI Pack" \
                  --waypoint wp-cli \
                  --visibility public > "$TMPDIR/pack.json"
                mish --state-dir "$MISH_STATE_DIR" geopack publish \
                  --pack pack-cli \
                  --relay mock://check > "$TMPDIR/publication.json"
                mish --state-dir "$MISH_STATE_DIR" identity bootstrap-operator > "$TMPDIR/operator.json"
                export MISH_CATALOG_PATH=${../catalog/mish-catalog.json}
                export MISH_QUEUE_SCRIPT_PATH=${../.github/scripts/queue_prs.py}
                export MISH_CI_ROOT=${../.}
                export MISH_CACHE_SCRIPT_PATH=${../.github/scripts/cache-outputs.sh}
                python3 -m unittest discover -s ${../tests} -p 'test_*.py'
                touch "$out"
              '';
        };
    };
}
