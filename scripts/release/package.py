#!/usr/bin/env python3
"""Package committed public inputs and freshly built outputs; never reuse a local audit freeze."""
import argparse
import importlib.util
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'dist' / 'release'
PROJECT_VERSION = '0.8.0'
VERSIONS = {'go': '0.4.0', 'rust': '0.3.0', 'lab': '0.4.0', 'js': '0.2.0', 'server': '0.2.0', 'sample': '0.1.0'}


def public_sources(*prefixes):
    spec = importlib.util.spec_from_file_location('source_policy', ROOT / 'scripts/release/plan-source.py')
    policy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(policy)
    files = [path for path in tracked(*prefixes) if policy.eligible(path.relative_to(ROOT))]
    if any(not policy.binary_allowed(path.relative_to(ROOT), path.read_bytes()) for path in files):
        raise RuntimeError('Unreviewed binary in public source inputs')
    return files


def expected_assets():
    return {
        *(f'{name}-{version}-source.tar.gz' for name, version in [
            ('cybergrid-sdk', VERSIONS['go']), ('cybergrid-sdk-rust', VERSIONS['rust']), ('cybergrid-sdk-js', VERSIONS['js']),
            ('CyberGrid-Lab', VERSIONS['lab']), ('CyberGrid', PROJECT_VERSION)]),
        'cybergrid-sdk-source-manifest.json', 'cybergrid-sdk-rust-source-manifest.json',
        'cybergrid-sdk-js-source-manifest.json', 'lab-source-manifest.json', 'cybergrid-source-manifest.json',
        f'cybergrid-sdk-js-{VERSIONS["js"]}-esm.tar.gz', 'CyberGrid-licenses.tar.gz',
        f'CyberGrid-Lab-{VERSIONS["lab"]}-macos-arm64-test.zip',
        f'CyberGrid-Lab-{VERSIONS["lab"]}-android-arm64-test.apk',
        f'CyberGrid-Lab-{VERSIONS["lab"]}-android-arm64-test.zip',
        f'cybergrid-sdk-{VERSIONS["go"]}-macos-arm64.tar.gz',
        f'cybergrid-sdk-{VERSIONS["go"]}-android-arm64.tar.gz',
        'build-sources.json', 'build-macos.json', 'build-android.json',
    }


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def tracked(*prefixes):
    names = subprocess.check_output(['git', 'ls-files', '-z', '--', *prefixes], cwd=ROOT).decode().split('\0')
    files = sorted(ROOT / name for name in names if name)
    if not files:
        raise RuntimeError('No committed public source inputs')
    for path in files:
        if path.is_symlink() or not path.is_file():
            raise RuntimeError('Nonregular public source input')
    return files


def write_json(name, data):
    (OUT / name).write_text(json.dumps(data, indent=2, sort_keys=True) + '\n')


def source_map(prefix):
    return {p.relative_to(ROOT / prefix).as_posix(): sha(p) for p in public_sources(prefix)}


def tar(name, paths, prefix, extra=None):
    with (OUT / name).open('wb') as raw:
        with gzip.GzipFile(fileobj=raw, filename='', mode='wb', mtime=0) as zipped:
            with tarfile.open(fileobj=zipped, mode='w', format=tarfile.PAX_FORMAT) as archive:
                entries = [(relative, path.read_bytes(), 0o755 if os.access(path, os.X_OK) else 0o644) for path, relative in paths]
                entries += [(relative, data, 0o644) for relative, data in (extra or {}).items()]
                seen = set()
                for relative, data, mode in sorted(entries):
                    if relative in seen or relative.startswith('/') or '..' in Path(relative).parts:
                        raise RuntimeError('Unsafe or duplicate archive input')
                    seen.add(relative)
                    info = tarfile.TarInfo(prefix + '/' + relative)
                    info.size, info.mtime, info.mode = len(data), 0, mode
                    info.uid = info.gid = 0
                    info.uname = info.gname = ''
                    archive.addfile(info, io.BytesIO(data))


def providers():
    canonical = ROOT / 'tools/cloudflared/bundled-manifest.json'
    manifest = json.loads(canonical.read_text())
    for prefix in ['sdk', 'sdk-rust']:
        root = ROOT / prefix / 'provider-assets'
        if sha(root / 'manifest.json') != sha(canonical):
            raise RuntimeError('SDK resource manifest differs from canonical')
        for item in manifest['platforms'].values():
            file = root / item['binary']
            if file.stat().st_size != item['bytes'] or sha(file) != item['sha256']:
                raise RuntimeError('Provider resource differs from fixed pin')
    return manifest


