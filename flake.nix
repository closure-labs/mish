{
  description = "VestaForge 0.1 alpha NixOS appliance";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
  };

  outputs = inputs: import ./nix inputs;
}
