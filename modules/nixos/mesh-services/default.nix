# Standalone exports: no Mish or Den dependency. State and identities never bridge.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.mesh-services;
  supply = cfg.supplyDrop;
  rns = cfg.reticulum;
  bbs = cfg.retiBBS;
  devices =
    cfg.reservedDevices
    ++ lib.optional supply.enable supply.device
    ++ lib.optional (rns.enable && rns.rnode.enable) rns.rnode.device;
  runtimeFile = lib.types.strMatching "/[^\n\r]+";
  hardening = name: {
    User = name;
    Group = name;
    StateDirectory = name;
    StateDirectoryMode = "0700";
    WorkingDirectory = "/var/lib/${name}";
    Restart = "on-failure";
    RestartSec = "5s";
    UMask = "0077";
    NoNewPrivileges = true;
    PrivateTmp = true;
    ProtectHome = true;
    ProtectSystem = "strict";
  };
  supplyConfig = (pkgs.formats.toml { }).generate "supply-drop.toml" {
    bbs = {
      name = supply.name;
      data_dir = "/var/lib/supply-drop";
    };
    database.synchronous = "FULL";
    plugins = {
      cli.enabled = true;
      mesh = {
        enabled = supply.transport == "meshcore";
        connection_type = "serial";
        serial_port = supply.device;
      };
      meshtastic = {
        enabled = supply.transport == "meshtastic";
        connection_type = "serial";
        serial_port = supply.device;
      };
      web = {
        enabled = supply.adminEnable;
        bind = "127.0.0.1:${toString supply.adminPort}";
      };
    };
  };
  daemonConfig = pkgs.writeText "reticulum-config" ''
    [reticulum]
    enable_transport = False
    share_instance = Yes
    shared_instance_type = tcp
    shared_instance_port = ${toString rns.sharedPort}
    instance_control_port = ${toString rns.controlPort}
    [interfaces]
    [[Event LAN]]
    type = AutoInterface
    enabled = Yes
    ${lib.optionalString rns.rnode.enable ''
      [[RNode]]
      type = RNodeInterface
      enabled = Yes
      port = ${rns.rnode.device}
      frequency = ${toString rns.rnode.frequency}
      bandwidth = ${toString rns.rnode.bandwidth}
      txpower = ${toString rns.rnode.txPower}
      spreadingfactor = ${toString rns.rnode.spreadingFactor}
      codingrate = ${toString rns.rnode.codingRate}
    ''}
  '';
  clientConfig = pkgs.writeText "retibbs-reticulum-config" ''
    [reticulum]
    share_instance = Yes
    shared_instance_type = tcp
    shared_instance_port = ${toString rns.sharedPort}
    instance_control_port = ${toString rns.controlPort}
    [interfaces]
  '';
  bbsConfig = pkgs.writeText "retibbs-config.json" (
    builtins.toJSON {
      server_name = bbs.name;
      announce_interval = 300;
      theme = "default";
      enable_web_server = bbs.webEnable;
      use_wsgi = false;
    }
  );
