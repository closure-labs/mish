{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.mish;
  radio = cfg.radio;
  rnode = radio.reticulumRnode;
  boolString = value: if value then "yes" else "no";
  optionalConfig = condition: line: lib.optionalString condition "      ${line}\n";
  reticulumConfig = pkgs.writeText "mish-reticulum-rnode-config" ''
    [reticulum]
      enable_transport = ${boolString rnode.enableTransport}
      share_instance = ${boolString rnode.shareInstance}
      shared_instance_port = ${toString rnode.sharedInstancePort}

    [interfaces]
      [[${rnode.interfaceName}]]
      type = RNodeInterface
      enabled = yes
      mode = ${rnode.mode}
      port = ${rnode.port}
      frequency = ${toString rnode.frequency}
      bandwidth = ${toString rnode.bandwidth}
      txpower = ${toString rnode.txPower}
      spreadingfactor = ${toString rnode.spreadingFactor}
      codingrate = ${toString rnode.codingRate}
      discoverable = ${boolString rnode.discoverable}
      flow_control = ${boolString rnode.flowControl}
    ${optionalConfig (rnode.idCallsign != null) "id_callsign = ${rnode.idCallsign}"}
    ${optionalConfig (rnode.idInterval != null) "id_interval = ${toString rnode.idInterval}"}
    ${rnode.configExtra}
  '';
  reticulumEnabled = cfg.enable && rnode.enable;
in
{
  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion =
          (radio.meshtastic.enable && radio.meshtastic.tcpEndpoint == null)
          -> lib.hasPrefix "/dev/serial/by-id/" radio.meshtastic.device;
        message = "Meshtastic USB must use a stable /dev/serial/by-id path.";
      }
      {
        assertion = !(radio.meshtastic.enable && radio.simulator.enable);
        message = "Select either physical Meshtastic or the explicit simulator.";
      }
      {
        assertion = radio.simulator.enable -> builtins.elem "meshtastic-simulated" radio.providers;
        message = ''services.mish.radio.simulator.enable requires "meshtastic-simulated" in services.mish.radio.providers.'';
      }
      {
        assertion = rnode.enable -> builtins.elem "reticulum-rnode" radio.providers;
        message = ''services.mish.radio.reticulumRnode.enable requires "reticulum-rnode" in services.mish.radio.providers.'';
      }
      {
        assertion = reticulumEnabled -> rnode.package != null;
        message = "The reticulum-rnode provider requires services.mish.radio.reticulumRnode.package, usually pkgs.rns after allowing the unfree Reticulum license in nixpkgs.";
      }
    ];

    den.quirks.meshServices.radio = {
      providers = radio.providers;
      simulatorEnabled = radio.simulator.enable;
      reticulumRnodeEnabled = rnode.enable;
      hardwareMeshtasticEnabled = radio.meshtastic.enable;
      reticulumRnode = {
        interface = rnode.interfaceName;
        port = rnode.port;
        mode = rnode.mode;
        frequency = rnode.frequency;
        bandwidth = rnode.bandwidth;
        txPower = rnode.txPower;
        spreadingFactor = rnode.spreadingFactor;
        codingRate = rnode.codingRate;
        service = "mish-reticulum-rnode.service";
        configDir = "${cfg.stateDir}/reticulum";
      };
    };

    environment.etc."mish/reticulum-rnode/config".source = reticulumConfig;

    environment.systemPackages = lib.optionals reticulumEnabled [ rnode.package ];

    systemd.services.mish-reticulum-rnode = lib.mkIf reticulumEnabled {
      description = "Mish Reticulum RNode radio provider";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        ExecStartPre = "${pkgs.coreutils}/bin/install -D -m 0600 /etc/mish/reticulum-rnode/config ${cfg.stateDir}/reticulum/config";
        ExecStart = "${rnode.package}/bin/rnsd --config ${cfg.stateDir}/reticulum --service";
        User = "mish";
        Group = "mish";
        Restart = "on-failure";
        RestartSec = "3s";
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
