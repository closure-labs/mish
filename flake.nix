{
  description = "VestaForge 0.1 alpha NixOS appliance";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
  };

  outputs =
    { self, nixpkgs }:
    let
      lib = nixpkgs.lib;
      systems = [ "x86_64-linux" ];
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
        vestaforge-control = final.callPackage ./pkgs/vestaforge-control { };
      };

      nixosModules.vestaforge = ./modules/nixos/vestaforge;

      nixosConfigurations.vestaforge-mini-pc = lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          ({ ... }: { nixpkgs.overlays = [ self.overlays.default ]; })
          self.nixosModules.vestaforge
          ./hosts/vestaforge-mini-pc/configuration.nix
        ];
      };

      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.vestaforge-control;
          vestaforge-control = pkgs.vestaforge-control;
          vestaforge-image = self.nixosConfigurations.vestaforge-mini-pc.config.system.build.vm;
        }
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

      checks = forAllSystems (
        system:
        let
          pkgs = pkgsFor system;
          miniConfig = self.nixosConfigurations.vestaforge-mini-pc.config;
          invalidMatrixConfig =
            (lib.nixosSystem {
              inherit system;
              modules = [
                ({ ... }: { nixpkgs.overlays = [ self.overlays.default ]; })
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
        in
        {
          module-eval =
            pkgs.runCommand "vestaforge-module-eval-check"
              {
                profile = miniConfig.services.vestaforge.profile;
                matrixProvider = miniConfig.services.vestaforge.matrix.provider;
                matrixBackend = miniConfig.den.quirks.meshServices.matrix.backend;
                releaseGated = if miniConfig.den.quirks.meshServices.profile.releaseGated then "1" else "0";
                apiBindHost = miniConfig.services.vestaforge.api.bindHost;
              }
              ''
                test "$profile" = "mini-pc"
                test "$matrixProvider" = "tuwunel"
                test "$matrixBackend" = "tuwunel"
                test "$releaseGated" = "1"
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

          schema =
            pkgs.runCommand "vestaforge-schema-check"
              {
                nativeBuildInputs = [ pkgs.vestaforge-control ];
              }
              ''
                vfctl internal-check-schemas \
                  --schemas ${./schemas} \
                  --examples ${./examples} \
                  --catalog ${./catalog/vestaforge-catalog.json}
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
                PYTHONPATH=${./apps/vfctl/src} python3 -m unittest discover -s ${./tests}
                touch "$out"
              '';

          service-config =
            pkgs.runCommand "vestaforge-service-config-check"
              {
                controlUser = miniConfig.systemd.services.vestaforge-control.serviceConfig.User;
                controlReadWritePath = builtins.elemAt miniConfig.systemd.services.vestaforge-control.serviceConfig.ReadWritePaths 0;
                openzitiMfa =
                  if miniConfig.den.quirks.meshServices.openziti.privilegedMfaRequired then "1" else "0";
                publicationPublicOnly =
                  if miniConfig.den.quirks.meshServices.geopack.publication.publicOnly then "1" else "0";
              }
              ''
                test "$controlUser" = "vestaforge"
                test "$controlReadWritePath" = "/var/lib/vestaforge"
                test "$openzitiMfa" = "1"
                test "$publicationPublicOnly" = "1"
                touch "$out"
              '';

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
        }
      );
    };
}
