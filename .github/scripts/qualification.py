"""Fail-closed software qualification; physical acceptance stays external."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

import yaml


def validate_graph(graph):
    visited, active = set(), set()

    def visit(node):
        if node in active:
            raise ValueError(f'dependency cycle at {node}')
        if node not in graph:
            raise ValueError(f'unknown dependency: {node}')
        if node in visited:
            return
        active.add(node)
        for dependency in graph[node]:
            visit(dependency)
        active.remove(node)
        visited.add(node)

    for node in graph:
        visit(node)


def validate_policy(root):
    policy = json.loads((root / '.github/qualification.json').read_text())
    if policy['version'] != 1:
        raise ValueError('unsupported qualification policy')
    validate_graph(policy['milestones'])
    rules = json.loads((root / '.github/merge-queue-rules.json').read_text())
    required = next(rule['parameters']['required_status_checks']
                    for rule in rules['rules'] if rule['type'] == 'required_status_checks')
    if {check['context'] for check in required} != set(policy['required_checks']):
        raise ValueError('required merge checks differ from qualification policy')
    for filename, expected in policy['workflows'].items():
        validate_graph(expected)
        workflow = yaml.safe_load((root / '.github/workflows' / filename).read_text())
        actual = {}
        for name, job in workflow['jobs'].items():
            needs = job.get('needs', [])
            actual[name] = [needs] if isinstance(needs, str) else needs
        validate_graph(actual)
        if actual != expected:
            raise ValueError(f'{filename}: job dependencies differ from policy')
        # A skipped prerequisite must yield a failed native context, not a
        # skipped context accepted by branch protection.
        guarded = 'checks' if filename == 'ci.yml' else 'images'
        if filename != 'maintenance.yml':
            job = workflow['jobs'][guarded]
            if job.get('if') != 'always()':
                raise ValueError(f'{filename}: {guarded} must run after failed prerequisites')
            first = job['steps'][0]
            if first.get('env', {}).get('QUALIFIER_RESULT') != '${{ needs.qualify.result }}':
                raise ValueError(f'{filename}: missing prerequisite result guard')
            if first.get('run') != 'test "$QUALIFIER_RESULT" = success':
                raise ValueError(f'{filename}: prerequisite guard must fail closed')
            if any(step.get('continue-on-error', False) for step in job['steps']):
                raise ValueError(f'{filename}: qualification steps cannot tolerate errors')
            if filename in ('images.yml', 'release.yml'):
                commands = [step.get('run', '') for step in job['steps']]
                native = next((i for i, command in enumerate(commands)
                               if command.startswith('nix flake check ')
                               and '--no-build' not in command), None)
                image = next((i for i, command in enumerate(commands)
                              if 'bash .github/scripts/build-images.sh' in command), None)
                integrity = next((i for i, command in enumerate(commands)
                                  if 'sha256sum --check SHA256SUMS' in command), None)
                if None in (native, image, integrity) or not native < image < integrity:
                    raise ValueError(f'{filename}: native/image/integrity gates missing or reordered')
        if filename in ('ci.yml', 'images.yml'):
            final = workflow['jobs']['qualified']
            if final.get('name') != ('Software qualification' if filename == 'ci.yml' else 'Image qualification'):
                raise ValueError(f'{filename}: aggregate context identity differs from policy')
            if final.get('if') != 'always()':
                raise ValueError(f'{filename}: aggregate gate must always report')
            last = final['steps'][-1]
            expected_command = ('nix develop --accept-flake-config --command python '
                                '.github/scripts/qualification.py --results "$GATE_RESULTS" '
                                f'--expected qualify {guarded}')
            if (last.get('env', {}).get('GATE_RESULTS') != '${{ toJSON(needs) }}'
                    or last.get('run') != expected_command):
                raise ValueError(f'{filename}: aggregate gate must require successful results')
    ci = yaml.safe_load((root / '.github/workflows/ci.yml').read_text())
    checks = ci['jobs']['checks']
    contexts = {f'checks ({entry["runner"]}, {entry["system"]})'
                for entry in checks['strategy']['matrix']['include']}
    if contexts | {'Software qualification'} != set(policy['required_checks']):
        raise ValueError('native matrix no longer matches required check names')
    if 'name' in checks or 'continue-on-error' in checks:
        raise ValueError('native checks cannot rename or tolerate errors')
    return policy


def components(policy, paths):
    if paths is None:
        return sorted(set(policy['components']) | {'unknown'})
    result = set()
    for path in paths:
        matches = {name for name, prefixes in policy['components'].items()
                   if any(path.startswith(prefix) if prefix.endswith('/') else path == prefix
                          for prefix in prefixes)}
        result.update(matches or {'unknown'})
    return sorted(result)


def require_success(results, expected):
    if set(results) != set(expected):
        raise ValueError('missing or unexpected prerequisite results')
    failures = {name: result.get('result') for name, result in results.items()
                if result.get('result') != 'success'}
    if failures:
        raise ValueError(f'unqualified prerequisites: {failures}')


def changed_paths(event_name, event):
    # Diff qualifiers are descriptive. Missing bases and unknown paths retain
    # full validation. No changed path or API response ever skips a check.
    base = (event.get('pull_request', {}).get('base', {}).get('sha')
            if event_name == 'pull_request' else
            event.get('merge_group', {}).get('base_sha')
            if event_name == 'merge_group' else
            event.get('before') if event_name == 'push' else None)
    if not base or not re.fullmatch(r'[0-9a-fA-F]{40}', base) or set(base) == {'0'}:
        return None
    try:
        output = subprocess.check_output(
            ['git', 'diff', '--name-only', '-z', base, 'HEAD', '--'], stderr=subprocess.DEVNULL)
        return [path.decode('utf-8', errors='replace') for path in output.split(b'\0') if path]
    except subprocess.CalledProcessError:
        return None


def verify_artifacts(directory, revision, lock_bytes):
    systems = {'x86_64-linux', 'aarch64-linux'}
    if {path.name for path in directory.iterdir()} != {f'images-{system}' for system in systems}:
        raise ValueError('both native image artifacts are required')
    for system in systems:
        artifact = directory / f'images-{system}'
        if artifact.is_symlink() or not artifact.is_dir():
            raise ValueError('image artifact must be a directory')
        files = list(artifact.iterdir())
        if any(path.is_symlink() or not path.is_file() for path in files):
            raise ValueError('unexpected image artifact entry')
        checksums = {}
        for line in (artifact / 'SHA256SUMS').read_text().splitlines():
            digest, filename = line.split('  ', 1)
            name = filename.removeprefix('./')
            if '/' in name or name in ('', '.', '..') or name in checksums:
                raise ValueError('invalid checksum filename')
            checksums[name] = digest
        if set(checksums) != {path.name for path in files if path.name != 'SHA256SUMS'}:
            raise ValueError('incomplete artifact checksums')
        for name, expected in checksums.items():
            with (artifact / name).open('rb') as stream:
                if hashlib.file_digest(stream, 'sha256').hexdigest() != expected:
                    raise ValueError(f'artifact checksum mismatch: {name}')
        if (artifact / 'source-revision.txt').read_text().strip() != revision:
            raise ValueError('image revision differs from tagged revision')
        if (artifact / 'validation-source-revision.txt').read_text().strip() != revision:
            raise ValueError('validation revision differs from tagged revision')
        if (artifact / 'flake.lock').read_bytes() != lock_bytes:
            raise ValueError('image lockfile differs from tagged lockfile')
        manifest = json.loads((artifact / 'validation-manifest.json').read_text())
        if manifest['system'] != system or manifest['lockHash'] != hashlib.sha256(lock_bytes).hexdigest():
            raise ValueError('validation identity differs from image target')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=Path('.'))
    parser.add_argument('--verify-only', action='store_true')
    parser.add_argument('--results', help='JSON needs object; all named jobs must succeed')
    parser.add_argument('--expected', nargs='+')
    parser.add_argument('--artifact-root', type=Path)
    args = parser.parse_args()
    policy = validate_policy(args.root)
    if args.verify_only:
        return
    if args.artifact_root:
        revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
        verify_artifacts(args.artifact_root, revision, (args.root / 'flake.lock').read_bytes())
        print('Both downloaded image artifacts match checksums, architecture, revision and lockfile.')
        return
    if args.results is not None:
        if not args.expected:
            parser.error('--results requires --expected')
        require_success(json.loads(args.results), args.expected)
        print('All declared software prerequisites succeeded; physical acceptance remains open.')
        return
    event_name = os.environ.get('GITHUB_EVENT_NAME', 'local')
    event_path = os.environ.get('GITHUB_EVENT_PATH')
    event = json.loads(Path(event_path).read_text()) if event_path else {}
    paths = changed_paths(event_name, event)
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip()
    report = {'version': 1, 'revision': revision, 'event': event_name,
              'diff_available': paths is not None, 'components': components(policy, paths),
              'lock_hashes': {str(path): hashlib.sha256((args.root / path).read_bytes()).hexdigest()
                              for path in [Path('flake.lock'), Path('apps/mish/Cargo.lock')]},
              'validation': 'full-native-both-architectures',
              'physical_acceptance': 'unverified'}
    directory = Path('artifacts')
    directory.mkdir(exist_ok=True)
    (directory / 'qualification.json').write_text(json.dumps(report, indent=2) + '\n')
    graph = ['flowchart TD']
    for filename, jobs in policy['workflows'].items():
        prefix = filename.removesuffix('.yml')
        for name, needs in jobs.items():
            node = f'{prefix}_{name.replace("-", "_")}'
            graph.append(f'  {node}["{filename}: {name}"]')
            graph.extend(f'  {prefix}_{dependency.replace("-", "_")} --> {node}'
                         for dependency in needs)
    graph.extend(f'  {dependency} --> {milestone}'
                 for milestone, needs in policy['milestones'].items() for dependency in needs)
    (directory / 'qualification-graph.mmd').write_text('\n'.join(graph) + '\n')
    summary = (f'Qualification for `{revision}`: full native validation on both architectures.\n\n'
               f'Changed components: {", ".join(report["components"]) or "none"}. '
               f'Diff available: {report["diff_available"]}.\n\n'
               'Physical acceptance remains unverified. Image builds are a separate software gate.\n')
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as stream:
            stream.write(summary + '\n```mermaid\n' + '\n'.join(graph) + '\n```\n')
    print(summary)


if __name__ == '__main__':
    main()
