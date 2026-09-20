#!/usr/bin/env python3
"""Publish a completed staged bundle, preserving the previous build as a ZIP."""
import argparse
from datetime import datetime, timezone
import fcntl
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid


def ensure_not_running(app):
    # comm includes the executable path, without arguments that could match by accident.
    processes = subprocess.check_output(['ps', '-axo', 'pid=,comm='], text=True)
    prefix = str(app.resolve()) + '/'
    for line in processes.splitlines():
        fields = line.strip().split(None, 1)
        if len(fields) == 2 and str(Path(fields[1]).resolve()).startswith(prefix):
            raise RuntimeError(f'Quit RWS before rebuilding: {app} is running (PID {fields[0]}).')


def archive_bundle(app, archive):
    subprocess.run(['ditto', '-c', '-k', '--sequesterRsrc', '--keepParent',
                    str(app), str(archive)], check=True)


def publish(stage, output):
    stage, output = Path(stage).absolute(), Path(output).absolute()
    if (output.name not in ('development', 'production') or output.is_symlink()
            or stage.is_symlink() or (stage / 'RWS.app').is_symlink()
            or stage.parent != output.parent
            or not stage.name.startswith('.build-' + output.name + '-')
            or not (stage / 'RWS.app/Contents/MacOS/RWSApp').is_file()):
        raise ValueError('Expected a staged RWS bundle beside the canonical build directory')
    with (output.parent / '.publish.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        ensure_not_running(output / 'RWS.app')
        backup = None
        if output.exists():
            if (not output.is_dir() or (output / 'RWS.app').is_symlink()
                    or not (output / 'RWS.app/Contents/MacOS/RWSApp').is_file()
                    or any(p.name != 'RWS.app' and not (p.name.startswith('RWS-') and p.suffix == '.zip' and p.is_file())
                           for p in output.iterdir())):
                raise ValueError('Refusing to replace an unrecognized build directory')
            archives = output.parent / 'archives'
            if archives.is_symlink():
                raise ValueError('Refusing a symlinked archive directory')
            archives.mkdir(exist_ok=True)
            stamp = datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
            archive = archives / f'RWS-{output.name}-{stamp}-{uuid.uuid4().hex[:8]}.zip'
            try:
                archive_bundle(output / 'RWS.app', archive)
            except BaseException:
                archive.unlink(missing_ok=True)
                raise
            ensure_not_running(output / 'RWS.app')
            backup = Path(tempfile.mkdtemp(prefix='.previous-', dir=output.parent))
            backup.rmdir()
            output.rename(backup)
        try:
            if backup is not None:
                # Even a retained recovery directory must not contain a launchable app.
                (backup / 'RWS.app').rename(backup / 'RWS.previous-bundle')
            stage.rename(output)
        except BaseException:
            if backup is not None:
                if (backup / 'RWS.previous-bundle').exists():
                    (backup / 'RWS.previous-bundle').rename(backup / 'RWS.app')
                backup.rename(output)
            raise
        if backup is not None:
            shutil.rmtree(backup)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check-running', type=Path)
    parser.add_argument('--stage', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    try:
        if args.check_running:
            ensure_not_running(args.check_running)
        elif args.stage and args.output:
            publish(args.stage, args.output)
        else:
            parser.error('Provide --check-running, or --stage and --output')
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'{error}\n')
