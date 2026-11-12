#!/usr/bin/env python3
"""Release version, full-source and privacy boundary regressions; no native build."""
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('release_package', ROOT / 'scripts/release/package.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackagingTests(unittest.TestCase):
    def test_component_and_self_lock_versions_match(self):
        for name, path in [('js', 'sdk-js/package'), ('server', 'server/package'), ('lab', 'apps/tauri-test/package')]:
            metadata = json.loads((ROOT / (path + '.json')).read_text())
            lock = json.loads((ROOT / (path + '-lock.json')).read_text())
            self.assertEqual(metadata['version'], package.VERSIONS[name])
            self.assertEqual(lock['version'], metadata['version'])
            self.assertEqual(lock['packages']['']['version'], metadata['version'])
        for path in ['sdk-rust/Cargo.lock', 'apps/tauri-test/src-tauri/Cargo.lock', 'examples/ram-only-workspace/Cargo.lock']:
            text = (ROOT / path).read_text()
            match = re.search(r'\[\[package\]\]\nname = "cybergrid-sdk"\nversion = "([^"]+)"', text)
            self.assertEqual(match.group(1), package.VERSIONS['rust'])
        self.assertEqual(json.loads((ROOT / 'apps/tauri-test/src-tauri/tauri.conf.json').read_text())['version'], package.VERSIONS['lab'])

    def test_exact_asset_set_tracks_versions_including_complete_source(self):
        assets = package.expected_assets()
        self.assertIn(f'CyberGrid-{package.PROJECT_VERSION}-source.tar.gz', assets)
        self.assertIn('cybergrid-source-manifest.json', assets)
        self.assertIn(f'cybergrid-sdk-js-{package.VERSIONS["js"]}-esm.tar.gz', assets)
        self.assertNotIn('CyberGrid-Lab-0.3.0-source.tar.gz', assets)
        with patch.dict(package.VERSIONS, {'lab': '9.8.7'}):
            self.assertIn('CyberGrid-Lab-9.8.7-android-arm64-test.apk', package.expected_assets())
            self.assertNotIn('CyberGrid-Lab-0.4.0-android-arm64-test.apk', package.expected_assets())

    def test_wrong_release_tag_fails_before_asset_processing(self):
        with patch.dict(os.environ, {'RELEASE_TAG': 'v0.7.0'}):
            with self.assertRaisesRegex(RuntimeError, 'tag differs'):
                package.finalize()

    def test_finalizer_requires_exact_inventory_and_hashes_complete_source(self):
        # Explicit synthetic finalizer fixture, never a native build/release.
        with tempfile.TemporaryDirectory() as folder:
            output = Path(folder)
            metadata = {'commit': 'FixtureCommit', 'versions': package.VERSIONS, 'providerManifestSha256': 'FixtureProvider'}
            for name in package.expected_assets():
                (output / name).write_bytes(b'Synthetic finalizer fixture only\n')
            for name in ['build-macos.json', 'build-android.json', 'build-sources.json']:
                (output / name).write_text(json.dumps(metadata))
            with patch.object(package, 'OUT', output), patch.object(package, 'build_metadata', return_value=metadata), patch.dict(os.environ, {'RELEASE_TAG': 'v' + package.PROJECT_VERSION}):
                (output / 'unexpected.txt').write_text('must fail')
                with self.assertRaisesRegex(RuntimeError, 'asset allowlist'):
                    package.finalize()
                (output / 'unexpected.txt').unlink()
                package.finalize()
            manifest = json.loads((output / 'release-manifest.json').read_text())
            name = f'CyberGrid-{package.PROJECT_VERSION}-source.tar.gz'
            self.assertEqual(manifest['artifacts'][name]['sha256'], package.sha(output / name))
            self.assertIn('cybergrid-source-manifest.json', manifest['artifacts'])
            self.assertIn(name, (output / 'SHA256SUMS').read_text())

    def fixture(self, root):
        scripts = root / 'scripts/release'
        scripts.mkdir(parents=True)
        for name in ['plan-source.py', 'source-policy.json']:
            shutil.copyfile(ROOT / 'scripts/release' / name, scripts / name)
        for path, text in {
            'README.md': 'Public fixture\n',
            'tools/cloudflared/bundled-manifest.json': '{}\n',
            'sdk-rust/Cargo.toml': '[package]\nname="fixture"\n',
            'examples/ram-only-workspace/web/.env.example': 'PUBLIC_EXAMPLE=fixture.invalid\n',
            'examples/ram-only-workspace/web/.env.local': 'PRIVATE_FIXTURE=must-not-archive\n',
            'examples/ram-only-workspace/desktop/src-tauri/Cargo.toml': '[dependencies]\ncybergrid-sdk={path="../../../../sdk-rust"}\n',
        }.items():
            file = root / path
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text(text)
        subprocess.run(['git', 'init', '-q', str(root)], check=True)
        subprocess.run(['git', 'add', '-f', '.'], cwd=root, check=True)
        subprocess.run(['git', '-c', 'user.name=Release Fixture', '-c', 'user.email=fixture@invalid.test', 'commit', '-qm', 'fixture'], cwd=root, check=True)

    def test_real_git_full_archive_manifest_and_relative_dependencies(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.fixture(root)
            output = root / 'out'
            output.mkdir()
            with patch.object(package, 'ROOT', root), patch.object(package, 'OUT', output):
                package.package_repository_sources()
            manifest = json.loads((output / 'cybergrid-source-manifest.json').read_text())
            prefix = 'CyberGrid-' + package.PROJECT_VERSION + '/'
            with tarfile.open(output / (prefix[:-1] + '-source.tar.gz')) as archive:
                entries = {m.name[len(prefix):]: archive.extractfile(m).read() for m in archive if m.isfile()}
            self.assertEqual(entries.pop('SOURCE-MANIFEST.json'), (output / 'cybergrid-source-manifest.json').read_bytes())
            self.assertEqual(set(entries), set(manifest['sha256']))
            self.assertIn('examples/ram-only-workspace/web/.env.example', entries)
            self.assertNotIn('examples/ram-only-workspace/web/.env.local', entries)
            self.assertIn('sdk-rust/Cargo.toml', entries)
            import hashlib
            for name, data in entries.items():
                self.assertEqual(hashlib.sha256(data).hexdigest(), manifest['sha256'][name])
            self.assertEqual(manifest['commit'], subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip())

    def test_tracked_unreviewed_binary_cannot_enter_full_source(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.fixture(root)
            file = root / 'examples/ram-only-workspace/extra.png'
            file.write_bytes(b'\x89PNG\0unreviewed')
            subprocess.run(['git', 'add', str(file)], cwd=root, check=True)
            with patch.object(package, 'ROOT', root):
                with self.assertRaisesRegex(RuntimeError, 'Unreviewed binary'):
                    package.public_sources()


    def test_all_component_sources_and_attachments_share_public_policy(self):
        # Real clean Git input, actual sources entry point and actual archives.
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.fixture(root)
            legal = {
                'LICENSE': 'Project license fixture\n',
                'sdk/LICENSE': 'Go original license fixture\n',
                'sdk/main.go': 'package fixture\n',
                'sdk/third_party/Fixture/LICENSE': 'Third-party fixture\n',
                'sdk/provider-assets/manifest.json': '{"platforms":{}}',
                'sdk-rust/LICENSE': 'Rust original license fixture\n',
                'sdk-rust/src/lib.rs': '// public Rust fixture\n',
                'sdk-rust/provider-assets/manifest.json': '{"platforms":{}}',
                'sdk-js/LICENSE': 'JS original license fixture\n',
                'sdk-js/README.md': 'JS public fixture\n',
                'sdk-js/package.json': '{"type":"module"}',
                'sdk-js/src/index.ts': 'export {};\n',
                'sdk-js/notices/Fixture/LICENSE': 'JS third-party fixture\n',
                'apps/tauri-test/README.md': 'Lab public fixture\n',
                'apps/tauri-test/notices/Fixture/LICENSE': 'Lab third-party fixture\n',
                'server/notices/Fixture/LICENSE': 'Server third-party fixture\n',
                'scripts/release/sample-notices/Fixture/LICENSE': 'Sample third-party fixture\n',
                'tools/cloudflared/bundled-manifest.json': '{"platforms":{}}',
            }
            forbidden = [
                'sdk/.env.local', 'sdk-rust/.env', 'sdk-js/.env.production',
                'apps/tauri-test/.env.local', 'sdk-js/notices/.env.local',
                'sdk/third_party/private.key', 'sdk-rust/licenses/receipt.log',
                'apps/tauri-test/notices/stale.bak',
                'server/notices/.env', 'scripts/release/sample-notices/.env.local',
                'tools/cloudflared/diagnostics/fixture.txt',
            ]
            for name, text in {**legal, **dict.fromkeys(forbidden, 'Synthetic forbidden fixture\n')}.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            subprocess.run(['git', 'add', '-f', '.'], cwd=root, check=True)
            subprocess.run(['git', '-c', 'user.name=Release Fixture', '-c', 'user.email=fixture@invalid.test', 'commit', '-qm', 'component fixture'], cwd=root, check=True)
            for module in ['index', 'bridge', 'directory', 'endpoints']:
                for suffix in ['.js', '.d.ts']:
                    path = root / 'sdk-js/dist' / (module + suffix)
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text('export {};\n')
            output = root / 'out'
            with patch.object(package, 'ROOT', root), patch.object(package, 'OUT', output), patch('sys.argv', ['package.py', 'sources']):
                package.main()
                binary = root / ('synthetic-' + 'executable')
                binary.write_text('Explicit synthetic native executable fixture\n')
                package.go_package('macos-arm64', binary)
            import hashlib
            for archive_path in output.glob('*.tar.gz'):
                with tarfile.open(archive_path) as archive:
                    entries = {m.name.split('/', 1)[1]: archive.extractfile(m).read() for m in archive if m.isfile()}
                self.assertFalse(any('Synthetic forbidden fixture' in data.decode('utf-8', errors='ignore') for data in entries.values()), archive_path.name)
                embedded = entries.pop('SOURCE-MANIFEST.json', None)
                if embedded:
                    mapping = json.loads(embedded)['sha256']
                    self.assertEqual(set(entries), set(mapping), archive_path.name)
                    self.assertEqual({n: hashlib.sha256(b).hexdigest() for n, b in entries.items()}, mapping)
                    external = 'cybergrid-source-manifest.json' if archive_path.name.startswith('CyberGrid-0.') else 'lab-source-manifest.json' if archive_path.name.startswith('CyberGrid-Lab-') else re.sub(r'-[0-9]+\.[0-9]+\.[0-9]+-source\.tar\.gz$', '-source-manifest.json', archive_path.name)
                    self.assertEqual(embedded, (output / external).read_bytes())
            for component, name, version in [('sdk', 'cybergrid-sdk', package.VERSIONS['go']), ('sdk-rust', 'cybergrid-sdk-rust', package.VERSIONS['rust']), ('sdk-js', 'cybergrid-sdk-js', package.VERSIONS['js'])]:
                with tarfile.open(output / f'{name}-{version}-source.tar.gz') as archive:
                    entries = {m.name.split('/', 1)[1]: archive.extractfile(m).read() for m in archive if m.isfile()}
                for path, text in legal.items():
                    if path.startswith(component + '/'):
                        self.assertEqual(entries[path[len(component) + 1:]], text.encode())
            with tarfile.open(output / 'CyberGrid-licenses.tar.gz') as archive:
                entries = {m.name.split('/', 1)[1]: archive.extractfile(m).read() for m in archive if m.isfile()}
            for path, text in legal.items():
                if path.endswith('/LICENSE') or path == 'LICENSE':
                    self.assertEqual(entries[path], text.encode())

    def test_component_and_notice_binary_policy_cannot_be_bypassed(self):
        for name in ['sdk/extra.bin', 'sdk-rust/licenses/extra.bin', 'sdk-js/notices/extra.bin', 'apps/tauri-test/notices/extra.bin', 'server/notices/extra.bin']:
            with self.subTest(name=name), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                self.fixture(root)
                file = root / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_bytes(b'\x00Synthetic unreviewed binary')
                subprocess.run(['git', 'add', '-f', name], cwd=root, check=True)
                with patch.object(package, 'ROOT', root):
                    with self.assertRaisesRegex(RuntimeError, 'Unreviewed binary'):
                        package.public_sources(name.split('/')[0])


if __name__ == '__main__':
    unittest.main()