def build_metadata(platform):
    return {'platform': platform, 'projectVersion': PROJECT_VERSION, 'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip(),
            'versions': VERSIONS, 'profile': 'debug-test' if platform != 'sources' else 'source',
            'githubRunId': os.environ.get('GITHUB_RUN_ID'), 'githubRunAttempt': os.environ.get('GITHUB_RUN_ATTEMPT'),
            'sourceDateEpoch': int(os.environ.get('SOURCE_DATE_EPOCH', '0')), 'providerManifestSha256': sha(ROOT / 'tools/cloudflared/bundled-manifest.json')}


def package_repository_sources():
    # Package only the explicitly reviewed public tree. The complete repository
    # preserves the sample's relative SDK, server and official-owner imports.
    files = public_sources()
    mapping = {p.relative_to(ROOT).as_posix(): sha(p) for p in files}
    meta = {**build_metadata('sources'), 'projectVersion': PROJECT_VERSION, 'sha256': mapping}
    write_json('cybergrid-source-manifest.json', meta)
    tar(f'CyberGrid-{PROJECT_VERSION}-source.tar.gz', [(p, p.relative_to(ROOT).as_posix()) for p in files],
        f'CyberGrid-{PROJECT_VERSION}', {'SOURCE-MANIFEST.json': (json.dumps(meta, indent=2, sort_keys=True) + '\n').encode()})


def package_sources():
    package_repository_sources()
    for prefix, name, version in [('sdk', 'cybergrid-sdk', VERSIONS['go']), ('sdk-rust', 'cybergrid-sdk-rust', VERSIONS['rust']), ('sdk-js', 'cybergrid-sdk-js', VERSIONS['js'])]:
        files = public_sources(prefix)
        mapping = source_map(prefix)
        meta = {'version': version, 'commit': build_metadata('sources')['commit'], 'sha256': mapping}
        write_json(name + '-source-manifest.json', meta)
        tar(f'{name}-{version}-source.tar.gz', [(p, p.relative_to(ROOT / prefix).as_posix()) for p in files], f'{name}-{version}', {'SOURCE-MANIFEST.json': (json.dumps(meta, indent=2, sort_keys=True) + '\n').encode()})
    js_root = ROOT / 'sdk-js'
    # The sources job builds these exact committed TypeScript inputs first.
    # Only the allowlisted ESM and declaration outputs can enter the browser kit.
    expected_outputs = {module + suffix for module in ['index', 'bridge', 'directory', 'endpoints'] for suffix in ['.js', '.d.ts']}
    outputs = [js_root / 'dist' / name for name in sorted(expected_outputs)]
    if {p.name for p in (js_root / 'dist').iterdir()} != expected_outputs or any(p.is_symlink() or not p.is_file() for p in outputs):
        raise RuntimeError('Build the exact JS ESM and types before source packaging')
    js_files = public_sources('sdk-js/package.json', 'sdk-js/README.md', 'sdk-js/LICENSE', 'sdk-js/notices') + outputs
    js_meta = {**build_metadata('sources'), 'version': VERSIONS['js'], 'sourceManifestSha256': sha(OUT / 'cybergrid-sdk-js-source-manifest.json'), 'sha256': {p.relative_to(js_root).as_posix(): sha(p) for p in js_files}}
    tar(f'cybergrid-sdk-js-{VERSIONS["js"]}-esm.tar.gz', [(p, p.relative_to(js_root).as_posix()) for p in js_files], f'cybergrid-sdk-js-{VERSIONS["js"]}', {'BUILD-INFO.json': (json.dumps(js_meta, indent=2, sort_keys=True) + '\n').encode()})
    app_files = public_sources('apps/tauri-test', 'sdk-rust', 'tools/cloudflared', 'scripts/release', 'docs/lab-browser-login.md')
    mapping = {p.relative_to(ROOT).as_posix(): sha(p) for p in app_files}
    meta = {'version': VERSIONS['lab'], 'commit': build_metadata('sources')['commit'], 'sha256': mapping, 'sdkSourceManifestSha256': sha(OUT / 'cybergrid-sdk-rust-source-manifest.json')}
    write_json('lab-source-manifest.json', meta)
    tar(f'CyberGrid-Lab-{VERSIONS["lab"]}-source.tar.gz', [(p, p.relative_to(ROOT).as_posix()) for p in app_files], f'CyberGrid-Lab-{VERSIONS["lab"]}', {'SOURCE-MANIFEST.json': (json.dumps(meta, indent=2, sort_keys=True) + '\n').encode()})
    license_files = public_sources('LICENSE', 'server/notices', 'sdk/LICENSE', 'sdk/third_party', 'sdk/THIRD_PARTY_NOTICES.md', 'sdk-rust/LICENSE', 'sdk-rust/licenses', 'sdk-rust/THIRD_PARTY_NOTICES.md', 'sdk-js/LICENSE', 'sdk-js/notices', 'apps/tauri-test/notices', 'scripts/release/sample-notices', 'tools/cloudflared/LICENSE', 'tools/cloudflared/BUNDLED_THIRD_PARTY_NOTICES.md')
    tar('CyberGrid-licenses.tar.gz', [(p, p.relative_to(ROOT).as_posix()) for p in license_files], 'CyberGrid-licenses')
    write_json('build-sources.json', build_metadata('sources'))


