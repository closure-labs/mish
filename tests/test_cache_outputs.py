"""Exercise cache upload boundaries without a network or real credentials."""
import hashlib
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


class CacheOutputsTests(unittest.TestCase):
    def run_cache(self, upload=False, fail=False):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        binary = root / 'bin'
        binary.mkdir()
        proof = root / 'proof'
        proof.mkdir()
        (proof / 'manifest.json').write_text('{"system":"x86_64-linux"}\n')
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
                   ' print("/nix/store/test-mish\\n/nix/store/test-proof\\n/nix/store/test-shell")\n'
                   'elif sys.argv[1] == "path-info": print(os.environ["TEST_PROOF"])\n'
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
                           MISH_CACHE_UPLOAD=str(upload).lower())
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

    def test_explicit_upload_includes_native_roots_and_images(self):
        root, result = self.run_cache(upload=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads((root / 'push.json').read_text()),
                         ['push', 'mish', '/nix/store/test-mish', '/nix/store/test-proof',
                          '/nix/store/test-shell', '/nix/store/test-image'])

    def test_failed_validation_cannot_upload_or_claim_evidence(self):
        root, result = self.run_cache(upload=True, fail=True)
        self.assertEqual(result.returncode, 17)
        self.assertFalse((root / 'push.json').exists())
        self.assertFalse((root / 'artifacts/validation-manifest.json').exists())


if __name__ == '__main__':
    unittest.main()
