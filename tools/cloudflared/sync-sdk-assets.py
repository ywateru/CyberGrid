#!/usr/bin/env python3
"""Copy only fixed canonical provider inputs into the two SDK source distributions."""
from pathlib import Path
import json,hashlib,shutil
root=Path(__file__).resolve().parent;repo=root.parents[1]
manifest=json.loads((root/'bundled-manifest.json').read_text())
for target,name in [('darwin-arm64','cloudflared-darwin-arm64'),('android-arm64','cloudflared-android-arm64')]:
 source=root/'bin'/name;entry=manifest['platforms'][target]
 assert source.is_file() and not source.is_symlink()
 raw=source.read_bytes();assert len(raw)==entry['bytes'] and hashlib.sha256(raw).hexdigest()==entry['sha256']
 for sdk in ['sdk','sdk-rust']:
  out=repo/sdk/'provider-assets'/entry['binary'];out.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,out);out.chmod(0o755)
for sdk in ['sdk','sdk-rust']:
 out=repo/sdk/'provider-assets'
 for source,name in [('bundled-manifest.json','manifest.json'),('manifest.json','android-build-manifest.json'),('LICENSE','LICENSE'),('BUNDLED_THIRD_PARTY_NOTICES.md','THIRD_PARTY_NOTICES.md'),('build-modules.txt','build-modules-android-arm64.txt'),('build-modules-darwin-arm64.txt','build-modules-darwin-arm64.txt')]:shutil.copyfile(root/source,out/name)
 (out/'rebuild/patches').mkdir(parents=True,exist_ok=True)
 for name in ['build-android.sh','write-manifest.py']:
  shutil.copyfile(root/name,out/'rebuild'/name)
 for source in sorted((root/'patches').glob('*.patch')):
  (out/'patches').mkdir(exist_ok=True);shutil.copyfile(source,out/'patches'/source.name)
  shutil.copyfile(source,out/'rebuild/patches'/source.name)
 print(sdk,': fixed target bytes and original notices copied')
