{
  lib,
  stdenvNoCC,
  fetchFromGitHub,
  fetchurl,
  python3,
  makeWrapper,
}:
let
  bootstrapCss = fetchurl {
    url = "https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css";
    sha256 = "3c8f27e6009ccfd710a905e6dcf12d0ee3c6f2ac7da05b0572d3e0d12e736fc8";
  };
  bootstrapJs = fetchurl {
    url = "https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js";
    sha256 = "0833b2e9c3a26c258476c46266e6877fc75218625162e0460be9a3a098a61c6c";
  };
  python = python3.withPackages (ps: [
    ps.rns
    ps.lxmf
    ps.flask
    ps.rich
  ]);
in
stdenvNoCC.mkDerivation {
  pname = "retibbs";
  version = "0-unstable-2026-01-21";
  src = fetchFromGitHub {
    owner = "kc1awv";
    repo = "RetiBBS";
    rev = "7c26d2c05e945f67382695d2f8de21abdb6d7020";
    hash = "sha256-6m317adIt9fUQ1+5xU97Yo4fcBvsW9KKLipLhUtdkvE=";
  };
  nativeBuildInputs = [ makeWrapper ];
  postPatch = ''
    substituteInPlace server/web/templates/base.html \
      --replace-fail 'https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css' '/static/vendor/bootstrap.min.css' \
      --replace-fail 'https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js' '/static/vendor/bootstrap.bundle.min.js'
    mkdir -p server/web/static/vendor
    cp ${bootstrapCss} server/web/static/vendor/bootstrap.min.css
    cp ${bootstrapJs} server/web/static/vendor/bootstrap.bundle.min.js
    # A BBS is always a client of the separately supervised shared daemon.
    substituteInPlace server/retibbs_server.py \
      --replace-fail 'RNS.Reticulum(self.configpath)' 'RNS.Reticulum(self.configpath, require_shared_instance=True, shared_instance_type="tcp")' \
      --replace-fail 'input()' 'threading.Event().wait()'
  '';
  installPhase = ''
    mkdir -p "$out/lib/retibbs" "$out/bin"
    cp -r server/. "$out/lib/retibbs/"
    makeWrapper ${python}/bin/python "$out/bin/retibbs" \
      --add-flags "$out/lib/retibbs/retibbs_server.py" \
      --set PYTHONDONTWRITEBYTECODE 1
    install -Dm644 LICENSE "$out/share/licenses/retibbs/LICENSE"
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    "$out/bin/retibbs" --help
    ${python}/bin/python -c 'import RNS, LXMF, flask, rich'
  '';
  meta = {
    description = "Experimental Reticulum BBS, independent of Mish identities and rooms";
    homepage = "https://github.com/kc1awv/RetiBBS";
    license = lib.licenses.mit;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    mainProgram = "retibbs";
  };
}
