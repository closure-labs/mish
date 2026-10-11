#!/usr/bin/env bash
set -euo pipefail
gh release create "$GITHUB_REF_NAME" --draft --verify-tag --title "$GITHUB_REF_NAME" --notes-file docs/operator-runbook.md
for directory in artifacts/*; do
  label=$(basename "$directory")
  for asset in "$directory"/*; do
    release_asset="artifacts/$label-$(basename "$asset")"
    if [ "$(basename "$asset")" = SHA256SUMS ]; then
      sed "s|  \./|  $label-|" "$asset" > "$release_asset"
    else
      cp "$asset" "$release_asset"
    fi
    gh release upload "$GITHUB_REF_NAME" "$release_asset"
  done
done
gh release upload "$GITHUB_REF_NAME" docs/operator-runbook.md docs/commissioning.md
