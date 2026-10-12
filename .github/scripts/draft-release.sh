#!/usr/bin/env bash
set -euo pipefail
notes=$(python .github/scripts/release_metadata.py --tag "${GITHUB_REF_NAME:?Release tag required}")
revision=$(git rev-parse HEAD)
test "$(git rev-parse "refs/tags/$GITHUB_REF_NAME^{commit}")" = "$revision"
# Verify the payload again at the mutation boundary, including both native proofs.
python .github/scripts/qualification.py --artifact-root artifacts
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
for directory in artifacts/images-*; do
  label=$(basename "$directory")
  for asset in "$directory"/*; do
    release_asset="$staging/$label-$(basename "$asset")"
    if [ "$(basename "$asset")" = SHA256SUMS ]; then
      sed "s|  \./|  $label-|" "$asset" > "$release_asset"
    else
      cp --reflink=auto "$asset" "$release_asset"
    fi
  done
done
cp LICENSE CHANGELOG.md "$notes" docs/operator-runbook.md docs/commissioning.md docs/acceptance.md "$staging/"
cat "$notes" > "$staging/release-notes.md"
printf '\nTagged source: `%s`\n\nRelease validation: https://github.com/%s/actions/runs/%s\n' \
  "$revision" "$GITHUB_REPOSITORY" "${GITHUB_RUN_ID:?Release run required}" >> "$staging/release-notes.md"
(cd "$staging" && sha256sum --check images-x86_64-linux-SHA256SUMS && sha256sum --check images-aarch64-linux-SHA256SUMS)
# One list also covers the independently attached documentation and checksum lists.
(cd "$staging" && sha256sum ./* > SHA256SUMS)
# Fail before creating a draft if an individual upload exceeds GitHub's 2 GiB limit.
python - "$staging" <<'PY'
from pathlib import Path
import sys
for path in Path(sys.argv[1]).iterdir():
    if path.stat().st_size >= 2 * 1024**3:
        raise SystemExit(f'Release asset is too large: {path.name}; prepare a smaller artifact before tagging.')
PY
# API/list failures remain failures, rather than being mistaken for an absent release.
existing=$(gh api --paginate --slurp "repos/${GITHUB_REPOSITORY:?Repository required}/releases" |
  python -c 'import json, sys
tag, revision = sys.argv[1:]
matches = [r for page in json.load(sys.stdin) for r in page if r["tag_name"] == tag]
if len(matches) > 1: raise SystemExit("Duplicate release tag")
if matches:
    release = matches[0]
    if not release["draft"] or not release["prerelease"] or release["target_commitish"] != revision:
        raise SystemExit("Refusing to modify a published release or a draft for another revision")
    print("draft")
' "$GITHUB_REF_NAME" "$revision")
if [ "$existing" = draft ]; then
  gh release edit "$GITHUB_REF_NAME" --draft --title "Mish $GITHUB_REF_NAME" --notes-file "$staging/release-notes.md" --prerelease --latest=false
else
  gh release create "$GITHUB_REF_NAME" --draft --prerelease --latest=false --verify-tag \
    --target "$revision" --title "Mish $GITHUB_REF_NAME" --notes-file "$staging/release-notes.md"
fi
# Only an existing draft for this exact tagged revision can reach replacement.
for asset in "$staging"/*; do
  gh release upload "$GITHUB_REF_NAME" "$asset" --clobber
done
gh release view "$GITHUB_REF_NAME" --json url,isDraft,isPrerelease,targetCommitish |
  python -c 'import json, sys
release = json.load(sys.stdin)
if not release["isDraft"] or not release["isPrerelease"] or release["targetCommitish"] != sys.argv[1]:
    raise SystemExit("Release did not retain its draft prerelease identity")
print(json.dumps(release))
' "$revision"
