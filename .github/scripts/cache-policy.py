"""Bound complete local closures before the only explicit Cachix push."""
import json
import os
from pathlib import Path
import re
import subprocess


# Positive selection: never upload the aggregate's links to VM/check images,
# developer tools, or a newly added check without deliberate policy review.
SELECTED = frozenset({'mish-control', 'schema', 'cli', 'channel-selection'})
MAX_PATHS = 256
MAX_NAR_BYTES = 1024 ** 3
STORE_PATH = re.compile(r'/nix/store/[0-9abcdfghijklmnpqrsvwxyz]{32}-[A-Za-z0-9+._?=-]+')


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate JSON metadata key: {key}')
        result[key] = value
    return result


def load_json(text):
    return json.loads(text, object_pairs_hook=unique_object)


def store_path(value):
    if (not isinstance(value, str) or not STORE_PATH.fullmatch(value)
            or value.endswith('.drv') or os.path.realpath(value) != value):
        raise ValueError(f'non-canonical output store path: {value!r}')
    return value


def select_roots(manifest):
    if (type(manifest.get('version')) is not int or manifest.get('version') != 1
            or manifest.get('system') not in {
            'x86_64-linux', 'aarch64-linux'}):
        raise ValueError('unknown validation manifest version/system')
    outputs = manifest['outputs']
    names = [entry['name'] for entry in outputs]
    if len(names) != len(set(names)) or not SELECTED.issubset(names):
        raise ValueError('missing or duplicate validation outputs')
    return sorted({store_path(entry['path']) for entry in outputs
                   if entry['name'] in SELECTED})


def closure_summary(metadata, roots):
    # Nix 2.x has emitted both a list and an object keyed by store path.
    if isinstance(metadata, dict):
        entries = []
        for path, info in metadata.items():
            if not isinstance(info, dict) or info.get('path', path) != path:
                raise ValueError('ambiguous path metadata')
            entries.append(dict(info, path=path))
    elif isinstance(metadata, list):
        entries = metadata
    else:
        raise ValueError('missing closure metadata')
    nodes = {}
    for info in entries:
        path = store_path(info['path'])
        size = info['narSize']
        refs = info['references']
        if (path in nodes or type(size) is not int or size <= 0
                or not isinstance(refs, list) or len(refs) != len(set(refs))):
            raise ValueError('duplicate or invalid closure metadata')
        nodes[path] = (size, {store_path(ref) for ref in refs})
    seen, pending = set(), set(roots)
    while pending:
        path = pending.pop()
        if path in seen:
            continue
        if path not in nodes:
            raise ValueError('incomplete recursive closure')
        seen.add(path)
        pending.update(nodes[path][1] - seen)
    if seen != nodes.keys():
        raise ValueError('unrelated paths in recursive closure')
    return {'paths': len(seen), 'narBytes': sum(nodes[path][0] for path in seen)}


def main():
    artifact = Path('artifacts/cache-policy.json')
    roots = select_roots(load_json(Path('artifacts/validation-manifest.json').read_text()))
    # Query only the local store: no upstream availability probes or unsafe
    # subtraction of dependencies from a closure. Auto selects the local daemon
    # on multi-user CI; forcing direct SQLite access would require DB ownership.
    # Metadata failures stop upload.
    result = subprocess.run(['nix', 'path-info', '--offline', '--store', 'auto',
                             '--json', '--recursive', '--', *roots],
                            check=True, capture_output=True, text=True)
    summary = closure_summary(load_json(result.stdout), roots)
    report = dict(summary, roots=roots, maxPaths=MAX_PATHS,
                  maxNarBytes=MAX_NAR_BYTES, status='read-only')
    bounded = summary['paths'] <= MAX_PATHS and summary['narBytes'] <= MAX_NAR_BYTES
    upload = os.environ.get('MISH_CACHE_UPLOAD') == 'true'
    authenticated = os.environ.get('MISH_CACHE_AUTHENTICATED') == 'true'
    if not bounded:
        report['status'] = 'over-budget'
    elif upload and authenticated:
        report['status'] = 'upload-requested'
    artifact.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, sort_keys=True), flush=True)
    if not bounded:
        # Optional caching must not prevent complete validation/image evidence.
        # No paths are pushed, including the roots which would recurse in Cachix.
        print('Cache upload skipped: complete closure exceeds policy budget.', flush=True)
        return
    if upload and authenticated:
        subprocess.run(['cachix', 'push', 'mish', *roots], check=True)
        report['status'] = 'push-completed'
        artifact.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
