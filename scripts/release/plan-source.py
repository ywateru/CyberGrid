#!/usr/bin/env python3
"""Inventory the explicit public allowlist. Does not move, git-add or publish files."""
import argparse
import hashlib
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
POLICY = json.loads((Path(__file__).with_name('source-policy.json')).read_text())


def eligible(relative):
    name = relative.as_posix()
    parts = relative.parts
    if name in POLICY['rootFiles'] + POLICY['exactFiles'] + POLICY['publicDocs']:
        return True
    if not any(name.startswith(p + '/') for p in POLICY['sourceRoots']):
        return False
    if any(part.startswith('.env') for part in parts):
        return False
    if name in POLICY['excludedExactFiles']:
        return False
    if relative.name in POLICY['excludedNames'] or relative.suffix.lower() in {'.log', '.apk', '.aab', '.keystore', '.jks', '.pem', '.key', '.p12', '.pfx', '.pyc'}:
        return False
    if any(name.startswith(p) for p in POLICY['excludedPrefixes']):
        return False
    exceptions = ('tools/cloudflared/bin/', 'sdk/provider-assets/', 'sdk-rust/provider-assets/')
    if not name.startswith(exceptions) and any(p in POLICY['excludedParts'] for p in parts):
        return False
    if relative.name.endswith(('.historical', '.retired', '.backup', '.bak')):
        return False
    return True


def binary_allowed(relative, data):
    """Binary exceptions apply to exact files or the existing reviewed roots."""
    try:
        data.decode('utf-8')
        binary = b'\0' in data
    except UnicodeDecodeError:
        binary = True
    name = relative.as_posix()
    return not binary or name in POLICY.get('allowedBinaryFiles', []) or any(
        name.startswith(prefix + '/') for prefix in POLICY['allowedBinaryRoots'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    keep, excluded, violations = [], [], []
    for directory, dirs, files in os.walk(ROOT):
        dirs.sort()
        files.sort()
        for name in files:
            path = Path(directory) / name
            relative = path.relative_to(ROOT)
            # Policy script is also usable in a repository: its VCS internals are not input.
            if '.git' in relative.parts:
                continue
            item = {'path': relative.as_posix(), 'bytes': path.lstat().st_size}
            if path.is_symlink():
                item['reason'] = 'symlink requires explicit review'
                excluded.append(item)
            elif eligible(relative) and not binary_allowed(relative, path.read_bytes()):
                item['reason'] = 'binary is not an explicit reviewed exception'
                violations.append(item)
            elif eligible(relative):
                item['sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
                keep.append(item)
            else:
                excluded.append(item)
    report = {'status': 'PLAN_BLOCKED_BINARY_POLICY' if violations else 'PLAN_ONLY_NOT_PRIVACY_APPROVAL', 'binaryPolicyViolations': violations, 'keptCount': len(keep), 'keptBytes': sum(x['bytes'] for x in keep), 'archiveCount': len(excluded), 'archiveBytes': sum(x['bytes'] for x in excluded), 'keep': keep, 'archive': excluded}
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + '\n')
    args.out.chmod(0o600)
    print(json.dumps({k: report[k] for k in ['status', 'keptCount', 'keptBytes', 'archiveCount', 'archiveBytes', 'binaryPolicyViolations']}))
    if violations:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
