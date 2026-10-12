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
    supply-drop-bbs = final.callPackage ../pkgs/supply-drop-bbs { };
    retibbs = final.callPackage ../pkgs/retibbs { };
  };
  pkgsFor =
    system:
    import nixpkgs {
      inherit system;
      overlays = [ overlay ];
      config.allowUnfreePredicate =
        pkg:
        builtins.elem (lib.getName pkg) [
          "supply-drop-bbs"
          "rns"
          "lxmf"
        ];
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
