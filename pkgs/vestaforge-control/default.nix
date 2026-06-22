{
  lib,
  makeWrapper,
  python3,
  stdenvNoCC,
}:

stdenvNoCC.mkDerivation {
  pname = "vestaforge-control";
  version = "0.1.0-alpha";

  src = ../../apps/vfctl;

  nativeBuildInputs = [ makeWrapper ];

  installPhase = ''
    runHook preInstall

    mkdir -p "$out/lib/vestaforge" "$out/bin"
    cp -R src "$out/lib/vestaforge/"

    makeWrapper ${python3}/bin/python3 "$out/bin/vfctl" \
      --set PYTHONPATH "$out/lib/vestaforge/src" \
      --add-flags "-m" \
      --add-flags "vestaforge.cli"

    makeWrapper ${python3}/bin/python3 "$out/bin/vf-matrix-stub" \
      --set PYTHONPATH "$out/lib/vestaforge/src" \
      --add-flags "-m" \
      --add-flags "vestaforge.matrix_stub"

    makeWrapper ${python3}/bin/python3 "$out/bin/vf-openziti-stub" \
      --set PYTHONPATH "$out/lib/vestaforge/src" \
      --add-flags "-m" \
      --add-flags "vestaforge.openziti_stub"

    runHook postInstall
  '';

  meta = {
    description = "VestaForge alpha control service and CLI";
    homepage = "https://example.invalid/vestaforge";
    license = lib.licenses.asl20;
    mainProgram = "vfctl";
    platforms = lib.platforms.linux;
  };
}
