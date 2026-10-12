{ config, lib, ... }:

let
  cfg = config.services.mish;
  alphaReleaseProfiles = [
    "mini-pc"
    "sbc-rpi4"
  ];
  knownProfiles = [
    "mini-pc"
    "sbc-rpi4"
    "sbc-rpi5-nvme"
    "home-server"
  ];
  isAlphaReleaseProfile = lib.elem cfg.profile alphaReleaseProfiles;
in
{
  options.den = {
    aspects = lib.mkOption {
      type = lib.types.attrsOf lib.types.anything;
      default = { };
      description = "Legacy NixOS capability metadata retained for compatibility; genuine Den aspects are composed at flake scope.";
    };

    quirks.meshServices = lib.mkOption {
      type = lib.types.attrsOf lib.types.anything;
      default = { };
      description = "Legacy NixOS mesh capability metadata retained for compatibility with existing module consumers.";
    };
  };

  config = lib.mkIf cfg.enable {
    den.aspects.profile = {
      name = cfg.profile;
      releaseGated = isAlphaReleaseProfile;
      profiles = knownProfiles;
      experimental = cfg.profile == "sbc-rpi5-nvme";
    };

    den.quirks.meshServices.profile = {
      name = cfg.profile;
      releaseGated = isAlphaReleaseProfile;
      primaryAcceptanceTarget = isAlphaReleaseProfile;
      experimental = cfg.profile == "sbc-rpi5-nvme";
    };
  };
}
