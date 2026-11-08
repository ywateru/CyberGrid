#!/usr/bin/env python3
"""Record build provenance and preserve root third-party notices for linked modules."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

root = Path(__file__).resolve().parent
source, modules_json, cc, go = sys.argv[1:]
source = Path(source)
binary = root / 'bin/cloudflared-android-arm64'
raw = Path(modules_json).read_text()
decoder = json.JSONDecoder()
modules = []
while raw.strip():
    module, count = decoder.raw_decode(raw.lstrip())
    modules.append(module)
    raw = raw.lstrip()[count:]
linked = set(re.findall(r'^\s*dep\s+(\S+)', (root / 'build-modules.txt').read_text(), flags=re.MULTILINE))
notices = ['# Third-party notices\n\nThese unmodified root notices accompany modules linked into the Android provider.\nThe authoritative dependency versions and checksums are in build-modules.txt and upstream go.sum.\n']
missing = []
for module in modules:
    if module['Path'] not in linked:
        continue
    actual = module.get('Replace', module)
    directory = actual.get('Dir')
    found = []
    if directory:
        for path in sorted(Path(directory).iterdir()):
            if path.is_file() and re.match(r'(?i)^(licen[cs]e|copying|notice|copyright)([._-].*)?$', path.name):
                found.append(path)
    if not found:
        missing.append(module['Path'])
        continue
    notices.append('\n## '+module['Path']+' '+module.get('Version', '')+'\n')
    for path in found:
        notices.append('\n### '+path.name+'\n\n'+path.read_text(errors='replace')+'\n')
(root / 'THIRD_PARTY_NOTICES.md').write_text('\n'.join(notices))
manifest = {
    'repository': 'https://github.com/cloudflare/cloudflared',
    'version': '2026.9.0',
    'commit': subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip(),
    'sourceModified': bool(subprocess.check_output(['git', '-C', str(source), 'status', '--porcelain'], text=True).strip()),
    'patches': [{'path':'patches/0001-android-explicit-dns.patch','sha256':hashlib.sha256((root/'patches/0001-android-explicit-dns.patch').read_bytes()).hexdigest()}],
    'license': 'Apache-2.0',
    'target': 'android/arm64',
    'androidAPI': int(os.environ.get('ANDROID_API', '28')),
    'cgoEnabled': True,
    'go': subprocess.check_output([go, 'version'], text=True).strip(),
    'clang': subprocess.check_output([cc, '--version'], text=True).splitlines()[0],
    'binary': 'bin/cloudflared-android-arm64',
    'sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'bytes': binary.stat().st_size,
    'sourceGoSumSHA256': hashlib.sha256((source / 'go.sum').read_bytes()).hexdigest(),
    'linkedModuleCount': len(linked),
    'modulesWithoutRootNotice': missing,
}
(root / 'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
