"""Check release identity without evaluating or building the Nix flake."""
import argparse
import os
from pathlib import Path
import re
import tomllib


def validate_metadata(root, tag=None):
    package = tomllib.loads((root / 'apps/mish/Cargo.toml').read_text())['package']
    version = package['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-[a-zA-Z0-9.-]+)?', version):
        raise ValueError('invalid release version')
    expected = f'v{version}'
    if tag is not None and tag != expected:
        raise ValueError(f'release tag must match package version: {expected}')
    locked = [item['version'] for item in tomllib.loads(
        (root / 'apps/mish/Cargo.lock').read_text())['package'] if item['name'] == 'mish']
    nix = re.findall(r'^\s*version = "([^"]+)";',
                     (root / 'pkgs/mish-control/default.nix').read_text(), re.MULTILINE)
    if locked != [version] or nix != [version]:
        raise ValueError('Cargo manifest, lockfile and Nix package versions must agree')
    notes = root / 'docs/releases' / f'{expected}.md'
    if not notes.is_file() or not notes.read_text().strip():
        raise ValueError('curated release notes are required')
    if f'## [{version}]' not in (root / 'CHANGELOG.md').read_text():
        raise ValueError('versioned changelog entry is required')
    return notes


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=Path('.'))
    parser.add_argument('--tag', default=os.environ.get('GITHUB_REF_NAME')
                        if os.environ.get('GITHUB_REF_TYPE') == 'tag' else None)
    args = parser.parse_args()
    print(validate_metadata(args.root, args.tag))


if __name__ == '__main__':
    main()
