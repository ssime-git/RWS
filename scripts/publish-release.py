#!/usr/bin/env python3
"""Resume upload to a release draft; never overwrite a published version."""
import json
import os
from pathlib import Path
import subprocess
from macos_metadata import cargo_version, validate_tag


def publish(tag, assets):
    validate_tag(tag, cargo_version())
    found = subprocess.run(['gh', 'release', 'view', tag, '--json', 'isDraft'],
                           capture_output=True, text=True)
    if found.returncode == 0:
        if not json.loads(found.stdout)['isDraft']:
            raise RuntimeError('Release already published; refusing to replace signed assets')
    else:
        # If view failed because of network/permissions rather than absence, this
        # creation will fail safely; no existing release is modified.
        subprocess.run(['gh', 'release', 'create', tag, '--verify-tag', '--generate-notes',
                        '--draft', '--title', 'RWS ' + tag], check=True)
    subprocess.run(['gh', 'release', 'upload', tag, *map(str, assets), '--clobber'], check=True)
    subprocess.run(['gh', 'release', 'edit', tag, '--draft=false'], check=True)


if __name__ == '__main__':
    version = cargo_version()
    root = Path('dist/publish')
    assets = [root / ('RWS-' + version + '-arm64.zip'), root / 'appcast.xml', root / 'SHA256SUMS']
    if not all(p.is_file() and p.stat().st_size > 0 for p in assets):
        raise SystemExit('Release assets missing or empty')
    publish(os.environ['RELEASE_TAG'], assets)