def go_package(platform, binary):
    paths = [(binary, 'cybergrid-' + platform)]
    docs = public_sources('sdk/LICENSE', 'sdk/README.md', 'sdk/CHANGELOG.md', 'sdk/THIRD_PARTY_NOTICES.md', 'sdk/third_party', 'sdk/scripts/prepare-android-provider.py', 'sdk/provider-assets')
    for path in docs:
        name = path.relative_to(ROOT / 'sdk').as_posix()
        if 'darwin-arm64/cloudflared' in name:
            continue  # The Mac executable embeds exactly one provider.
        if platform == 'macos-arm64' and 'android-arm64/jniLibs/' in name:
            continue
        paths.append((path, name))
    tar('cybergrid-sdk-' + VERSIONS['go'] + '-' + platform + '.tar.gz', paths, 'cybergrid-sdk-' + VERSIONS['go'] + '-' + platform,
        {'BUILD-INFO.json': (json.dumps(build_metadata(platform), indent=2, sort_keys=True) + '\n').encode()})


def package_macos(app, binary):
    if not app.is_dir() or not binary.is_file():
        raise RuntimeError('Build native Mac app and CLI first')
    native = app / 'Contents/MacOS/cybergrid-tauri-lab'
    subprocess.run(['lipo', str(native), '-verify_arch', 'arm64'], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    target = OUT / f'CyberGrid-Lab-{VERSIONS["lab"]}-macos-arm64-test.zip'
    subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app), str(target)], check=True)
    with zipfile.ZipFile(target, 'a', compression=zipfile.ZIP_DEFLATED) as archive:
        for path in public_sources('apps/tauri-test/notices', 'apps/tauri-test/README.md'):
            archive.write(path, path.relative_to(ROOT / 'apps/tauri-test').as_posix())
    go_package('macos-arm64', binary)
    meta = build_metadata('macos-arm64')
    meta['toolchains'] = {name: subprocess.check_output(args).decode().strip() for name, args in {'go': ['go', 'version'], 'rust': ['rustc', '--version'], 'node': ['node', '--version']}.items()}
    meta.update({'notarized': False, 'appBinarySha256': sha(native), 'goBinarySha256': sha(binary)})
    write_json('build-macos.json', meta)


