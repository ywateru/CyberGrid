#!/usr/bin/env python3
"""Offline: stage the one fixed APK native executable; never run or download it."""
from pathlib import Path
import hashlib,json,os,sys,uuid
root=Path(__file__).resolve().parents[1]
if len(sys.argv)!=2:raise SystemExit('usage: python3 scripts/prepare-android-provider.py HOST_JNILIBS_DIRECTORY')
assets=root/'provider-assets';manifest=json.loads((assets/'manifest.json').read_text());entry=manifest['platforms']['android-arm64']
source=assets/'android-arm64/jniLibs/arm64-v8a/libcloudflared.so'
if source.is_symlink() or not source.is_file():raise SystemExit('Android bundled provider is missing or not regular')
raw=source.read_bytes()
if len(raw)!=entry['bytes'] or hashlib.sha256(raw).hexdigest()!=entry['sha256']:raise SystemExit('Android bundled provider checksum mismatch')
output=Path(sys.argv[1]).absolute()
for path in [output,output/'arm64-v8a',output/'arm64-v8a/libcloudflared.so']:
 if path.is_symlink():raise SystemExit('Refusing symlink in provider output')
output=output/'arm64-v8a';output.mkdir(parents=True,exist_ok=True)
target=output/'libcloudflared.so';temporary=output/('.libcloudflared-'+uuid.uuid4().hex)
try:
 with temporary.open('xb') as f:f.write(raw);f.flush();os.fsync(f.fileno())
 temporary.chmod(0o755);os.replace(temporary,target)
finally:temporary.unlink(missing_ok=True)
print('Prepared one APK provider:',str(target),'SHA256',entry['sha256'])
