{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.vestaforge;
  releaseGatedMatrix =
    (cfg.profile == "mini-pc" && cfg.matrix.provider == "tuwunel")
    || (cfg.profile == "sbc-rpi4" && cfg.matrix.provider == "conduit");
  configJson = {
    version = "0.1.0-alpha";
    profile = cfg.profile;
    domain = cfg.domain;
    nixDistribution = cfg.nixDistribution;
    stateDir = cfg.stateDir;
    coordinatorId = cfg.coordinatorId;
    retentionDays = cfg.retentionDays;
    api = {
      bindHost = cfg.api.bindHost;
      port = cfg.api.port;
    };
    matrix = {
      provider = cfg.matrix.provider;
      domain = cfg.matrix.domain;
      port = cfg.matrix.port;
      homeserverUrl = "http://127.0.0.1:${toString cfg.matrix.port}";
      releaseGated = releaseGatedMatrix;
      providers = {
        tuwunel = {
          enable = cfg.matrix.providers.tuwunel.enable;
        };
        conduit = {
          enable = cfg.matrix.providers.conduit.enable;
        };
      };
    };
    openziti = {
      enable = cfg.openziti.enable;
      controllerUrl = "http://127.0.0.1:${toString cfg.openziti.controllerPort}";
      edgeRouterUrl = "http://127.0.0.1:${toString cfg.openziti.edgeRouterPort}";
      privilegedMfaRequired = cfg.openziti.privilegedMfaRequired;
      serviceIdentities = cfg.openziti.serviceIdentities;
    };
    geopack = {
      enable = cfg.geopack.enable;
      signingKeyPath = "${cfg.stateDir}/keys/geopack-signing.key";
    };
    nostr = {
      relays = cfg.nostr.relays;
    };
    radio = {
      adapter =
        if cfg.radio.meshtastic.enable then
          "meshtastic"
        else if cfg.radio.simulator.enable then
          "simulator"
        else
          "disabled";
      providers = cfg.radio.providers;
      meshtastic = {
        enable = cfg.radio.meshtastic.enable;
        device = cfg.radio.meshtastic.device;
        channel = cfg.radio.meshtastic.channel;
        sendIntervalSeconds =
          if cfg.radio.meshtastic.enable then cfg.radio.meshtastic.sendIntervalSeconds else 0;
      };
      simulator = {
        enable = cfg.radio.simulator.enable;
      };
      reticulumRnode = {
        enable = cfg.radio.reticulumRnode.enable;
        interface = cfg.radio.reticulumRnode.interfaceName;
        port = cfg.radio.reticulumRnode.port;
        mode = cfg.radio.reticulumRnode.mode;
        frequency = cfg.radio.reticulumRnode.frequency;
        bandwidth = cfg.radio.reticulumRnode.bandwidth;
        txPower = cfg.radio.reticulumRnode.txPower;
        spreadingFactor = cfg.radio.reticulumRnode.spreadingFactor;
        codingRate = cfg.radio.reticulumRnode.codingRate;
      };
    };
    catalog = {
      path = "/etc/vestaforge/catalog.json";
    };
  };
  renderedConfig = pkgs.writeText "vestaforge-config.json" (builtins.toJSON configJson);
in
{
  config = lib.mkIf cfg.enable {
    den.quirks.meshServices.control = {
      localUrl = "http://${cfg.api.bindHost}:${toString cfg.api.port}";
      matrixReleaseGated = releaseGatedMatrix;
      configPath = "/etc/vestaforge/config.json";
      catalogPath = "/etc/vestaforge/catalog.json";
    };

    environment.etc."vestaforge/config.json".source = renderedConfig;
    environment.etc."vestaforge/catalog.json".source = ../../../catalog/vestaforge-catalog.json;

    services.caddy = lib.mkIf cfg.localAccess.enable {
      enable = true;
      virtualHosts.${cfg.localAccess.hostName}.extraConfig = ''
        tls internal
        reverse_proxy 127.0.0.1:${toString cfg.api.port}
      '';
    };
    networking.firewall.allowedTCPPorts = lib.optionals cfg.localAccess.enable [
      80
      443
    ];
    users.users.vestaforge.extraGroups = lib.optionals cfg.radio.meshtastic.enable [ "dialout" ];
    systemd.services.vestaforge-control = {
      description = "VestaForge local control API";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network.target"

      ]
      ++ lib.optionals cfg.openziti.enable [
        "vestaforge-openziti-controller.service"
        "vestaforge-openziti-edge-router.service"
      ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/vfctl --config /etc/vestaforge/config.json --state-dir ${lib.escapeShellArg cfg.stateDir} serve";
        User = "vestaforge";
        Group = "vestaforge";
        Restart = "on-failure";
        RestartSec = "2s";
        UMask = "0077";
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        ReadWritePaths = [ cfg.stateDir ];
      };
    };
  };
}
