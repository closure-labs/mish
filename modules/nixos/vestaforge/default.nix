{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.vestaforge;
in
{
  imports = [
    ./control.nix
    ./den.nix
    ./geopack.nix
    ./matrix.nix
    ./openziti.nix
    ./radio.nix
  ];

  options.services.vestaforge = {
    enable = lib.mkEnableOption "the VestaForge alpha appliance control plane";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.vestaforge-control;
      defaultText = lib.literalExpression "pkgs.vestaforge-control";
      description = "Package providing vfctl and VestaForge local service helpers.";
    };

    profile = lib.mkOption {
      type = lib.types.enum [
        "mini-pc"
        "sbc-rpi4"
        "home-server"
      ];
      default = "mini-pc";
      description = "Deployment profile selected for appliance composition.";
    };

    domain = lib.mkOption {
      type = lib.types.str;
      default = "vestaforge.local";
      description = "Primary appliance domain used by service capability modules.";
    };

    stateDir = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/vestaforge";
      description = "Persistent mutable state directory for keys, waypoints, GeoPacks, and publication outbox data.";
    };

    api = {
      bindHost = lib.mkOption {
        type = lib.types.str;
        default = "127.0.0.1";
        description = "Address for the local control API. OpenZiti is expected to expose this service remotely.";
      };

      port = lib.mkOption {
        type = lib.types.port;
        default = 8787;
        description = "Port for the local VestaForge control API.";
      };
    };

    matrix = {
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
        defaultText = lib.literalExpression "config.services.vestaforge.domain";
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
        default = true;
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
            "meshtastic-simulated"
            "reticulum-rnode"
          ]
        );
        default = [ "meshtastic-simulated" ];
        description = "Radio edge providers enabled for waypoint and field-network integration.";
      };

      simulator.enable = lib.mkOption {
        type = lib.types.bool;
        default = builtins.elem "meshtastic-simulated" cfg.radio.providers;
        defaultText = lib.literalExpression ''builtins.elem "meshtastic-simulated" config.services.vestaforge.radio.providers'';
        description = "Enable simulator-first Meshtastic-like waypoint ingest.";
      };

      reticulumRnode = {
        enable = lib.mkOption {
          type = lib.types.bool;
          default = builtins.elem "reticulum-rnode" cfg.radio.providers;
          defaultText = lib.literalExpression ''builtins.elem "reticulum-rnode" config.services.vestaforge.radio.providers'';
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
          default = "VestaForge RNode";
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
    services.vestaforge.matrix.provider = lib.mkDefault (
      if cfg.profile == "sbc-rpi4" then "conduit" else "tuwunel"
    );
    services.vestaforge.matrix.domain = lib.mkDefault cfg.domain;
    services.vestaforge.matrix.providers.tuwunel.enable = lib.mkDefault (
      cfg.matrix.provider == "tuwunel"
    );
    services.vestaforge.matrix.providers.conduit.enable = lib.mkDefault (
      cfg.matrix.provider == "conduit"
    );

    environment.systemPackages = [ cfg.package ];

    users.groups.vestaforge = { };
    users.users.vestaforge = {
      isSystemUser = true;
      group = "vestaforge";
      home = cfg.stateDir;
      createHome = true;
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.stateDir} 0700 vestaforge vestaforge -"
      "d ${cfg.stateDir}/waypoints 0700 vestaforge vestaforge -"
      "d ${cfg.stateDir}/geopacks 0700 vestaforge vestaforge -"
      "d ${cfg.stateDir}/keys 0700 vestaforge vestaforge -"
      "d ${cfg.stateDir}/nostr 0700 vestaforge vestaforge -"
      "d ${cfg.stateDir}/openziti 0700 vestaforge vestaforge -"
      "d ${cfg.stateDir}/reticulum 0700 vestaforge vestaforge -"
    ];
  };
}
