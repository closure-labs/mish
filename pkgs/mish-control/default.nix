{
  lib,
  makeWrapper,
  rustPlatform,
  pkg-config,
  sqlite,
  rustfmt,
  clippy,
}:

rustPlatform.buildRustPackage {
  pname = "mish-control";
  version = "0.1.0-alpha";

  src = lib.cleanSource ../../apps/mish;
  cargoLock.lockFile = ../../apps/mish/Cargo.lock;

  nativeBuildInputs = [
    makeWrapper
    pkg-config
    rustfmt
    clippy
  ];
  buildInputs = [ sqlite ];
  cargoTestFlags = [ "--all-targets" ];
  preCheck = ''
    cargo fmt --all -- --check
    cargo clippy --all-targets --offline -- -D warnings
  '';

  postInstall = ''
    install -Dm644 LICENSE "$out/share/licenses/mish-control/LICENSE"
    makeWrapper "$out/bin/mish" "$out/bin/mish-matrix-stub" \
      --add-flags "stub matrix"
    makeWrapper "$out/bin/mish" "$out/bin/mish-openziti-stub" \
      --add-flags "stub openziti"
  '';

  meta = {
    description = "Mish offline event coordination service and CLI";
    homepage = "https://github.com/closure-labs/mish";
    license = lib.licenses.agpl3Only;
    mainProgram = "mish";
    platforms = lib.platforms.unix;
  };
}
