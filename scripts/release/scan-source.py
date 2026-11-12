#!/usr/bin/env python3
"""Fail closed on private file classes and credential literals; report paths/counts only."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--private-values', type=Path, help='Private JSON array used only in memory')
    parser.add_argument('--report', type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve()
    patterns = {
        'githubToken': rb'gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}',
        'logtoPersonalAccessToken': rb'pat_[A-Za-z0-9]{25,}',
        'secretPlaceholder': rb'\{\{SECRET_[A-Za-z0-9_]+\}\}',
        'privateKey': rb'(?m)^-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\r?\n[A-Za-z0-9+/=\r\n]{40,}-----END (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
    }
    private = [x.encode() for x in json.loads(args.private_values.read_text())] if args.private_values else []
    fixture = json.loads(Path(__file__).with_name('provider-public-fixture.json').read_text())
    public_fixtures = []
    findings = []
    files = []
    for path in sorted(root.rglob('*')):
        relative = path.relative_to(root)
        if '.git' in relative.parts or not path.is_file():
            continue
        files.append(path)
        classes = {}
        if path.is_symlink():
            classes['symlink'] = 1
        if any(p in {'.vercel', 'node_modules', 'credentials', 'private-device', '.private', '.ssh', '.gnupg'} for p in relative.parts):
            classes['privateOrGeneratedDirectory'] = 1
        if path.name == '.DS_Store':
            classes['privateFileClass'] = 1
        if (path.name.startswith('.env') and path.name != '.env.example') or path.suffix in {'.keystore', '.jks', '.key', '.p12', '.pfx', '.pem', '.log'}:
            classes['privateFileClass'] = 1
        data = path.read_bytes()  # Includes binaries; never patch executable bytes.
        for name, pattern in patterns.items():
            matches = re.findall(pattern, data)
            if name == 'privateKey' and matches:
                expected_file_sha = fixture['allowedFiles'].get(relative.as_posix())
                if expected_file_sha and hashlib.sha256(data).hexdigest() == expected_file_sha:
                    allowed = [b for b in matches if len(b) == fixture['publicFixtureBlockBytes'] and hashlib.sha256(b).hexdigest() == fixture['publicFixtureBlockSHA256']]
                    if allowed:
                        public_fixtures.append({'path': relative.as_posix(), 'binarySHA256': expected_file_sha, 'blockSHA256': fixture['publicFixtureBlockSHA256'], 'count': len(allowed), 'sourceURL': fixture['sourceURL']})
                        matches = [b for b in matches if b not in allowed]
            if matches:
                classes[name] = len(matches)
        count = sum(data.count(value) for value in private if value)
        if count:
            classes['knownPrivateValue'] = count
        if classes:
            findings.append({'path': relative.as_posix(), 'matches': classes})
    report = {'status': 'PRIVACY_LITERAL_SCAN_PASS' if not findings else 'PRIVACY_LITERAL_SCAN_BLOCKED',
              'fileCount': len(files), 'binaryFilesAlsoScanned': True, 'findings': findings, 'exactPublicUpstreamFixtures': public_fixtures,
              'scope': 'Literal/file-class scanner only; independent source and history review remains required'}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + '\n')
    args.report.chmod(0o600)
    print(json.dumps(report))
    if findings:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
