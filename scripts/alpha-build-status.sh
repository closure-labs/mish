#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/alpha-build-status.sh [--full]

Runs the VestaForge alpha build-readiness checks that are safe on the current
host. By default this performs all-system evaluation, host-native checks, and
Linux image dry-runs. Use --full to require real Linux builds through a
configured Linux builder.
USAGE
}

full=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --full)
      full=1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
  shift
done

cd "$(dirname "$0")/.."

expect_eval_json() {
  local label="$1"
  local expr="$2"
  local expected="$3"
  local actual

  actual="$(nix eval --json "${expr}")"
  if [ "${actual}" != "${expected}" ]; then
    echo "check-failed: ${label}: expected ${expected}, got ${actual}" >&2
    exit 1
  fi
  echo "ok: ${label}=${actual}"
}

dry_run_image() {
  local label="$1"
  local attr="$2"
  local log

  log="$(mktemp "${TMPDIR:-/tmp}/vestaforge-${label}.XXXXXX")"
  if nix build "${attr}" --dry-run >"${log}" 2>&1; then
    echo "ok: ${label} dry-run"
    grep -E '^(this|these) [0-9]+ (derivation|derivations|path|paths)' "${log}" || true
  else
    cat "${log}" >&2
    exit 1
  fi
}

current_system="$(nix eval --impure --raw --expr builtins.currentSystem)"
builders="$(nix config show builders 2>/dev/null | sed 's/^builders = //')"
extra_platforms="$(nix config show extra-platforms 2>/dev/null | sed 's/^extra-platforms = //')"
external_builders="$(nix config show external-builders 2>/dev/null | sed 's/^external-builders = //')"

echo "current-system: ${current_system}"
echo "extra-platforms: ${extra_platforms:-<none>}"
echo "builders: ${builders:-<none>}"
echo "external-builders: ${external_builders:-<none>}"
if [ "${builders}" = "@/etc/nix/machines" ] && [ ! -e /etc/nix/machines ]; then
  echo "builder-status: /etc/nix/machines is not present"
fi

echo "== all-system flake evaluation =="
nix flake check --no-build --all-systems

echo "== alpha profile assertions =="
expect_eval_json "mini-pc.profile" ".#nixosConfigurations.vestaforge-mini-pc.config.services.vestaforge.profile" '"mini-pc"'
expect_eval_json "mini-pc.matrix.provider" ".#nixosConfigurations.vestaforge-mini-pc.config.services.vestaforge.matrix.provider" '"tuwunel"'
expect_eval_json "mini-pc.matrix.backend" ".#nixosConfigurations.vestaforge-mini-pc.config.den.quirks.meshServices.matrix.backend" '"tuwunel"'
expect_eval_json "mini-pc.profile.releaseGated" ".#nixosConfigurations.vestaforge-mini-pc.config.den.quirks.meshServices.profile.releaseGated" "true"
expect_eval_json "rpi4.profile" ".#nixosConfigurations.vestaforge-rpi4.config.services.vestaforge.profile" '"sbc-rpi4"'
expect_eval_json "rpi4.matrix.provider" ".#nixosConfigurations.vestaforge-rpi4.config.services.vestaforge.matrix.provider" '"conduit"'
expect_eval_json "rpi4.matrix.backend" ".#nixosConfigurations.vestaforge-rpi4.config.den.quirks.meshServices.matrix.backend" '"conduit"'
expect_eval_json "rpi4.profile.releaseGated" ".#nixosConfigurations.vestaforge-rpi4.config.den.quirks.meshServices.profile.releaseGated" "true"

case "${current_system}" in
  aarch64-darwin)
    echo "== host-native Darwin package and checks =="
    nix build .#packages.aarch64-darwin.vestaforge-control --no-link
    nix build .#checks.aarch64-darwin.schema --no-link
    nix build .#checks.aarch64-darwin.cli --no-link
    ;;
  *)
    echo "host-native Darwin checks skipped for ${current_system}"
    ;;
esac

echo "== Linux image closure planning =="
dry_run_image "mini-pc-image" ".#packages.x86_64-linux.vestaforge-mini-pc-image"
dry_run_image "rpi4-image" ".#packages.aarch64-linux.vestaforge-rpi4-image"

if [ "${full}" -eq 1 ]; then
  echo "== Linux builder-backed checks =="
  cat <<'NOTE'
full-build-status: attempting
This accepts either classic Nix buildMachines or Determinate Nix native
external builders. If no Linux-capable builder is available, Nix will fail with
"required system or feature not available".
NOTE
  nix build .#checks.x86_64-linux.cli --no-link
  nix build .#checks.aarch64-linux.module-eval --no-link
  nix build .#packages.x86_64-linux.vestaforge-mini-pc-image --no-link
  nix build .#packages.aarch64-linux.vestaforge-rpi4-image --no-link
else
  cat <<'NOTE'
full-build-status: skipped
Run scripts/alpha-build-status.sh --full after configuring a Linux builder to
realize the x86_64-linux Mini-PC VM and aarch64-linux Raspberry Pi SD image.
NOTE
fi
