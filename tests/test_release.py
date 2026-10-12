"""Release preparation stays draft-only and binds the payload to tagged source."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(os.environ.get('MISH_CI_ROOT', Path(__file__).parents[1]))
spec = importlib.util.spec_from_file_location('release_metadata', ROOT / '.github/scripts/release_metadata.py')
metadata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metadata)
REVISION = 'a' * 40
TAG = 'v0.1.0-alpha'


class ReleaseTests(unittest.TestCase):
    def fixture(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for name in ['.github', 'apps/mish', 'pkgs/mish-control', 'docs/releases']:
            (root / name).mkdir(parents=True, exist_ok=True)
        shutil.copytree(ROOT / '.github', root / '.github', dirs_exist_ok=True)
        for name in ['apps/mish/Cargo.toml', 'apps/mish/Cargo.lock', 'pkgs/mish-control/default.nix',
                     'LICENSE', 'CHANGELOG.md', f'docs/releases/{TAG}.md', 'docs/commissioning.md',
                     'docs/operator-runbook.md', 'docs/acceptance.md', 'flake.lock']:
            shutil.copyfile(ROOT / name, root / name)
        # Copied Nix store fixtures must be writable only inside our temporary directory.
        for path in root.rglob('*'):
            path.chmod(0o700 if path.is_dir() else 0o600)
        return root

    def test_metadata_matches_and_rejects_wrong_tag_or_version(self):
        root = self.fixture()
        self.assertEqual(metadata.validate_metadata(root, TAG).name, f'{TAG}.md')
        with self.assertRaisesRegex(ValueError, 'tag must match'):
            metadata.validate_metadata(root, 'v9.0.0')
        path = root / 'pkgs/mish-control/default.nix'
        path.write_text(path.read_text().replace('0.1.0-alpha', '0.1.0'))
        with self.assertRaisesRegex(ValueError, 'versions must agree'):
            metadata.validate_metadata(root, TAG)

    def test_missing_notes_or_changelog_entry_is_rejected(self):
        for name in [f'docs/releases/{TAG}.md', 'CHANGELOG.md']:
            with self.subTest(name=name):
                root = self.fixture()
                (root / name).write_text('')
                with self.assertRaises(ValueError):
                    metadata.validate_metadata(root, TAG)

    def run_draft(self, release=None, api_failure=False, wrong_tag=False, upload_failure=False):
        root = self.fixture()
        for system in ['x86_64-linux', 'aarch64-linux']:
            artifact = root / 'artifacts' / f'images-{system}'
            artifact.mkdir(parents=True)
            (artifact / 'source-revision.txt').write_text(REVISION)
            (artifact / 'validation-source-revision.txt').write_text(REVISION)
            shutil.copyfile(root / 'flake.lock', artifact / 'flake.lock')
            shutil.copyfile(root / 'apps/mish/Cargo.lock', artifact / 'Cargo.lock')
            (artifact / 'validation-manifest.json').write_text(json.dumps({
                'system': system, 'lockHash': hashlib.sha256((root / 'flake.lock').read_bytes()).hexdigest()}))
            targets = (['mish-mini-pc-image', 'mish-mini-pc-installer']
                       if system == 'x86_64-linux' else ['mish-rpi4-image'])
            (artifact / 'build-manifest.txt').write_text('\n'.join(
                f'{target} /nix/store/fixture-{target}' for target in targets))
            for name in (['installer.iso', f'mish-mini-pc-image-{system}.nar.gz']
                         if system == 'x86_64-linux' else ['sd.img.zst']):
                (artifact / name).write_bytes(b'fixture image')
            (artifact / 'SHA256SUMS').write_text('\n'.join(
                f'{hashlib.sha256(path.read_bytes()).hexdigest()}  ./{path.name}'
                for path in artifact.iterdir()) + '\n')
        binary = root / 'bin'
        binary.mkdir()
        scripts = {
            'git': 'import os, sys\nprint(os.environ["TEST_REVISION"] if "HEAD" in sys.argv else os.environ["TEST_TAG_REVISION"])\n',
            'gh': '''import json, os, pathlib, sys
args = sys.argv[1:]
with open(os.environ['TEST_CALLS'], 'a') as stream: stream.write(json.dumps(args) + '\\n')
if args[0] == 'api':
    if os.environ['TEST_API_FAILURE'] == 'true': sys.exit(7)
    print(os.environ['TEST_RELEASES'])
elif args[:2] == ['release', 'upload']:
    if os.environ['TEST_UPLOAD_FAILURE'] == 'true': sys.exit(8)
    asset = pathlib.Path(args[3])
    pathlib.Path(os.environ['TEST_UPLOADS'], asset.name).write_bytes(asset.read_bytes())
elif args[:2] in (['release', 'create'], ['release', 'edit']):
    notes = pathlib.Path(args[args.index('--notes-file') + 1]).read_text()
    assert os.environ['TEST_REVISION'] in notes
    assert 'actions/runs/123' in notes
elif args[:2] == ['release', 'view']:
    print(json.dumps(dict(url='https://example.invalid/draft', isDraft=True,
                         isPrerelease=True, targetCommitish=os.environ['TEST_REVISION'])))
''',
        }
        for name, content in scripts.items():
            path = binary / name
            path.write_text(f'#!{sys.executable}\n' + content)
            path.chmod(0o700)
        uploads = root / 'uploads'
        uploads.mkdir()
        environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                           GITHUB_REF_NAME=TAG, GITHUB_REPOSITORY='closure-labs/mish', GITHUB_RUN_ID='123',
                           TEST_REVISION=REVISION, TEST_TAG_REVISION='b' * 40 if wrong_tag else REVISION,
                           TEST_CALLS=str(root / 'calls.jsonl'), TEST_UPLOADS=str(uploads),
                           TEST_API_FAILURE=str(api_failure).lower(),
                           TEST_UPLOAD_FAILURE=str(upload_failure).lower(),
                           TEST_RELEASES=json.dumps([[release] if release else []]))
        result = subprocess.run(['bash', '.github/scripts/draft-release.sh'], cwd=root,
                                env=environment, capture_output=True, text=True)
        calls = ([json.loads(line) for line in (root / 'calls.jsonl').read_text().splitlines()]
                 if (root / 'calls.jsonl').exists() else [])
        return root, result, calls

    def test_new_release_is_draft_prerelease_with_checksummed_assets(self):
        root, result, calls = self.run_draft()
        self.assertEqual(result.returncode, 0, result.stderr)
        create = next(call for call in calls if call[:2] == ['release', 'create'])
        for flag in ['--draft', '--prerelease', '--verify-tag', '--latest=false']:
            self.assertIn(flag, create)
        self.assertEqual(create[create.index('--target') + 1], REVISION)
        uploads = root / 'uploads'
        checked = set()
        for line in (uploads / 'SHA256SUMS').read_text().splitlines():
            digest, filename = line.split('  ', 1)
            path = uploads / filename
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), digest)
            checked.add(path.name)
        self.assertEqual(checked, {path.name for path in uploads.iterdir()} - {'SHA256SUMS'})
        self.assertEqual({path.name for path in (root / 'artifacts').iterdir()},
                         {'images-x86_64-linux', 'images-aarch64-linux'})

    def test_partial_draft_for_same_source_can_resume(self):
        release = {'tag_name': TAG, 'draft': True, 'prerelease': True, 'target_commitish': REVISION}
        _, result, calls = self.run_draft(release)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any(call[:2] == ['release', 'edit'] for call in calls))
        self.assertTrue(all('--draft' in call for call in calls if call[:2] == ['release', 'edit']))
        self.assertFalse(any(call[:2] == ['release', 'create'] for call in calls))
        uploads = [call for call in calls if call[:2] == ['release', 'upload']]
        self.assertTrue(uploads)
        self.assertTrue(all('--clobber' in call for call in uploads))

    def test_failed_upload_leaves_draft_and_stops(self):
        _, result, calls = self.run_draft(upload_failure=True)
        self.assertEqual(result.returncode, 8)
        create = next(call for call in calls if call[:2] == ['release', 'create'])
        self.assertIn('--draft', create)
        self.assertEqual(sum(call[:2] == ['release', 'upload'] for call in calls), 1)
        self.assertFalse(any(call[:2] in (['release', 'edit'], ['release', 'delete']) for call in calls))

    def test_published_wrong_source_and_failed_api_cannot_mutate(self):
        for changes in [{'draft': False}, {'target_commitish': 'b' * 40}, {'prerelease': False}]:
            with self.subTest(changes=changes):
                release = dict(tag_name=TAG, draft=True, prerelease=True, target_commitish=REVISION)
                release.update(changes)
                _, result, calls = self.run_draft(release)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any(call[0] == 'release' for call in calls))
        _, result, calls = self.run_draft(api_failure=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(call[0] == 'release' for call in calls))

    def test_tag_pointing_elsewhere_cannot_reach_github(self):
        _, result, calls = self.run_draft(wrong_tag=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])
