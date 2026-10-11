{
  lib,
  makeWrapper,
  python3,
  stdenvNoCC,
  withMeshtastic ? true,
}:

let
  python = python3.withPackages (
    ps:
    [ ps.aiohttp ]
    ++ lib.optionals (withMeshtastic && stdenvNoCC.hostPlatform.isLinux) [ ps.meshtastic ]
  );
in
stdenvNoCC.mkDerivation {
  pname = "mish-control";
  version = "0.1.0-alpha";

  src = ../../apps/mish;

  nativeBuildInputs = [ makeWrapper ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/lib/mish" "$out/bin"
    cp -R src "$out/lib/mish/"

    makeWrapper ${python}/bin/python3 "$out/bin/mish" \
      --set PYTHONPATH "$out/lib/mish/src" \
      --add-flags "-m" \
      --add-flags "mish.cli"

    makeWrapper ${python}/bin/python3 "$out/bin/mish-matrix-stub" \
      --set PYTHONPATH "$out/lib/mish/src" \
      --add-flags "-m" \
      --add-flags "mish.matrix_stub"

    makeWrapper ${python}/bin/python3 "$out/bin/mish-openziti-stub" \
      --set PYTHONPATH "$out/lib/mish/src" \
      --add-flags "-m" \
      --add-flags "mish.openziti_stub"

    runHook postInstall
  '';

  meta = {
    description = "Mish offline event coordination service and CLI";
    homepage = "https://github.com/closure-labs/mish";
    license = lib.licenses.asl20;
    mainProgram = "mish";
    platforms = lib.platforms.unix;
  };
}
