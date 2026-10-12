#!/usr/bin/env bash
set -euo pipefail
mkdir -p artifacts
: > artifacts/image-roots.txt
read -r -a image_targets <<< "${TARGETS:?Target package list required}"
for target in "${image_targets[@]}"; do
  path=$(nix build --no-link --print-out-paths --accept-flake-config ".#packages.$SYSTEM.$target")
  printf '%s\n' "$path" >> artifacts/image-roots.txt
  if [ -d "$path/iso" ]; then
    cp "$path"/iso/*.iso artifacts/
  elif [ -d "$path/sd-image" ]; then
    cp "$path"/sd-image/* artifacts/
  else
    closure=$(nix-store -qR "$path")
    mapfile -t closure_paths <<< "$closure"
    nix-store --export "${closure_paths[@]}" | gzip > "artifacts/$target-$SYSTEM.nar.gz"
  fi
  printf '%s %s\n' "$target" "$path" >> artifacts/build-manifest.txt
done
git rev-parse HEAD > artifacts/source-revision.txt
cp flake.lock artifacts/flake.lock
cp apps/mish/Cargo.lock artifacts/Cargo.lock
cp CHANGELOG.md artifacts/
cp docs/commissioning.md docs/operator-runbook.md docs/acceptance.md docs/roadmap.md artifacts/
tar -czf artifacts/operating-docs.tar.gz docs
cd artifacts
checksums_file=$(mktemp)
find . -maxdepth 1 -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > "$checksums_file"
mv "$checksums_file" SHA256SUMS
