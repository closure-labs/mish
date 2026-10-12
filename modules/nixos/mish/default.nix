{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.mish;
in
{
  imports = [
    ../mesh-services
    ./control.nix
    ./den.nix
    ./geopack.nix
    ./matrix.nix
    ./openziti.nix
    ./radio.nix
  ];

  options.services.mish = {
    enable = lib.mkEnableOption "the Mish alpha appliance control plane";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.mish-control;
      defaultText = lib.literalExpression "pkgs.mish-control";
      description = "Package providing mish and Mish local service helpers.";
    };

    profile = lib.mkOption {
      type = lib.types.enum [
        "mini-pc"
        "sbc-rpi4"
        "sbc-rpi5-nvme"
        "home-server"
      ];
      default = "mini-pc";
      description = "Deployment profile selected for appliance composition.";
    };

    domain = lib.mkOption {
      type = lib.types.str;
      default = "mish.local";
      description = "Primary appliance domain used by service capability modules.";
    };

    nixDistribution = lib.mkOption {
      type = lib.types.enum [
        "nixos"
        "determinate"
      ];
      default = "nixos";
      description = "Operator-selected Nix implementation intent for the appliance. The determinate value records that the host should be switched to Determinate Nix out of band.";
    };

    stateDir = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/mish";
      description = "Persistent mutable state directory for keys, waypoints, GeoPacks, and publication outbox data.";
    };

    coordinatorId = lib.mkOption {
      type = lib.types.str;
      default = "coordinator-1";
      description = "Identity of the single authoritative event coordinator.";
    };
    retentionDays = lib.mkOption {
      type = lib.types.ints.positive;
      default = 7;
      description = "Days of event records retained before operator cleanup.";
    };
    localAccess = {
      tlsMode = lib.mkOption {
        type = lib.types.enum [
          "internal"
          "external"
        ];
        default = "internal";
        description = "Explicit local CA or pre-provisioned event certificate mode.";
      };
      certificateFile = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Runtime PEM certificate path outside the Nix store; required for external TLS.";
      };
      keyFile = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Runtime private key path outside the Nix store; readable only by the operator and Caddy.";
      };
      enable = lib.mkEnableOption "Caddy HTTPS through an external Ethernet access point";
      hostName = lib.mkOption {
        type = lib.types.str;
        default = "mish.home.arpa";
        description = "Local DNS name provided by the external access point.";
      };
    };
    api = {
      bindHost = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Address for the local control API. Caddy exposes the authenticated application on the local LAN.";
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 8787;
        description = "Port for the local Mish control API.";
      };
    };

    matrix = {
      enable = lib.mkEnableOption "legacy Matrix health simulation";
      provider = lib.mkOption {
        type = lib.types.enum [
          "tuwunel"
          "conduit"
        ];
        default = "tuwunel";
        description = "Matrix provider aspect selected for this appliance.";
      };

      domain = lib.mkOption {
        type = lib.types.str;
        default = cfg.domain;
        defaultText = lib.literalExpression "config.services.mish.domain";
        description = "Matrix server domain.";
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 6167;
        description = "Local Matrix provider health/API port.";
      };

      providers = {
        tuwunel.enable = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Enable the Tuwunel Matrix provider aspect.";
        };

        conduit.enable = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Enable the Conduit Matrix provider aspect.";
        };
      };
    };

    openziti = {
      enable = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = "Enable the OpenZiti baseline controller and edge-router services.";
      };

      controllerPort = lib.mkOption {
        type = lib.types.port;
        default = 1280;
        description = "Local OpenZiti controller health port for the alpha baseline.";
      };

      edgeRouterPort = lib.mkOption {
        type = lib.types.port;
        default = 3022;
        description = "Local OpenZiti edge-router health port for the alpha baseline.";
      };

      serviceIdentities = lib.mkOption {
        type = lib.types.listOf lib.types.str;
        default = [
          "matrix"
          "geopack"
          "admin-api"
        ];
        description = "Service identities created by the alpha OpenZiti bootstrap policy model.";
      };

      privilegedMfaRequired = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Require MFA in privileged operator policy metadata where supported by OpenZiti configuration.";
      };
    };

    geopack.enable = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable local GeoPack creation, signing, and publication support.";
    };

    nostr.relays = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [
        "wss://relay.example.org"
        "mock://local"
      ];
      description = "Nostr relay URLs for public GeoPack publication.";
    };

    radio = {
      providers = lib.mkOption {
        type = lib.types.listOf (
          lib.types.enum [
            "meshtastic-usb"
            "meshtastic-simulated"
            "reticulum-rnode"
          ]
        );
        default = [ ];
        description = "Radio edge providers enabled for waypoint and field-network integration.";
      };

      meshtastic = {
        enable = lib.mkEnableOption "Meshtastic USB application transport";
        tcpEndpoint = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "Optional literal IP:port for meshtasticd (for example 127.0.0.1:4403); uses verified channel semantics.";
        };
        device = lib.mkOption {
          type = lib.types.str;
          default = "/dev/serial/by-id/REPLACE-WITH-RADIO-ID";
          description = "Stable USB radio device path used by explicit channel commissioning and the verified messaging adapter.";
        };
        channel = lib.mkOption {
          type = lib.types.ints.between 0 7;
          default = 0;
          description = "Legacy simulator channel index; native radio routing uses the verified runtime channel registry.";
        };
        sendIntervalSeconds = lib.mkOption {
          type = lib.types.ints.positive;
          default = 30;
          description = "Minimum interval between radio submissions, to limit channel congestion.";
        };
      };
      simulator.enable = lib.mkOption {
        type = lib.types.bool;
        default = builtins.elem "meshtastic-simulated" cfg.radio.providers;
        defaultText = lib.literalExpression ''builtins.elem "meshtastic-simulated" config.services.mish.radio.providers'';
        description = "Enable simulator-first Meshtastic-like waypoint ingest.";
      };

      reticulumRnode = {
        enable = lib.mkOption {
          type = lib.types.bool;
          default = builtins.elem "reticulum-rnode" cfg.radio.providers;
          defaultText = lib.literalExpression ''builtins.elem "reticulum-rnode" config.services.mish.radio.providers'';
          description = "Enable Reticulum over an RNode LoRa transceiver.";
        };

        package = lib.mkOption {
          type = lib.types.nullOr lib.types.package;
          default = null;
          example = lib.literalExpression "pkgs.rns";
          description = "Package providing rnsd and Reticulum utilities. In nixpkgs this is currently unfree, so deployments must explicitly allow and provide it.";
        };

        interfaceName = lib.mkOption {
          type = lib.types.str;
          default = "Mish RNode";
          description = "Reticulum interface name rendered into the RNode configuration.";
        };

        port = lib.mkOption {
          type = lib.types.str;
          default = "/dev/ttyACM0";
          example = "tcp://rnode.local";
          description = "Serial, TCP, or BLE port used to reach the RNode.";
        };

        mode = lib.mkOption {
          type = lib.types.enum [
            "full"
            "access_point"
            "gateway"
            "roaming"
            "boundary"
          ];
          default = "access_point";
          description = "Reticulum interface mode for the RNode radio link.";
        };

        frequency = lib.mkOption {
          type = lib.types.ints.positive;
          default = 915000000;
          description = "RNode LoRa frequency in Hz.";
        };

        bandwidth = lib.mkOption {
          type = lib.types.ints.positive;
          default = 125000;
          description = "RNode LoRa bandwidth in Hz.";
        };

        txPower = lib.mkOption {
          type = lib.types.int;
          default = 7;
          description = "RNode transmit power in dBm.";
        };

        spreadingFactor = lib.mkOption {
          type = lib.types.ints.between 7 12;
          default = 8;
          description = "RNode LoRa spreading factor.";
        };

        codingRate = lib.mkOption {
          type = lib.types.ints.between 5 8;
          default = 5;
          description = "RNode LoRa coding rate.";
        };

        enableTransport = lib.mkOption {
          type = lib.types.bool;
          default = true;
          description = "Enable Reticulum transport for this appliance instance.";
        };

        shareInstance = lib.mkOption {
          type = lib.types.bool;
          default = true;
          description = "Expose the local Reticulum shared instance socket for other Reticulum programs on the appliance.";
        };

        sharedInstancePort = lib.mkOption {
          type = lib.types.port;
          default = 37428;
          description = "Reticulum shared instance TCP port.";
        };

        discoverable = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Publish Reticulum interface discovery announces for this RNode.";
        };

        flowControl = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Enable RNode packet flow control for devices with small internal buffers.";
        };

        idCallsign = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "N0CALL-0";
          description = "Optional Reticulum RNode identification callsign.";
        };

        idInterval = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.positive;
          default = null;
          example = 600;
          description = "Optional RNode identification interval in seconds.";
        };

        configExtra = lib.mkOption {
          type = lib.types.lines;
          default = "";
          description = "Additional lines appended to the rendered RNode interface configuration.";
        };
      };
    };
  };

  config = lib.mkIf cfg.enable {
    services.mish.matrix.provider = lib.mkDefault (
      if cfg.profile == "sbc-rpi4" then "conduit" else "tuwunel"
    );
    services.mish.matrix.domain = lib.mkDefault cfg.domain;
    services.mish.matrix.providers.tuwunel.enable = lib.mkDefault (cfg.matrix.provider == "tuwunel");
    services.mish.matrix.providers.conduit.enable = lib.mkDefault (cfg.matrix.provider == "conduit");

    environment.systemPackages = [ cfg.package ];

    users.groups.mish = { };
    users.users.mish = {
      isSystemUser = true;
      group = "mish";
      home = cfg.stateDir;
      createHome = true;
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0700 mish mish -"
      "d ${cfg.stateDir}/waypoints 0700 mish mish -"
      "d ${cfg.stateDir}/geopacks 0700 mish mish -"
      "d ${cfg.stateDir}/keys 0700 mish mish -"
      "d ${cfg.stateDir}/nostr 0700 mish mish -"
      "d ${cfg.stateDir}/openziti 0700 mish mish -"
      "d ${cfg.stateDir}/reticulum 0700 mish mish -"
    ];
  };
}
