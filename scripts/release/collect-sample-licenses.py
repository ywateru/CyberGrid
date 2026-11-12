#!/usr/bin/env python3
"""Collect exact locked sample npm license texts; never download platform binaries."""
import concurrent.futures
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location('npm_licenses', Path(__file__).with_name('collect-server-licenses.py'))
npm = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(npm)
DESTINATION = ROOT / 'scripts/release/sample-notices'


def entry(name, meta, folders):
    version = meta['version']
    source = {'package': name, 'version': version, 'archive': meta['resolved'], 'integrity': meta['integrity']}
    texts = None
    wrapper = next((candidate for prefix, candidate in [('@esbuild/', 'esbuild'), ('@rollup/rollup-', 'rollup'), ('@tauri-apps/cli-', '@tauri-apps/cli')] if name.startswith(prefix)), None)
    if wrapper:
        # Always use this path, even when this OS installed the optional binary.
        # This keeps both text paths and provenance identical across platforms.
        metadata = json.loads(npm.public_bytes(f'https://registry.npmjs.org/{name}/{version}'))
        if metadata.get('dist', {}).get('integrity') != meta['integrity'] or metadata.get('license') != meta.get('license'):
            raise RuntimeError('Optional npm package metadata differs from lock: ' + name)
        for root in ['desktop', 'web']:
            folder = ROOT / 'examples/ram-only-workspace' / root / 'node_modules' / wrapper
            if folder.is_dir() and json.loads((folder / 'package.json').read_text())['version'] == version:
                texts = {'same-version-wrapper/' + key: value for key, value in npm.local_texts(folder).items()}
                if texts:
                    source.update({'textSource': 'Same-version upstream wrapper; optional binary not downloaded', 'wrapperPackage': wrapper, 'wrapperVersion': version, 'optionalMetadataURL': f'https://registry.npmjs.org/{name}/{version}'})
                    break
        if not texts:
            raise RuntimeError('Missing exact-version optional wrapper license: ' + name)
    else:
        source['textSource'] = 'Exact locked npm package (installed or integrity-verified archive)'
        for folder in folders:
            if folder.is_dir():
                if json.loads((folder / 'package.json').read_text())['version'] != version:
                    raise RuntimeError('Installed sample package differs from lock: ' + name)
                texts = npm.local_texts(folder)
                if texts:
                    break
    if not texts:
        texts, archive_source = npm.archive_texts(meta, name)
        source.update(archive_source)
    if not texts:
        raise RuntimeError('Missing exact original sample license: ' + name)
    return {'name': name, 'version': version, 'license': meta.get('license'), 'optional': bool(meta.get('optional')), 'developmentOnly': bool(meta.get('dev')), 'source': source, 'texts': {key: hashlib.sha256(data).hexdigest() for key, data in sorted(texts.items())}}, texts


def collect(out):
    packages, locks = {}, {}
    for root in ['desktop', 'web']:
        path = ROOT / 'examples/ram-only-workspace' / root / 'package-lock.json'
        locks[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
        for relative, meta in json.loads(path.read_text())['packages'].items():
            if not relative:
                continue
            name = relative.split('node_modules/')[-1]
            key = (name, meta['version'])
            folder = path.parent / relative
            if key in packages:
                if packages[key][0]['integrity'] != meta['integrity']:
                    raise RuntimeError('Same sample package version has conflicting lock integrity')
                packages[key][1].append(folder)
            else:
                packages[key] = (meta, [folder])
    def one(item):
        (name, _), (meta, folders) = item
        return entry(name, meta, folders)
    records = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        for record, texts in pool.map(one, sorted(packages.items())):
            records.append(record)
            folder = out / 'npm' / (record['name'].replace('/', '__') + '-' + record['version'])
            for relative, data in texts.items():
                file = folder / relative
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_bytes(data)
    (out / 'manifest.json').write_text(json.dumps({'scope': 'All locked desktop/Web npm packages including optional platforms and build dependencies; original license bytes only, not dependency code/binaries', 'locks': locks, 'packages': records}, indent=2) + '\n')
    (out / 'README.md').write_text('# RAM-only sample npm notices\n\nExact locked desktop/Web dependency license texts. No dependency implementation or optional binary is vendored. Same-version platform-wrapper text is explicitly labelled and requires exact optional-package metadata integrity/license validation; other absent packages require their integrity-verified archive. Missing text blocks collection. Regenerate with `python3 scripts/release/collect-sample-licenses.py` after the sample npm ci commands. Rust texts remain in sdk-rust/licenses and apps/tauri-test/notices/rust; the final release audits sample Cargo.lock against those exact versions. Project LICENSE grants remain unchanged.\n')
    return len(records)


def tree(root):
    return {p.relative_to(root).as_posix(): p.read_bytes() for p in root.rglob('*') if p.is_file()}


def main():
    with tempfile.TemporaryDirectory(prefix='cybergrid-sample-notices-') as folder:
        out = Path(folder)
        count = collect(out)
        if '--check' in sys.argv:
            if tree(out) != tree(DESTINATION):
                raise SystemExit('Sample notices differ from exact locks and original license texts')
        else:
            if DESTINATION.exists():
                shutil.rmtree(DESTINATION)
            shutil.copytree(out, DESTINATION)
        print(f'Checked {count} exact locked sample npm package licenses')


if __name__ == '__main__':
    main()