in
{
  options.services.mesh-services = {
    reservedDevices = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Physical devices owned by other services; assignments must be distinct stable paths.";
    };
    supplyDrop = {
      enable = lib.mkEnableOption "standalone Supply Drop radio BBS";
      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.callPackage ../../../pkgs/supply-drop-bbs { };
      };
      name = lib.mkOption {
        type = lib.types.str;
        default = "Event BBS";
      };
      transport = lib.mkOption {
        type = lib.types.enum [
          "meshtastic"
          "meshcore"
        ];
        default = "meshtastic";
      };
      device = lib.mkOption {
        type = runtimeFile;
        default = "/dev/serial/by-id/REPLACE-WITH-BBS-RADIO";
      };
      adminEnable = lib.mkEnableOption "loopback-only Supply Drop operator web UI";
      adminPort = lib.mkOption {
        type = lib.types.port;
        default = 8080;
      };
    };
    reticulum = {
      enable = lib.mkEnableOption "experimental shared Reticulum daemon";
      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.rns;
      };
      openFirewall = lib.mkEnableOption "Reticulum LAN discovery and data UDP ports";
      sharedPort = lib.mkOption {
        type = lib.types.port;
        default = 37428;
      };
      controlPort = lib.mkOption {
        type = lib.types.port;
        default = 37429;
      };
      rnode = {
        enable = lib.mkEnableOption "RNode owned exclusively by the shared Reticulum daemon";
        device = lib.mkOption {
          type = runtimeFile;
          default = "/dev/serial/by-id/REPLACE-WITH-RNODE";
        };
        frequency = lib.mkOption {
          type = lib.types.ints.positive;
          default = 915000000;
          description = "Operator-selected legal RF frequency in Hz; verify before enabling hardware.";
        };
        bandwidth = lib.mkOption {
          type = lib.types.ints.positive;
          default = 125000;
        };
        txPower = lib.mkOption {
          type = lib.types.ints.between 0 22;
          default = 10;
        };
        spreadingFactor = lib.mkOption {
          type = lib.types.ints.between 5 12;
          default = 7;
        };
        codingRate = lib.mkOption {
          type = lib.types.ints.between 5 8;
          default = 5;
        };
      };
    };
    retiBBS = {
      enable = lib.mkEnableOption "experimental RetiBBS client of the shared daemon";
      package = lib.mkOption {
        type = lib.types.package;
        default = pkgs.callPackage ../../../pkgs/retibbs { };
      };
      name = lib.mkOption {
        type = lib.types.str;
        default = "Experimental event BBS";
      };
      webEnable = lib.mkEnableOption "RetiBBS read-only loopback web interface on port 5000";
    };
  };
  config = lib.mkMerge [
    {
      assertions = [
        {
          assertion = builtins.length devices == builtins.length (lib.unique devices);
          message = "Mesh services require distinct physical devices; Supply Drop needs a dedicated radio.";
        }
        {
          assertion = lib.all (d: lib.hasPrefix "/dev/serial/by-id/" d && !(lib.hasInfix "REPLACE-WITH" d)) (
            lib.optional supply.enable supply.device
            ++ lib.optional (rns.enable && rns.rnode.enable) rns.rnode.device
          );
          message = "Mesh services require operator-supplied stable radio paths.";
        }
        {
          assertion = bbs.enable -> rns.enable;
          message = "RetiBBS requires the shared Reticulum daemon.";
        }
        {
          assertion = rns.sharedPort != rns.controlPort;
          message = "Reticulum shared and control ports must differ.";
        }
      ];
    }
    (lib.mkIf supply.enable {
      users.groups.supply-drop = { };
      users.users.supply-drop = {
        isSystemUser = true;
        group = "supply-drop";
        extraGroups = [ "dialout" ];
      };
      environment.systemPackages = [ supply.package ];
      systemd.services.supply-drop = {
        description = "Standalone Supply Drop radio BBS";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];
        serviceConfig = hardening "supply-drop" // {
          # The operator UI requests restart by exiting successfully.
          Restart = "always";
          ExecStart = "${supply.package}/bin/supply-drop-bbs run --config ${supplyConfig}";
        };
      };
    })
    (lib.mkIf rns.enable {
      networking.firewall.allowedUDPPorts = lib.optionals rns.openFirewall [
        29716
        29717
        42671
      ];
      users.groups.mesh-reticulum = { };
      users.users.mesh-reticulum = {
        isSystemUser = true;
        group = "mesh-reticulum";
        extraGroups = lib.optional rns.rnode.enable "dialout";
      };
      systemd.services.mesh-reticulum = {
        description = "Experimental shared Reticulum daemon";
        wantedBy = [ "multi-user.target" ];
        after = [ "network.target" ];
        serviceConfig = hardening "mesh-reticulum" // {
          ExecStartPre = "${pkgs.coreutils}/bin/install -m 0600 ${daemonConfig} /var/lib/mesh-reticulum/config";
          ExecStart = "${rns.package}/bin/rnsd --config /var/lib/mesh-reticulum --service";
        };
      };
    })
    (lib.mkIf bbs.enable {
      users.groups.retibbs = { };
      users.users.retibbs = {
        isSystemUser = true;
        group = "retibbs";
      };
      systemd.services.retibbs = {
        description = "Experimental independent RetiBBS";
        wantedBy = [ "multi-user.target" ];
        after = [ "mesh-reticulum.service" ];
        requires = [ "mesh-reticulum.service" ];
        partOf = [ "mesh-reticulum.service" ];
        preStart = ''
          install -d -m 0700 /var/lib/retibbs/reticulum
          install -m 0600 ${clientConfig} /var/lib/retibbs/reticulum/config
          install -m 0600 ${bbsConfig} /var/lib/retibbs/server_config.json
          cp -r --no-preserve=mode ${bbs.package}/lib/retibbs/themes /var/lib/retibbs/
        '';
        serviceConfig = hardening "retibbs" // {
          ExecStart = "${bbs.package}/bin/retibbs --reticulum-config /var/lib/retibbs/reticulum --identity-file /var/lib/retibbs/server_identity.pem";
        };
      };
    })
  ];
}