def package_android(apk, binary, manifest):
    if not apk.is_file() or not binary.is_file():
        raise RuntimeError('Build Android app and CLI first')
    fixed = manifest['platforms']['android-arm64']
    if os.environ.get('CI'):
        aapt = Path(os.environ['ANDROID_HOME']) / 'build-tools/36.0.0/aapt2'
        badging = subprocess.check_output([str(aapt), 'dump', 'badging', str(apk)]).decode()
        lines = [line.strip() for line in badging.splitlines()]
        minimums = [line for line in lines if line.startswith(('sdkVersion:', 'minSdkVersion:'))]
        native = [line for line in lines if line.startswith('native-code:')]
        if len(minimums) != 1 or minimums[0] not in {"sdkVersion:'28'", "minSdkVersion:'28'"} or 'application-debuggable' not in lines or native != ["native-code: 'arm64-v8a'"]:
            raise RuntimeError('APK must be API28 debug-test arm64 only')
    with zipfile.ZipFile(apk) as archive:
        names = [n for n in archive.namelist() if n.endswith('/libcloudflared.so')]
        if names != ['lib/arm64-v8a/libcloudflared.so']:
            raise RuntimeError('APK must have exactly one installed arm64 provider')
        data = archive.read(names[0])
        if len(data) != fixed['bytes'] or hashlib.sha256(data).hexdigest() != fixed['sha256']:
            raise RuntimeError('APK provider changed')
    target = OUT / f'CyberGrid-Lab-{VERSIONS["lab"]}-android-arm64-test.apk'
    shutil.copyfile(apk, target)
    with zipfile.ZipFile(OUT / f'CyberGrid-Lab-{VERSIONS["lab"]}-android-arm64-test.zip', 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        archive.write(target, target.name)
        for path in public_sources('apps/tauri-test/notices', 'apps/tauri-test/README.md', 'apps/tauri-test/tools/android-adb.py'):
            archive.write(path, path.relative_to(ROOT / 'apps/tauri-test').as_posix())
    go_package('android-arm64', binary)
    meta = build_metadata('android-arm64')
    meta['toolchains'] = {name: subprocess.check_output(args).decode().strip() for name, args in {'go': ['go', 'version'], 'rust': ['rustc', '--version'], 'node': ['node', '--version']}.items()}
    meta.update({'minSdk': 28, 'targetSdk': 37, 'ndk': '27.2.12479018', 'signing': 'ephemeral CI debug key; upgrade from another key requires uninstall or a user-managed signing key', 'adbControl': 'android+debug_assertions+adb-test-control', 'providerCopies': 1, 'providerSha256': fixed['sha256'], 'goBinarySha256': sha(binary)})
    write_json('build-android.json', meta)


def finalize():
    if os.environ.get('RELEASE_TAG', 'v' + PROJECT_VERSION) != 'v' + PROJECT_VERSION:
        raise RuntimeError('Release tag differs from reviewed project version')
    apk = OUT / f'CyberGrid-Lab-{VERSIONS["lab"]}-android-arm64-test.apk'
    wrapper = OUT / f'CyberGrid-Lab-{VERSIONS["lab"]}-android-arm64-test.zip'
    if apk.is_file() and not wrapper.exists():
        with zipfile.ZipFile(wrapper, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(apk, apk.name)
            for path in public_sources('apps/tauri-test/notices', 'apps/tauri-test/README.md', 'apps/tauri-test/tools/android-adb.py'):
                archive.write(path, path.relative_to(ROOT / 'apps/tauri-test').as_posix())
    for name in ['build-macos.json', 'build-android.json', 'build-sources.json']:
        if not (OUT / name).is_file():
            raise RuntimeError('All required jobs must complete before Release')
        data = json.loads((OUT / name).read_text())
        current = build_metadata('release')
        if data['commit'] != current['commit'] or data['versions'] != VERSIONS or data['providerManifestSha256'] != current['providerManifestSha256']:
            raise RuntimeError('Build job provenance differs from this release commit')
    expected = expected_assets()
    actual = {p.name for p in OUT.iterdir() if p.is_file()} - {'SHA256SUMS', 'release-manifest.json'}
    if actual != expected:
        raise RuntimeError('Unexpected or incomplete release asset allowlist')
    files = sorted(p for p in OUT.iterdir() if p.is_file() and p.name not in {'SHA256SUMS', 'release-manifest.json'})
    meta = {'projectRelease': os.environ.get('RELEASE_TAG', 'unreleased'), 'commit': build_metadata('release')['commit'], 'versions': VERSIONS,
            'profile': 'debug-test', 'license': 'CyberGrid original code: all rights reserved; third-party licenses preserved',
            'artifacts': {p.name: {'bytes': p.stat().st_size, 'sha256': sha(p)} for p in files}}
    write_json('release-manifest.json', meta)
    files.append(OUT / 'release-manifest.json')
    (OUT / 'SHA256SUMS').write_text(''.join(sha(p) + '  ' + p.name + '\n' for p in sorted(files)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['sources', 'macos', 'android', 'finalize'])
    parser.add_argument('--app', type=Path)
    parser.add_argument('--apk', type=Path)
    parser.add_argument('--go-binary', type=Path)
    args = parser.parse_args()
    if subprocess.run(['git', 'diff', '--quiet', 'HEAD', '--'], cwd=ROOT).returncode != 0:
        raise RuntimeError('Package only a clean reviewed commit; tracked inputs differ from HEAD')
    OUT.mkdir(parents=True, exist_ok=True)
    manifest = providers()
    if args.mode == 'sources':
        package_sources()
    elif args.mode == 'macos':
        package_macos(args.app.resolve(), args.go_binary.resolve())
    elif args.mode == 'android':
        package_android(args.apk.resolve(), args.go_binary.resolve(), manifest)
    else:
        finalize()
    print('Packaged ' + args.mode + ' from current committed inputs into dist/release; no credentials or local audit freeze used.')


if __name__ == '__main__':
    main()
