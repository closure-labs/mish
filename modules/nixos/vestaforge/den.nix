{ config, lib, ... }:

let
  cfg = config.services.vestaforge;
in
{
  options.den = {
    aspects = lib.mkOption {
      type = lib.types.attrsOf lib.types.anything;
      default = { };
      description = "Internal Den-compatible provider aspect data.";
    };

    quirks.meshServices = lib.mkOption {
      type = lib.types.attrsOf lib.types.anything;
      default = { };
      description = "Stable Den-compatible mesh service quirk data.";
    };
  };

  config = lib.mkIf cfg.enable {
    den.aspects.profile = {
      name = cfg.profile;
      releaseGated = cfg.profile == "mini-pc";
      profiles = [
        "mini-pc"
        "sbc-rpi4"
        "home-server"
      ];
    };

    den.quirks.meshServices.profile = {
      name = cfg.profile;
      releaseGated = cfg.profile == "mini-pc";
      primaryAcceptanceTarget = cfg.profile == "mini-pc";
    };
  };
}
