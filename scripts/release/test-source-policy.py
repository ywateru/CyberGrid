#!/usr/bin/env python3
"""Focused public-policy negatives; no cloud/build/private data required."""
import importlib.util
from pathlib import Path
import unittest
import json
import hashlib
import shutil
import subprocess
import tempfile
import sys

spec = importlib.util.spec_from_file_location('policy', Path(__file__).with_name('plan-source.py'))
policy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(policy)

class PublicPolicyTests(unittest.TestCase):
    def test_sample_icon_is_exact_binary_exception(self):
        icon = 'examples/ram-only-workspace/desktop/src-tauri/icons/icon.png'
        self.assertTrue(policy.eligible(Path(icon)))
        self.assertTrue(policy.binary_allowed(Path(icon), b'\x89PNG\0fixture'))
        for name in [icon + '.backup.png', icon.replace('icon.png', 'extra.png'), 'examples/ram-only-workspace/private.bin']:
            self.assertFalse(policy.binary_allowed(Path(name), b'\0binary'))

    def test_sample_private_and_generated_files_are_excluded(self):
        for name in ['.env', 'target/program', 'desktop/node_modules/example.js', 'desktop/dist/index.js', 'private-device/state.json', 'credentials/owner.json', 'sample.log']:
            self.assertFalse(policy.eligible(Path('examples/ram-only-workspace') / name))
        self.assertTrue(policy.eligible(Path('examples/ram-only-workspace/desktop/src/ram-owner.ts')))

    def test_web_configuration_example_is_an_exact_exception(self):
        self.assertTrue(policy.eligible(Path('examples/ram-only-workspace/web/.env.example')))
        for name in ['.env', '.env.local', '.env.example.backup', '.env.production', 'nested/.env.example']:
            self.assertFalse(policy.eligible(Path('examples/ram-only-workspace/web') / name))
        for name in policy.POLICY['publicDocs'] + policy.POLICY['exactFiles']:
            self.assertTrue((policy.ROOT / name).is_file(), 'Missing explicit public input: ' + name)

    def test_actual_plan_and_export_reject_unreviewed_sample_binary(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            scripts = root / 'scripts/release'
            scripts.mkdir(parents=True)
            for name in ['plan-source.py', 'export-source.py', 'source-policy.json']:
                shutil.copyfile(Path(__file__).with_name(name), scripts / name)
            candidate = root / 'examples/ram-only-workspace/extra.png'
            candidate.parent.mkdir(parents=True)
            candidate.write_bytes(b'\x89PNG\0unreviewed')
            plan = root / 'plan.json'
            result = subprocess.run([sys.executable, str(scripts / 'plan-source.py'), '--out', str(plan)], capture_output=True)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(json.loads(plan.read_text())['status'], 'PLAN_BLOCKED_BINARY_POLICY')
            # A hand-edited plan cannot bypass the same exporter policy.
            plan.write_text(json.dumps({'keep': [{'path': candidate.relative_to(root).as_posix(), 'sha256': hashlib.sha256(candidate.read_bytes()).hexdigest()}]}))
            out = root / 'snapshot'
            result = subprocess.run([sys.executable, str(scripts / 'export-source.py'), '--plan', str(plan), '--out', str(out), '--report', str(root / 'report.json')], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'outside current public policy', result.stderr)
            self.assertFalse((out / candidate.relative_to(root)).exists())

    def test_actual_scan_rejects_finder_metadata(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            snapshot = root / 'snapshot'
            snapshot.mkdir()
            (snapshot / '.DS_Store').write_bytes(b'Finder-local-metadata')
            report = root / 'report.json'
            result = subprocess.run([sys.executable, str(Path(__file__).with_name('scan-source.py')), str(snapshot), '--report', str(report)], capture_output=True)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(json.loads(report.read_text())['findings'][0]['path'], '.DS_Store')

    def test_actual_export_rejects_extra_output_not_in_plan(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            scripts = root / 'scripts/release'
            scripts.mkdir(parents=True)
            for name in ['plan-source.py', 'export-source.py', 'source-policy.json']:
                shutil.copyfile(Path(__file__).with_name(name), scripts / name)
            modules = b'cloudflared-android-arm64: go1.fixture\n'
            binary = b'\0public-provider-fixture'
            manifest = json.dumps({'platforms': {key: {'sha256': hashlib.sha256(binary).hexdigest()} for key in ['darwin-arm64', 'android-arm64']}}).encode()
            for prefix in ['tools/cloudflared', 'sdk/provider-assets', 'sdk-rust/provider-assets', 'apps/tauri-test/notices/cloudflared']:
                base = root / prefix
                base.mkdir(parents=True, exist_ok=True)
                canonical = prefix == 'tools/cloudflared'
                (base / ('build-modules.txt' if canonical else 'build-modules-android-arm64.txt')).write_bytes(modules)
                (base / ('bundled-manifest.json' if canonical else 'manifest.json')).write_bytes(manifest)
                (base / ('manifest.json' if canonical else 'android-build-manifest.json')).write_text('{}')
            notice = root / 'apps/tauri-test/notices/cloudflared/source-files-sha256.json'
            notice.write_text('{}')
            binaries = root / 'tools/cloudflared/bin'
            binaries.mkdir()
            for name in ['cloudflared-darwin-arm64', 'cloudflared-android-arm64']:
                (binaries / name).write_bytes(binary)
            plan = root / 'plan.json'
            result = subprocess.run([sys.executable, str(scripts / 'plan-source.py'), '--out', str(plan)], capture_output=True)
            self.assertEqual(result.returncode, 0)
            snapshot = root / 'snapshot'
            report = root / 'report.json'
            # Invoke the actual exporter, substituting only copy I/O to inject
            # an extra output. No product/export validation function is changed.
            wrapper = root / 'inject-output.py'
            wrapper.write_text("import pathlib,runpy,shutil,sys\noriginal=shutil.copyfile\ndef copy(source,target):\n result=original(source,target)\n (pathlib.Path(sys.argv[4])/'unexpected.txt').write_text('unreviewed output')\n return result\nshutil.copyfile=copy\nscript=sys.argv.pop(1)\nrunpy.run_path(script,run_name='__main__')\n")
            result = subprocess.run([sys.executable, str(wrapper), str(scripts / 'export-source.py'), '--plan', str(plan), '--out', str(snapshot), '--report', str(report)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'Snapshot inventory differs from explicit source plan', result.stderr)
            self.assertFalse(report.exists())

    def test_existing_provider_and_build_tool_exceptions_are_explicit(self):
        self.assertTrue(policy.binary_allowed(Path('sdk-rust/provider-assets/darwin-arm64/cloudflared'), b'\0provider'))
        self.assertTrue(policy.binary_allowed(Path('apps/tauri-test/src-tauri/gen/android/gradle/wrapper/gradle-wrapper.jar'), b'\0jar'))
        self.assertFalse(policy.binary_allowed(Path('scripts/release/extra.jar'), b'\0jar'))
        self.assertTrue(policy.binary_allowed(Path('sdk-js/src/index.ts'), b'export {};\n'))
        self.assertFalse(policy.binary_allowed(Path('sdk-js/src/payload.ts'), b'\xffinvalid-utf8'))

if __name__ == '__main__':
    unittest.main()
