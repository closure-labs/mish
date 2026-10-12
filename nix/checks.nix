{
  self,
  nixpkgs,
  nixos-raspberrypi,
  ...
}@inputs:
let
  context = import ./context.nix { inherit inputs; };
  inherit (context) lib linuxSystems pkgsFor;
in
lib.genAttrs linuxSystems (
  system:
  let
    pkgs = pkgsFor system;
    miniConfig = self.nixosConfigurations.mish-mini-pc.config;
    rpi4Config = self.nixosConfigurations.mish-rpi4.config;
    rpi5Config = self.nixosConfigurations.mish-rpi5-nvme.config;
    profileConfig = if system == "aarch64-linux" then rpi4Config else miniConfig;
    installerConfig = self.nixosConfigurations.mish-mini-pc-installer.config;
    simulationConfig =
      (lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.mish-simulation
          { system.stateVersion = "24.11"; }
        ];
      }).config;
    meshtasticConfig =
      (lib.nixosSystem {
        inherit system;
        modules = [
          self.nixosModules.mish-meshtastic
          { services.mish.radio.meshtastic.device = "/dev/serial/by-id/test-operator-radio"; }
        ];
      }).config;
    rejectedMode = modules: (lib.nixosSystem { inherit system modules; }).config;
    placeholderConfig = rejectedMode [ self.nixosModules.mish-meshtastic ];
    unstableDeviceConfig = rejectedMode [
      self.nixosModules.mish-meshtastic
      { services.mish.radio.meshtastic.device = "/dev/ttyUSB0"; }
    ];
    dualRadioConfig = rejectedMode [
      self.nixosModules.mish-simulation
      self.nixosModules.mish-meshtastic
      { services.mish.radio.meshtastic.device = "/dev/serial/by-id/test-operator-radio"; }
    ];
    wrongArchitectureConfig = rejectedMode [
      (if system == "x86_64-linux" then self.nixosModules.mish-aarch64 else self.nixosModules.mish-x86_64)
    ];
    rejects =
      config: message: lib.any (entry: !entry.assertion && entry.message == message) config.assertions;
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
      den-composition =
        assert
          (builtins.attrNames self.nixosConfigurations) == [
            "mish-mini-pc"
            "mish-mini-pc-installer"
            "mish-rpi4"
            "mish-rpi5-nvme"
          ];
        assert miniConfig.nixpkgs.hostPlatform.system == "x86_64-linux";
        assert rpi4Config.nixpkgs.hostPlatform.system == "aarch64-linux";
        assert rpi5Config.nixpkgs.hostPlatform.system == "aarch64-linux";
        assert rpi5Config.boot.loader.raspberry-pi.bootloader == "kernel";
        assert (installerConfig.services.mish.enable or false) == false;
        assert lib.all (cfg: cfg.system.stateVersion == "26.05") [
          miniConfig
          rpi4Config
          rpi5Config
          installerConfig
        ];
        assert lib.all
          (
            cfg:
            cfg.services.mish.enable
            && cfg.services.mish.localAccess.enable
            && !cfg.services.mish.radio.simulator.enable
            && !cfg.services.mish.radio.meshtastic.enable
            && !cfg.services.mish.matrix.enable
            && !cfg.services.mish.openziti.enable
            && !cfg.services.mish.geopack.enable
          )
          [
            miniConfig
            rpi4Config
            rpi5Config
          ];
        assert simulationConfig.services.mish.radio.simulator.enable;
        assert simulationConfig.system.stateVersion == "24.11";
        assert !simulationConfig.services.mish.radio.meshtastic.enable;
        assert builtins.elem "meshtastic-simulated" simulationConfig.services.mish.radio.providers;
        assert meshtasticConfig.services.mish.radio.meshtastic.enable;
        assert !meshtasticConfig.services.mish.radio.simulator.enable;
        assert builtins.elem "meshtastic-usb" meshtasticConfig.services.mish.radio.providers;
        assert
          meshtasticConfig.services.mish.radio.meshtastic.device == "/dev/serial/by-id/test-operator-radio";
        assert rejects placeholderConfig
          "Mish Meshtastic aspect requires an operator-supplied stable device path; commission and verify the runtime channel plan separately.";
        assert rejects unstableDeviceConfig "Meshtastic USB must use a stable /dev/serial/by-id path.";
        assert rejects dualRadioConfig "Select either physical Meshtastic or the explicit simulator.";
        assert rejects wrongArchitectureConfig
          "Mish architecture aspect requires ${
            if system == "x86_64-linux" then "aarch64-linux" else "x86_64-linux"
          }.";
        pkgs.runCommand "mish-den-composition-check" { } ''
          touch "$out"
        '';
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
}
