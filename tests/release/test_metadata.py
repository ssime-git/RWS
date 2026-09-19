import importlib.util
from pathlib import Path
import unittest

MODULE = Path(__file__).resolve().parents[2] / 'scripts/macos_metadata.py'
spec = importlib.util.spec_from_file_location('metadata', MODULE)
metadata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(metadata)

class MetadataTests(unittest.TestCase):
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
