"""Use the public RFC 8032 test vector, never a release private key."""
import base64
import os
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]

@unittest.skipUnless(sys.platform == 'darwin', 'CryptoKit validation is macOS-only')
class SparkleKeyTests(unittest.TestCase):
    def test_matching_and_mismatched_pairs(self):
        env = dict(os.environ)
        env['RWS_SPARKLE_PRIVATE_KEY'] = base64.b64encode(bytes.fromhex(
            '9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60')).decode()
        env['RWS_SPARKLE_PUBLIC_KEY'] = base64.b64encode(bytes.fromhex(
            'd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a')).decode()
        args = ['swift', '-module-cache-path', '/tmp/rws-release-swift-cache', str(ROOT / 'scripts/verify-sparkle-key.swift')]
        good = subprocess.run(args, env=env, capture_output=True, text=True)
        self.assertEqual(good.returncode, 0, good.stderr)
        env['RWS_SPARKLE_PUBLIC_KEY'] = base64.b64encode(bytes([1])*32).decode()
        bad = subprocess.run(args, env=env, capture_output=True, text=True)
        self.assertNotEqual(bad.returncode, 0)
        self.assertIn('mismatch', bad.stderr)
        self.assertNotIn(env['RWS_SPARKLE_PRIVATE_KEY'], bad.stdout + bad.stderr)
