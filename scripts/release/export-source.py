#!/usr/bin/env python3
"""Create a reviewable public snapshot from a frozen allowlist; never mutate the workspace."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--private-values', type=Path, help='Optional private JSON array of local values; report only paths/counts')
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text())
    out = args.out.resolve()
    if out.exists():
        raise RuntimeError('Snapshot destination must not exist')
    out.mkdir(parents=True, mode=0o700)
    for item in plan['keep']:
        relative = Path(item['path'])
        if relative.is_absolute() or '..' in relative.parts:
            raise RuntimeError('Unsafe allowlist path')
        source = ROOT / relative
        if source.is_symlink() or sha(source) != item['sha256']:
            raise RuntimeError('Source changed since the plan; regenerate after freeze')
        target = out / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, target)
        target.chmod(0o755 if source.stat().st_mode & 0o111 else 0o644)
    changes = []

    def update(path, data, reason):
        before = sha(path)
        path.write_bytes(data)
        after = sha(path)
        if before != after:
            changes.append({'path': path.relative_to(out).as_posix(), 'originalSha256': before, 'publicSha256': after, 'reason': reason})

    # `go version -m` prefixes the first line with the build machine's filename.
    # Normalize only that display path; never rewrite either provider binary.
    canonical = out / 'tools/cloudflared/build-modules.txt'
    original = canonical.read_text()
    lines = original.splitlines(keepends=True)
    if not lines or ': go1.' not in lines[0]:
        raise RuntimeError('Unexpected provider build-info format')
    lines[0] = 'cloudflared-android-arm64:' + lines[0].split(':', 1)[1]
    public_info = ''.join(lines).encode()
    for relative in ['tools/cloudflared/build-modules.txt', 'sdk/provider-assets/build-modules-android-arm64.txt', 'sdk-rust/provider-assets/build-modules-android-arm64.txt', 'apps/tauri-test/notices/cloudflared/build-modules-android-arm64.txt']:
        update(out / relative, public_info, 'Normalize build-host filename only; module pins and binary bytes unchanged')
    info_sha = hashlib.sha256(public_info).hexdigest()
    canonical_manifest = out / 'tools/cloudflared/bundled-manifest.json'
    manifest = json.loads(canonical_manifest.read_text())
    manifest['platforms']['android-arm64']['buildInformation'] = {'sha256': info_sha, 'normalization': 'Build-host filename removed from first line; module pins/settings unchanged; original binary bytes preserved'}
    manifest_bytes = (json.dumps(manifest, indent=2) + '\n').encode()
    for relative in ['tools/cloudflared/bundled-manifest.json', 'sdk/provider-assets/manifest.json', 'sdk-rust/provider-assets/manifest.json', 'apps/tauri-test/notices/cloudflared/manifest.json']:
        update(out / relative, manifest_bytes, 'Bind normalized public build metadata; provider resource pins unchanged')
    android_manifest = json.loads((out / 'tools/cloudflared/manifest.json').read_text())
    android_manifest['publicBuildInformationSha256'] = info_sha
    android_manifest['publicBuildInformationNormalization'] = 'Build-host filename removed; raw provider unchanged'
    android_bytes = (json.dumps(android_manifest, indent=2) + '\n').encode()
    for relative in ['tools/cloudflared/manifest.json', 'sdk/provider-assets/android-build-manifest.json', 'sdk-rust/provider-assets/android-build-manifest.json', 'apps/tauri-test/notices/cloudflared/android-build-manifest.json']:
        update(out / relative, android_bytes, 'Bind normalized public metadata without changing original executable')
    notice_root = out / 'apps/tauri-test/notices/cloudflared'
    notice_map = json.loads((notice_root / 'source-files-sha256.json').read_text())
    for relative in notice_map:
        notice_map[relative] = sha(notice_root / relative)
    update(notice_root / 'source-files-sha256.json', (json.dumps(notice_map, indent=2) + '\n').encode(), 'Rebind copied notice metadata to this public source snapshot')
    # No original-license content is changed. The snapshot's CI creates fresh
    # source manifests from git; old local audit manifests are excluded by policy.
    personal_values = [x.encode() for x in json.loads(args.private_values.read_text())] if args.private_values else []
    violations = []
    for path in sorted(p for p in out.rglob('*') if p.is_file()):
        data = path.read_bytes()  # Binary resources are scanned too.
        hits = sum(data.count(value) for value in personal_values if value)
        if hits:
            violations.append({'path': path.relative_to(out).as_posix(), 'knownPersonalValueMatchCount': hits})
    raw_pins = {}
    for platform, name in [('darwin-arm64', 'cloudflared-darwin-arm64'), ('android-arm64', 'cloudflared-android-arm64')]:
        path = out / 'tools/cloudflared/bin' / name
        expected = manifest['platforms'][platform]['sha256']
        if sha(path) != expected:
            raise RuntimeError('Provider binary unexpectedly changed')
        raw_pins[platform] = expected
    files = []
    tree = hashlib.sha256()
    for path in sorted(p for p in out.rglob('*') if p.is_file()):
        relative = path.relative_to(out).as_posix()
        data = path.read_bytes()
        tree.update(relative.encode() + b'\0' + data + b'\0')
        files.append({'path': relative, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    report = {'status': 'PUBLIC_SNAPSHOT_READY_FOR_PRIVACY_REVIEW' if not violations else 'PUBLIC_SNAPSHOT_PRIVACY_BLOCKED',
              'files': files, 'fileCount': len(files), 'totalBytes': sum(p['bytes'] for p in files), 'treeSha256': tree.hexdigest(),
              'transformations': changes, 'knownPersonalValueFindings': violations, 'providerRawPinsUnchanged': raw_pins,
              'originalSourceNotMutated': True, 'gitInitialized': False, 'pushed': False}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    write_json(args.report, report)
    args.report.chmod(0o600)
    print(json.dumps({k: report[k] for k in ['status', 'fileCount', 'totalBytes', 'treeSha256', 'knownPersonalValueFindings']}))
    if violations:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
