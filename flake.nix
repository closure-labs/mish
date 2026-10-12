{
  description = "Mish: offline event coordination on NixOS";

  inputs = {
    den.url = "github:denful/den/65c9fd40e958109f5a352c1a93ae8254bf1ed29f";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    nixos-raspberrypi.url = "github:nvmd/nixos-raspberrypi/main";
  };

  nixConfig = {
    extra-substituters = [
      "https://mish.cachix.org"
      "https://nixos-raspberrypi.cachix.org"
    ];
    extra-trusted-public-keys = [
      "mish.cachix.org-1:TVqZ5PamlcnYEpNEJtBz7xNmS7H5X/gXQXYVeDBD4FY="
      "nixos-raspberrypi.cachix.org-1:4iMO9LXa8BqhU+Rpg6LQKiGa2lsNh/j2oiYLNOQ5sPI="
    ];
  };

  outputs = inputs: import ./nix inputs;
}
