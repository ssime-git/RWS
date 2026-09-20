import importlib.util
from datetime import datetime, timezone
import plistlib
import subprocess
import sys
import tempfile
from pathlib import Path
import unittest

MODULE = Path(__file__).resolve().parents[2] / 'scripts/macos_metadata.py'
spec = importlib.util.spec_from_file_location('metadata', MODULE)
metadata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metadata)

class MetadataTests(unittest.TestCase):
    def test_emitted_bundle_has_utc_build_timestamp(self):
        before = datetime.now(timezone.utc).replace(microsecond=0)
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / 'Info.plist'
            subprocess.run([sys.executable, str(MODULE), '--output', str(output)], check=True)
            info = plistlib.loads(output.read_bytes())
        self.assertIn('RWSBuildTimestamp', info)
        value = info['RWSBuildTimestamp']
        self.assertRegex(value, r'^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$')
        timestamp = datetime.strptime(value, '%Y-%m-%dT%H:%M:%SZ').replace(tzinfo=timezone.utc)
        self.assertGreaterEqual(timestamp, before)
        self.assertLessEqual(timestamp, datetime.now(timezone.utc))

    def test_build_revision_is_separate_from_release_version(self):
        info = metadata.bundle_info('0.1.0', False, '', revision='abcdef012345-dirty')
        self.assertEqual(info['RWSBuildRevision'], 'abcdef012345-dirty')
        self.assertEqual(info['CFBundleVersion'], '0.1.0')

    def test_development_has_no_update_source(self):
        info = metadata.bundle_info('0.1.0', False, '')
        self.assertFalse(info['RWSUpdatesEnabled'])
        self.assertNotIn('SUFeedURL', info)
        self.assertNotIn('SUPublicEDKey', info)

    def test_release_requires_ed25519_public_key(self):
        for key in ('', 'placeholder', 'a' * 44):
            with self.assertRaises(ValueError):
                metadata.bundle_info('0.1.0', True, key)
        info = metadata.bundle_info('0.1.0', True, 'AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=')
        self.assertTrue(info['RWSUpdatesEnabled'])
        self.assertTrue(info['SUVerifyUpdateBeforeExtraction'])
        self.assertFalse(info['SUAllowsAutomaticUpdates'])
        self.assertFalse(info['SUAutomaticallyUpdate'])
        self.assertEqual(info['CFBundleVersion'], '0.1.0')

    def test_rejects_prerelease_and_mismatched_tag(self):
        for tag in ('v0.2.0', '0.1.0', 'v0.1.0-beta', 'v0.1.0\n'):
            with self.assertRaises(ValueError):
                metadata.validate_tag(tag, '0.1.0')
        metadata.validate_tag('v0.1.0', '0.1.0')
        with self.assertRaises(ValueError):
            metadata.bundle_info('0.1.0-beta', False, '')

if __name__ == '__main__':
    unittest.main()
