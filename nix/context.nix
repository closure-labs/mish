{ inputs }:
let
  inherit (inputs) self nixpkgs;
  lib = nixpkgs.lib;
  systems = [
    "aarch64-darwin"
    "x86_64-linux"
    "aarch64-linux"
  ];
  linuxSystems = [
    "x86_64-linux"
    "aarch64-linux"
  ];
  overlay = final: _prev: {
    mish-control = final.callPackage ../pkgs/mish-control { };
  };
  pkgsFor =
    system:
    import nixpkgs {
      inherit system;
      overlays = [ overlay ];
    };
in
{
  inherit
    self
    lib
    systems
    linuxSystems
    overlay
    pkgsFor
    ;
  forAllSystems = lib.genAttrs systems;
}
