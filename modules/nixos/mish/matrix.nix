{ config, lib, ... }:

let
  cfg = config.services.mish;
  matrix = cfg.matrix;
  enabledProviders = lib.filter (provider: matrix.providers.${provider}.enable) [
    "tuwunel"
    "conduit"
  ];

  mkMatrixService = provider: {
    description = "Mish Matrix ${provider} provider health service";
    wantedBy = [ "multi-user.target" ];
    after = [ "network.target" ];
    serviceConfig = {
      ExecStart = "${cfg.package}/bin/mish-matrix-stub --provider ${provider} --domain ${lib.escapeShellArg matrix.domain} --host 127.0.0.1 --port ${toString matrix.port}";
      User = "mish";
      Group = "mish";
      Restart = "on-failure";
      RestartSec = "2s";
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectHome = true;
      ProtectSystem = "strict";
    };
  };
in
{
  config = lib.mkIf (cfg.enable && matrix.enable) (
    lib.mkMerge [
      {
        assertions = [
          {
            assertion = lib.length enabledProviders == 1;
            message = "Exactly one Mish Matrix provider must be enabled for ${matrix.domain}; enabled providers: ${toString enabledProviders}";
          }
          {
            assertion = lib.elem matrix.provider enabledProviders;
            message = "services.mish.matrix.provider must match the enabled Matrix provider aspect.";
          }
        ];

        den.aspects."matrix-${matrix.provider}" = {
          nixos.meshServices.matrix = {
            provider = matrix.provider;
            domain = matrix.domain;
          };

          den.quirks.meshServices.matrix = {
            backend = matrix.provider;
            serviceName = "matrix";
            domain = matrix.domain;
          };
        };

        den.quirks.meshServices.matrix = {
          backend = matrix.provider;
          serviceName = "matrix";
          domain = matrix.domain;
          localUrl = "http://127.0.0.1:${toString matrix.port}";
          migrationSupported = false;
        };
      }

      (lib.mkIf matrix.providers.tuwunel.enable {
        systemd.services.mish-matrix-tuwunel = mkMatrixService "tuwunel";
      })

      (lib.mkIf matrix.providers.conduit.enable {
        systemd.services.mish-matrix-conduit = mkMatrixService "conduit";
      })
    ]
  );
}
