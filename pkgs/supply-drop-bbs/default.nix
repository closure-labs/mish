{
  lib,
  rustPlatform,
  fetchFromGitHub,
  buildNpmPackage,
  importNpmLock,
  pkg-config,
  openssl,
}:
let
  src = fetchFromGitHub {
    owner = "Mesh-America";
    repo = "supply-drop-bbs";
    rev = "470df11119cc05b0b5d7530f246855829ca5a7a1";
    hash = "sha256-ystiZFBov+gGs5JkL7Mm0GeIy4xbmOAz/U4yl/3rmpg=";
  };
  frontend = buildNpmPackage {
    pname = "supply-drop-bbs-admin-assets";
    version = "1.1.6";
    inherit src;
    sourceRoot = "source/crates/bbs-web/web";
    npmDeps = importNpmLock { npmRoot = ./.; };
    npmConfigHook = importNpmLock.npmConfigHook;
    installPhase = ''mkdir -p "$out"; cp -r dist/. "$out/"'';
  };
in
rustPlatform.buildRustPackage {
  pname = "supply-drop-bbs";
  version = "1.1.6";
  inherit src;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ openssl ];
  env.SQLX_OFFLINE = "true";
  env.BBS_WEB_SKIP_NPM_BUILD = "1";
  postPatch = ''
    # Native Nix compiler wrappers own the linker on both supported architectures.
    rm .cargo/config.toml
    mkdir -p crates/bbs-web/web/dist
    cp -r ${frontend}/. crates/bbs-web/web/dist/
  '';
  postInstall = ''
    install -Dm644 LICENSE "$out/share/licenses/supply-drop-bbs/LICENSE"
    install -Dm644 NOTICE "$out/share/licenses/supply-drop-bbs/NOTICE"
    install -Dm644 config.example.toml "$out/share/supply-drop-bbs/config.example.toml"
  '';
  meta = {
    description = "Optional MeshCore and Meshtastic radio BBS with separate operator UI";
    homepage = "https://github.com/Mesh-America/supply-drop-bbs";
    license = {
      fullName = "Apache-2.0 with Commons Clause";
      url = "https://github.com/Mesh-America/supply-drop-bbs/blob/470df11119cc05b0b5d7530f246855829ca5a7a1/LICENSE";
      free = false;
      redistributable = true;
    };
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "supply-drop-bbs";
  };
}
