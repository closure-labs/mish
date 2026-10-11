{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.mish;
  ziti = cfg.openziti;
  servicePolicy = {
    version = "0.1";
    privilegedMfaRequired = ziti.privilegedMfaRequired;
    services = {
      matrix = {
        identity = "matrix";
        bind = "127.0.0.1:${toString cfg.matrix.port}";
        dialRoles = [
          "operator"
          "friend"
        ];
      };
      geopack = {
        identity = "geopack";
        bind = "127.0.0.1:${toString cfg.api.port}";
        dialRoles = [
          "operator"
          "friend"
        ];
      };
      admin-api = {
        identity = "admin-api";
        bind = "127.0.0.1:${toString cfg.api.port}";
        dialRoles = [ "operator" ];
        requireMfa = ziti.privilegedMfaRequired;
      };
    };
  };
  policyFile = pkgs.writeText "mish-openziti-policy.json" (builtins.toJSON servicePolicy);
in
{
  config = lib.mkIf (cfg.enable && ziti.enable) {
    environment.etc."mish/openziti-policy.json".source = policyFile;

    den.quirks.meshServices.openziti = {
      controllerUrl = "http://127.0.0.1:${toString ziti.controllerPort}";
      edgeRouterUrl = "http://127.0.0.1:${toString ziti.edgeRouterPort}";
      serviceIdentities = ziti.serviceIdentities;
      privilegedMfaRequired = ziti.privilegedMfaRequired;
    };

    systemd.services.mish-openziti-controller = {
      description = "Mish OpenZiti controller baseline";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/mish-openziti-stub --role controller --host 127.0.0.1 --port ${toString ziti.controllerPort} ${lib.optionalString ziti.privilegedMfaRequired "--require-mfa"}";
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

    systemd.services.mish-openziti-edge-router = {
      description = "Mish OpenZiti edge-router baseline";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network.target"
        "mish-openziti-controller.service"
      ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/mish-openziti-stub --role edge-router --host 127.0.0.1 --port ${toString ziti.edgeRouterPort} ${lib.optionalString ziti.privilegedMfaRequired "--require-mfa"}";
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
  };
}
