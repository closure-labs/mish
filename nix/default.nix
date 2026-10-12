inputs:
let
  lib = inputs.nixpkgs.lib;
in
(lib.evalModules {
  specialArgs = { inherit inputs; };
  modules = [
    inputs.den.flakeModule
    inputs.den.flakeOutputs.all
    ./den
  ];
}).config.flake
