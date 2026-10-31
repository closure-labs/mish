#!/usr/bin/env bash
set -euo pipefail

# Called after successful checks, within nix develop. Builds exactly the native
# validation aggregate for complete evidence; no cross compilation.
system=${SYSTEM:-$(nix eval --impure --raw --expr builtins.currentSystem)}
mkdir -p artifacts
nix build --no-link --print-out-paths --accept-flake-config \
  ".#packages.$system.mish-control" \
  ".#packages.$system.mish-validation" > artifacts/validation-roots.txt
proof=$(nix path-info --accept-flake-config ".#packages.$system.mish-validation")
cp "$proof/manifest.json" artifacts/validation-manifest.json
git rev-parse HEAD > artifacts/validation-source-revision.txt

# Select and measure separately from the complete validation/image evidence.
python3 "$(dirname "${BASH_SOURCE[0]}")/cache-policy.py"

# Finalize checksums after adding evidence to any image artifacts.
checksums_file=$(mktemp)
(
  cd artifacts
  find . -maxdepth 1 -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum
) > "$checksums_file"
mv "$checksums_file" artifacts/SHA256SUMS
