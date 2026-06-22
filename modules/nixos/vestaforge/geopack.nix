{ config, lib, ... }:

let
  cfg = config.services.vestaforge;
in
{
  config = lib.mkIf (cfg.enable && cfg.geopack.enable) {
    den.quirks.meshServices.geopack = {
      enabled = true;
      localUrl = "http://${cfg.api.bindHost}:${toString cfg.api.port}";
      signingKeyPath = "${cfg.stateDir}/keys/geopack-signing.key";
      publication = {
        publicOnly = true;
        relays = cfg.nostr.relays;
      };
    };

    den.quirks.meshServices.radio = {
      simulatorEnabled = cfg.radio.simulator.enable;
      hardwareMeshtasticEnabled = false;
    };
  };
}
