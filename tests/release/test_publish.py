import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts'))
spec = importlib.util.spec_from_file_location('publish_release', ROOT / 'scripts/publish-release.py')
publish = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publish)

class PublishingTests(unittest.TestCase):
    @patch.object(publish.subprocess, 'run')
    def test_existing_public_release_is_never_mutated(self, run):
        run.return_value = subprocess.CompletedProcess([], 0, '{"isDraft": false}')
        with self.assertRaisesRegex(RuntimeError, 'already published'):
            publish.publish('v' + publish.cargo_version(), ['archive.zip'])
        self.assertEqual(run.call_count, 1)

    @patch.object(publish.subprocess, 'run')
    def test_existing_draft_resumes_without_recreation(self, run):
        run.return_value = subprocess.CompletedProcess([], 0, '{"isDraft": true}')
        publish.publish('v' + publish.cargo_version(), ['archive.zip'])
        self.assertEqual([c.args[0][2] for c in run.call_args_list], ['view', 'upload', 'edit'])

    @patch.object(publish.subprocess, 'run')
    def test_failed_upload_never_publishes(self, run):
        run.side_effect = [subprocess.CompletedProcess([], 0, '{"isDraft": true}'),
                           subprocess.CalledProcessError(1, ['gh', 'release', 'upload'])]
        with self.assertRaises(subprocess.CalledProcessError):
            publish.publish('v' + publish.cargo_version(), ['archive.zip'])
        self.assertEqual(run.call_count, 2)
