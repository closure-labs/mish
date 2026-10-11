{
  pkgs,
  system,
  checks,
  package,
  lockHash,
}:
let
  outputs = checks // {
    mish-control = package;
  };
  entries = pkgs.lib.mapAttrsToList (name: output: {
    inherit name;
    path = toString output;
    derivation = builtins.unsafeDiscardStringContext (builtins.baseNameOf output.drvPath);
  }) outputs;
in
pkgs.runCommand "mish-validation-${system}"
  {
    manifest = builtins.toJSON {
      version = 1;
      inherit system lockHash;
      outputs = entries;
      scope = "Realized Nix package/check outputs; no physical hardware qualification";
    };
    passAsFile = [ "manifest" ];
  }
  ''
    mkdir -p "$out/checks"
    cp "$manifestPath" "$out/manifest.json"
    ${pkgs.lib.concatMapStringsSep "\n" (entry: ''
      ln -s ${pkgs.lib.escapeShellArg entry.path} "$out/checks/${entry.name}"
    '') entries}
  ''
