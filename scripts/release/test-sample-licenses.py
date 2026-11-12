#!/usr/bin/env python3
"""Cross-platform license provenance negatives; fixtures never fetch binaries."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('sample_licenses', Path(__file__).with_name('collect-sample-licenses.py'))
licenses = importlib.util.module_from_spec(spec)
spec.loader.exec_module(licenses)


class LicenseTests(unittest.TestCase):
    def test_optional_present_or_absent_uses_identical_exact_wrapper(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            wrapper = root / 'examples/ram-only-workspace/desktop/node_modules/esbuild'
            wrapper.mkdir(parents=True)
            (wrapper / 'package.json').write_text('{"version":"1.2.3"}')
            (wrapper / 'LICENSE').write_text('Original fixture text\n')
            meta = {'version': '1.2.3', 'resolved': 'https://registry.npmjs.org/fixture', 'integrity': 'sha512-FixtureIntegrity', 'license': 'MIT', 'optional': True}
            metadata = json.dumps({'dist': {'integrity': meta['integrity']}, 'license': 'MIT'}).encode()
            optional = root / 'installed-optional'
            optional.mkdir()
            (optional / 'package.json').write_text('{"version":"1.2.3"}')
            (optional / 'LICENSE').write_text('Must not choose OS-dependent text path')
            with patch.object(licenses, 'ROOT', root), patch.object(licenses.npm, 'public_bytes', return_value=metadata):
                installed = licenses.entry('@esbuild/darwin-arm64', meta, [optional])
                absent = licenses.entry('@esbuild/darwin-arm64', meta, [])
                self.assertEqual(installed, absent)
                with patch.object(licenses.npm, 'public_bytes', return_value=b'{"dist":{"integrity":"wrong"},"license":"MIT"}'):
                    with self.assertRaisesRegex(RuntimeError, 'metadata differs'):
                        licenses.entry('@esbuild/darwin-arm64', meta, [])
                (wrapper / 'LICENSE').unlink()
                with self.assertRaisesRegex(RuntimeError, 'Missing exact-version'):
                    licenses.entry('@esbuild/darwin-arm64', meta, [])

    def test_regular_installed_archive_sources_are_deterministic(self):
        with tempfile.TemporaryDirectory() as folder:
            package = Path(folder)
            (package / 'package.json').write_text('{"version":"1.2.3"}')
            (package / 'LICENSE').write_bytes(b'Original fixture text\n')
            meta = {'version': '1.2.3', 'resolved': 'https://registry.npmjs.org/fixture', 'integrity': 'sha512-FixtureIntegrity', 'license': 'MIT'}
            installed = licenses.entry('fixture', meta, [package])
            with patch.object(licenses.npm, 'archive_texts', return_value=({'LICENSE': b'Original fixture text\n'}, {'package': 'fixture', 'version': '1.2.3'})):
                self.assertEqual(installed, licenses.entry('fixture', meta, []))
            with patch.object(licenses.npm, 'archive_texts', return_value=({}, {})):
                with self.assertRaisesRegex(RuntimeError, 'Missing exact original'):
                    licenses.entry('fixture', meta, [])


if __name__ == '__main__':
    unittest.main()
