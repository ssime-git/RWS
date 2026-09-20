import importlib.util
from pathlib import Path
import subprocess
import sys
import shutil
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('local_build', ROOT / 'scripts/publish-macos-build.py')


class LocalBuildTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(Path(spec.origin).exists(), 'Staged build publisher is missing')
        self.publisher = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.publisher)
        if sys.platform != 'darwin':
            # ditto preserves macOS bundle metadata; other CI hosts exercise publication
            # with a portable ZIP implementation because ditto is macOS-only.
            def portable_archive(app, archive):
                shutil.make_archive(str(archive.with_suffix('')), 'zip', app.parent, app.name)
            archive_patch = patch.object(self.publisher, 'archive_bundle', portable_archive)
            archive_patch.start()
            self.addCleanup(archive_patch.stop)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.dist = Path(self.temp.name)
        self.output = self.dist / 'development'
        self.stage = self.dist / '.build-development-test'
        self.bundle(self.stage, 'new')

    def bundle(self, directory, content):
        executable = directory / 'RWS.app/Contents/MacOS/RWSApp'
        executable.parent.mkdir(parents=True)
        executable.write_text(content)
        return executable

    def publish(self):
        with patch.object(self.publisher, 'ensure_not_running'):
            self.publisher.publish(self.stage, self.output)

    def test_first_build_and_repeated_replacement(self):
        self.publish()
        self.bundle(self.stage, 'newer')
        self.publish()
        self.assertEqual((self.output / 'RWS.app/Contents/MacOS/RWSApp').read_text(), 'newer')
        archives = list((self.dist / 'archives').glob('*.zip'))
        self.assertEqual(len(archives), 1)
        with zipfile.ZipFile(archives[0]) as archive:
            self.assertEqual(archive.read('RWS.app/Contents/MacOS/RWSApp'), b'new')
        self.assertEqual(list(self.dist.rglob('*.app')), [self.output / 'RWS.app'])

    def test_failed_install_rolls_back(self):
        self.bundle(self.output, 'old')
        rename = Path.rename
        def fail_install(source, target):
            if source == self.stage:
                raise OSError('install failed')
            return rename(source, target)
        with patch.object(Path, 'rename', fail_install):
            with self.assertRaisesRegex(OSError, 'install failed'):
                self.publish()
        self.assertEqual((self.output / 'RWS.app/Contents/MacOS/RWSApp').read_text(), 'old')

    def test_archive_failure_leaves_old_bundle_intact(self):
        self.bundle(self.output, 'old')
        with patch.object(self.publisher, 'archive_bundle', side_effect=OSError('archive failed')):
            with self.assertRaisesRegex(OSError, 'archive failed'):
                self.publish()
        self.assertEqual((self.output / 'RWS.app/Contents/MacOS/RWSApp').read_text(), 'old')

    def test_running_bundle_is_preserved(self):
        executable = self.bundle(self.output, 'old')
        with patch.object(self.publisher.subprocess, 'check_output', return_value=f'123 {executable}\n'):
            with self.assertRaisesRegex(RuntimeError, 'Quit RWS'):
                self.publisher.publish(self.stage, self.output)
        self.assertEqual(executable.read_text(), 'old')

    def test_refuses_symlink_destination_and_unexpected_files(self):
        elsewhere = self.dist / 'elsewhere'
        elsewhere.mkdir()
        self.output.symlink_to(elsewhere, target_is_directory=True)
        with self.assertRaises(ValueError):
            self.publish()
        self.output.unlink()
        self.bundle(self.output, 'old')
        (self.output / 'personal.txt').write_text('keep')
        with self.assertRaises(ValueError):
            self.publish()
        self.assertEqual((self.output / 'personal.txt').read_text(), 'keep')

    def test_app_started_during_archiving_prevents_replacement(self):
        executable = self.bundle(self.output, 'old')
        with patch.object(self.publisher, 'ensure_not_running', side_effect=[None, RuntimeError('Quit RWS')]):
            with self.assertRaisesRegex(RuntimeError, 'Quit RWS'):
                self.publisher.publish(self.stage, self.output)
        self.assertEqual(executable.read_text(), 'old')

    def test_refuses_symlinked_staged_app(self):
        shutil.rmtree(self.stage / 'RWS.app')
        self.bundle(self.output, 'old')
        (self.stage / 'RWS.app').symlink_to(self.output / 'RWS.app', target_is_directory=True)
        with self.assertRaises(ValueError):
            self.publish()

    def test_process_inspection_failure_fails_closed(self):
        with patch.object(self.publisher.subprocess, 'check_output', side_effect=subprocess.CalledProcessError(1, ['ps'])):
            with self.assertRaises(subprocess.CalledProcessError):
                self.publisher.ensure_not_running(self.output / 'RWS.app')
