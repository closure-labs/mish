"""Qualification cannot turn skipped checks or unknown changes into success."""
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(os.environ.get('MISH_CI_ROOT', Path(__file__).parents[1]))
spec = importlib.util.spec_from_file_location('qualification', ROOT / '.github/scripts/qualification.py')
qualification = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qualification)


def writable_policy_copy(root):
    destination = root / '.github'
    shutil.copytree(ROOT / '.github', destination)
    # Nix store inputs are read-only. Mutate only this owned temporary copy.
    for path in [destination, *destination.rglob('*')]:
        path.chmod(0o700 if path.is_dir() else 0o600)


class QualificationTests(unittest.TestCase):
    def test_repository_policy_matches_actual_workflow_dependencies(self):
        qualification.validate_policy(ROOT)

    def test_cycles_and_missing_dependencies_are_rejected(self):
        for graph in [{'a': ['b'], 'b': ['a']}, {'a': ['missing']}]:
            with self.subTest(graph=graph), self.assertRaises(ValueError):
                qualification.validate_graph(graph)

    def test_every_non_success_result_and_missing_result_fails(self):
        for state in ['failure', 'cancelled', 'skipped', 'neutral', None]:
            with self.subTest(state=state), self.assertRaises(ValueError):
                qualification.require_success({'checks': {'result': state}}, ['checks'])
        for results in [{}, {'checks': {'result': 'success'}, 'extra': {'result': 'success'}}]:
            with self.subTest(results=results), self.assertRaises(ValueError):
                qualification.require_success(results, ['checks'])
        qualification.require_success({'checks': {'result': 'success'}}, ['checks'])

    def test_unknown_diff_and_paths_retain_full_classification(self):
        policy = qualification.validate_policy(ROOT)
        self.assertIn('unknown', qualification.components(policy, None))
        self.assertEqual(qualification.components(policy, ['unmapped-file']), ['unknown'])
        self.assertEqual(qualification.components(policy, ['docs/guide.md']), ['documentation'])
        self.assertIsNone(qualification.changed_paths('workflow_dispatch', {}))
        self.assertIsNone(qualification.changed_paths('push', {'before': '$(unsafe)'}))

    def test_workflow_dependency_drift_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            writable_policy_copy(root)
            path = root / '.github/workflows/ci.yml'
            path.write_text(path.read_text().replace('needs: qualify', 'needs: []', 1))
            with self.assertRaisesRegex(ValueError, 'dependencies differ'):
                qualification.validate_policy(root)

    def test_required_context_guard_cannot_be_skipped_or_removed(self):
        for replacement in ['if: false', 'if: success()']:
            with self.subTest(replacement=replacement), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                writable_policy_copy(root)
                path = root / '.github/workflows/ci.yml'
                path.write_text(path.read_text().replace('if: always()', replacement, 1))
                with self.assertRaises(ValueError):
                    qualification.validate_policy(root)

    def test_release_gate_rejects_corruption_wrong_identity_and_missing_architecture(self):
        def write_checksums(artifact):
            lines = [f'{hashlib.sha256(path.read_bytes()).hexdigest()}  ./{path.name}'
                     for path in artifact.iterdir() if path.name != 'SHA256SUMS']
            (artifact / 'SHA256SUMS').write_text('\n'.join(lines) + '\n')

        for fault in ['none', 'corrupt', 'revision', 'validation-revision', 'architecture', 'missing', 'incomplete']:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for system in ['x86_64-linux', 'aarch64-linux']:
                    artifact = root / f'images-{system}'
                    artifact.mkdir()
                    (artifact / 'source-revision.txt').write_text('revision\n')
                    (artifact / 'validation-source-revision.txt').write_text('revision\n')
                    (artifact / 'flake.lock').write_bytes(b'locked')
                    (artifact / 'image.bin').write_bytes(b'image')
                    (artifact / 'validation-manifest.json').write_text(json.dumps({
                        'system': system, 'lockHash': hashlib.sha256(b'locked').hexdigest()}))
                    write_checksums(artifact)
                artifact = root / 'images-x86_64-linux'
                if fault == 'corrupt':
                    (artifact / 'image.bin').write_bytes(b'corrupt')
                elif fault == 'revision':
                    (artifact / 'source-revision.txt').write_text('wrong')
                    write_checksums(artifact)
                elif fault == 'validation-revision':
                    (artifact / 'validation-source-revision.txt').write_text('wrong')
                    write_checksums(artifact)
                elif fault == 'architecture':
                    manifest = json.loads((artifact / 'validation-manifest.json').read_text())
                    manifest['system'] = 'wrong'
                    (artifact / 'validation-manifest.json').write_text(json.dumps(manifest))
                    write_checksums(artifact)
                elif fault == 'missing':
                    shutil.rmtree(artifact)
                elif fault == 'incomplete':
                    (artifact / 'unchecked.bin').write_bytes(b'extra')
                if fault == 'none':
                    qualification.verify_artifacts(root, 'revision', b'locked')
                else:
                    with self.assertRaises(ValueError):
                        qualification.verify_artifacts(root, 'revision', b'locked')
