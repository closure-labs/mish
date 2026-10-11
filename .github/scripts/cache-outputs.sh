#!/usr/bin/env bash
set -euo pipefail

# Called after successful checks, within nix develop. Builds exactly the native
# validation aggregate and pinned developer shell; no cross compilation.
system=${SYSTEM:-$(nix eval --impure --raw --expr builtins.currentSystem)}
mkdir -p artifacts
nix build --no-link --print-out-paths --accept-flake-config \
  ".#packages.$system.mish-control" \
  ".#packages.$system.mish-validation" \
  ".#devShells.$system.default.inputDerivation" > artifacts/validation-roots.txt
proof=$(nix path-info --accept-flake-config ".#packages.$system.mish-validation")
cp "$proof/manifest.json" artifacts/validation-manifest.json
git rev-parse HEAD > artifacts/validation-source-revision.txt

# Finalize checksums after adding evidence to any image artifacts.
checksums_file=$(mktemp)
(
  cd artifacts
  find . -maxdepth 1 -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum
) > "$checksums_file"
mv "$checksums_file" artifacts/SHA256SUMS

# Passing explicit roots also seeds already-substituted paths. The action's
# daemon separately uploads new intermediate builds as they finish.
if [ "${MISH_CACHE_UPLOAD:-false}" = true ]; then
  mapfile -t roots < artifacts/validation-roots.txt
  if [ -f artifacts/image-roots.txt ]; then
    mapfile -t image_roots < artifacts/image-roots.txt
    roots+=("${image_roots[@]}")
  fi
  cachix push mish "${roots[@]}"
fi
