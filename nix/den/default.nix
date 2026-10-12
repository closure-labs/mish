{ inputs, ... }:
{
  imports = [
    ./aspects.nix
    ./hosts.nix
    ./outputs.nix
  ];
  den.systems = [
    "x86_64-linux"
    "aarch64-linux"
    "aarch64-darwin"
  ];
}
