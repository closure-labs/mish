"""Exercise cache upload boundaries without a network or real credentials."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = os.environ.get(
    'MISH_CACHE_SCRIPT_PATH',
    str(Path(__file__).parents[1] / '.github/scripts/cache-outputs.sh'),
)


POLICY = Path(SCRIPT).with_name('cache-policy.py')
spec = importlib.util.spec_from_file_location('cache_policy', POLICY)
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)
ROOTS = ['/nix/store/' + c * 32 + '-' + name for c, name in zip('0123', sorted(policy.SELECTED))]
DEPENDENCY = '/nix/store/' + '4' * 32 + '-lib'
MANIFEST = {'version': 1, 'system': 'x86_64-linux', 'outputs': [
    {'name': name, 'path': path} for name, path in zip(sorted(policy.SELECTED), ROOTS)]}
METADATA = [{'path': path, 'narSize': 10, 'references': [DEPENDENCY]} for path in ROOTS] + [
    {'path': DEPENDENCY, 'narSize': 20, 'references': []}]


class CacheOutputsTests(unittest.TestCase):
    def run_cache(self, upload=False, fail=False, authenticated=True, metadata=None, query_fail=False):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        binary = root / 'bin'
        binary.mkdir()
        proof = root / 'proof'
        proof.mkdir()
        (proof / 'manifest.json').write_text(json.dumps(MANIFEST))
        artifacts = root / 'artifacts'
        artifacts.mkdir()
        (artifacts / 'image-roots.txt').write_text('/nix/store/test-image\n')
        (artifacts / 'image.iso').write_bytes(b'image fixture')
        (artifacts / 'SHA256SUMS').write_text('stale checksum list\n')
        scripts = {
            'nix': '#!/usr/bin/env python3\n'
                   'import os, sys\n'
                   'if sys.argv[1] == "build":\n'
                   ' if os.environ.get("TEST_BUILD_FAIL") == "true": sys.exit(17)\n'
                   ' print("/nix/store/test-mish\\n/nix/store/test-proof")\n'
                   'elif sys.argv[1] == "path-info":\n'
                   ' if "--json" in sys.argv:\n'
                   '  if os.environ.get("TEST_QUERY_FAIL") == "true": sys.exit(23)\n'
                   '  print(os.environ["TEST_METADATA"])\n'
                   ' else: print(os.environ["TEST_PROOF"])\n'
                   'else: sys.exit(19)\n',
            'git': '#!/usr/bin/env python3\nprint("revision-fixture")\n',
            'cachix': '#!/usr/bin/env python3\n'
                      'import json, os, pathlib, sys\n'
                      'pathlib.Path(os.environ["TEST_PUSH"]).write_text(json.dumps(sys.argv[1:]))\n',
        }
        for name, content in scripts.items():
            path = binary / name
            path.write_text(content.replace('#!/usr/bin/env python3', f'#!{sys.executable}'))
            path.chmod(0o755)
        environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                           SYSTEM='x86_64-linux', TEST_PROOF=str(proof),
                           TEST_PUSH=str(root / 'push.json'),
                           TEST_BUILD_FAIL=str(fail).lower(),
                           MISH_CACHE_UPLOAD=str(upload).lower(),
                           MISH_CACHE_AUTHENTICATED=str(authenticated).lower(),
                           TEST_METADATA=json.dumps(METADATA if metadata is None else metadata),
                           TEST_QUERY_FAIL=str(query_fail).lower())
        result = subprocess.run(['bash', SCRIPT], cwd=root, env=environment,
                                capture_output=True, text=True)
        return root, result

    def test_read_only_finalizes_all_artifacts_without_upload(self):
        root, result = self.run_cache()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((root / 'push.json').exists())
        artifacts = root / 'artifacts'
        checksums = {}
        for line in (artifacts / 'SHA256SUMS').read_text().splitlines():
            digest, name = line.split('  ', 1)
            checksums[name.removeprefix('./')] = digest
        self.assertEqual(set(checksums), {p.name for p in artifacts.iterdir()
                                         if p.name != 'SHA256SUMS'})
        for name, digest in checksums.items():
            self.assertEqual(digest, hashlib.sha256((artifacts / name).read_bytes()).hexdigest())

    def test_explicit_upload_selects_native_roots_without_images_or_shell(self):
        root, result = self.run_cache(upload=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((root / 'push.json').read_text()),
                         ['push', 'mish', *ROOTS])


    def test_no_token_cannot_push_even_when_upload_requested(self):
        root, result = self.run_cache(upload=True, authenticated=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((root / 'push.json').exists())

    def test_recursive_oversize_skips_push_and_keeps_image_evidence(self):
        metadata = [dict(info) for info in METADATA]
        metadata[-1]['narSize'] = policy.MAX_NAR_BYTES
        root, result = self.run_cache(upload=True, metadata=metadata)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((root / 'push.json').exists())
        self.assertEqual(json.loads((root / 'artifacts/cache-policy.json').read_text())['status'],
                         'over-budget')
        self.assertTrue((root / 'artifacts/SHA256SUMS').exists())
        self.assertTrue((root / 'artifacts/image.iso').exists())

    def test_missing_metadata_and_query_failure_never_push(self):
        for options in [{'metadata': METADATA[:-1]}, {'query_fail': True}, {'metadata': []}]:
            with self.subTest(options=options):
                root, result = self.run_cache(upload=True, **options)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((root / 'push.json').exists())

    def test_metadata_is_local_offline_and_never_subtracts_upstream_dependencies(self):
        # Upstream outages cannot change selection: no remote query is issued.
        source = POLICY.read_text()
        self.assertIn("'--offline', '--store', 'local'", source)
        root, result = self.run_cache(upload=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads((root / 'artifacts/cache-policy.json').read_text())
        self.assertEqual((report['paths'], report['narBytes']), (5, 60))

    def test_closure_metadata_rejects_ambiguous_sizes_paths_and_extra_nodes(self):
        cases = [METADATA + [METADATA[0]], METADATA[:-1],
                 [dict(info, narSize=True) for info in METADATA],
                 [dict(info, narSize=-1) for info in METADATA],
                 METADATA + [{'path': '/nix/store/' + '5' * 32 + '-extra',
                              'narSize': 1, 'references': []}]]
        for metadata in cases:
            with self.subTest(metadata=metadata), self.assertRaises((ValueError, KeyError)):
                policy.closure_summary(metadata, ROOTS)
        self.assertEqual(policy.closure_summary(
            {info['path']: {k: v for k, v in info.items() if k != 'path'} for info in METADATA},
            ROOTS), {'paths': 5, 'narBytes': 60})

    def test_root_alias_subpath_derivation_and_symlink_rejected(self):
        for path in ['result', ROOTS[0] + '/bin', ROOTS[0] + '/../other', ROOTS[0] + '.drv']:
            with self.subTest(path=path), self.assertRaises(ValueError):
                policy.store_path(path)
        from unittest.mock import patch
        with patch.object(policy.os.path, 'realpath', return_value=ROOTS[1]):
            with self.assertRaises(ValueError):
                policy.store_path(ROOTS[0])

    def test_count_budget_checks_full_closure(self):
        metadata = [dict(info) for info in METADATA]
        for index in range(policy.MAX_PATHS):
            path = f'/nix/store/{index:032d}-dependency'
            metadata[-1]['references'] = [path]
            metadata.append({'path': path, 'narSize': 1, 'references': []})
        root, result = self.run_cache(upload=True, metadata=metadata)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((root / 'push.json').exists())

    def test_action_has_no_ambient_push_and_workflows_keep_trust_boundary(self):
        ci_root = Path(os.environ.get('MISH_CI_ROOT', Path(__file__).parents[1]))
        import yaml
        action = yaml.safe_load((ci_root / '.github/actions/nix/action.yml').read_text())
        cachix = [step for step in action['runs']['steps']
                  if step.get('uses', '').startswith('cachix/cachix-action@')][0]
        self.assertIs(cachix['with']['skipPush'], True)
        self.assertEqual(cachix['with']['authToken'], '${{ inputs.token }}')
        for path in (ci_root / '.github/workflows').glob('*.yml'):
            workflow = yaml.safe_load(path.read_text())
            for job in workflow['jobs'].values():
                for step in job.get('steps', []):
                    token = step.get('with', {}).get('token', '')
                    if token and 'CACHIX_TOKEN' in token:
                        self.assertTrue('refs/heads/main' in token or 'refs/tags/v' in token, path)
                        self.assertNotIn('pull_request', token)

    def test_failed_validation_cannot_upload_or_claim_evidence(self):
        root, result = self.run_cache(upload=True, fail=True)
        self.assertEqual(result.returncode, 17)
        self.assertFalse((root / 'push.json').exists())
        self.assertFalse((root / 'artifacts/validation-manifest.json').exists())


if __name__ == '__main__':
    unittest.main()
