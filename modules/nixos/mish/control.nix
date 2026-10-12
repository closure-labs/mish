{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.mish;
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
        device =
          if cfg.radio.meshtastic.tcpEndpoint == null then
            cfg.radio.meshtastic.device
          else
            "tcp://${cfg.radio.meshtastic.tcpEndpoint}";
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
      path = "/etc/mish/catalog.json";
    };
  };
  renderedConfig = pkgs.writeText "mish-config.json" (builtins.toJSON configJson);
in
{
  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion =
          cfg.localAccess.tlsMode != "external"
          ||
            builtins.all
              (
                p:
                p != null
                && lib.hasPrefix "/" p
                && !(lib.hasPrefix "/nix/store/" p)
                && builtins.match "[-A-Za-z0-9_./]+" p != null
              )
              [
                cfg.localAccess.certificateFile
                cfg.localAccess.keyFile
              ];
        message = "External event TLS requires runtime certificate/key paths outside the Nix store.";
      }
    ];
    den.quirks.meshServices.control = {
      localUrl = "http://${cfg.api.bindHost}:${toString cfg.api.port}";
      matrixReleaseGated = releaseGatedMatrix;
      configPath = "/etc/mish/config.json";
      catalogPath = "/etc/mish/catalog.json";
    };

    environment.etc."mish/config.json".source = renderedConfig;
    environment.etc."mish/catalog.json".source = ../../../catalog/mish-catalog.json;

    services.caddy = lib.mkIf cfg.localAccess.enable {
      enable = true;
      virtualHosts.${cfg.localAccess.hostName}.extraConfig = ''
        ${
          if cfg.localAccess.tlsMode == "internal" then
            "tls internal"
          else
            "tls \"${toString cfg.localAccess.certificateFile}\" \"${toString cfg.localAccess.keyFile}\""
        }
        reverse_proxy 127.0.0.1:${toString cfg.api.port}
      '';
    };
    networking.firewall.allowedTCPPorts = lib.optionals cfg.localAccess.enable [
      80
      443
    ];
    users.users.mish.extraGroups = lib.optionals cfg.radio.meshtastic.enable [ "dialout" ];
    systemd.services.mish-control = {
      description = "Mish local control API";
      wantedBy = [ "multi-user.target" ];
      after = [
        "network.target"

      ]
      ++ lib.optionals cfg.openziti.enable [
        "mish-openziti-controller.service"
        "mish-openziti-edge-router.service"
      ];
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/mish --config /etc/mish/config.json --state-dir ${lib.escapeShellArg cfg.stateDir} serve";
        User = "mish";
        Group = "mish";
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
