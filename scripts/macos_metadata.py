#!/usr/bin/env python3
"""Version and update metadata shared by local bundles and release CI."""
import argparse
import base64
from datetime import datetime, timezone
from pathlib import Path
import plistlib
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent
FEED = 'https://github.com/ssime-git/RWS/releases/latest/download/appcast.xml'


def validate_version(version):
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', version):
        raise ValueError('Only stable numeric major.minor.patch versions are supported')


def cargo_version():
    # Cargo.toml keeps the package version explicit; fail on ambiguous input.
    package = (ROOT / 'Cargo.toml').read_text().split('[package]', 1)[1].split('\n[', 1)[0]
    matches = re.findall(r'^version\s*=\s*"([^"]+)"\s*$', package, re.MULTILINE)
    if len(matches) != 1:
        raise ValueError('Expected one explicit Cargo package version')
    validate_version(matches[0])
    return matches[0]


def validate_tag(tag, version):
    validate_version(version)
    if tag != 'v' + version:
        raise ValueError('Release tag must exactly match v' + version)


def git_revision():
    try:
        revision = subprocess.check_output(['git', 'rev-parse', '--short=12', 'HEAD'], cwd=ROOT, text=True).strip()
        dirty = subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=ROOT, text=True)
        return revision + ('-dirty' if dirty else '')
    except (OSError, subprocess.CalledProcessError):
        return 'unknown'


def bundle_info(version, production, public_key, revision='unknown'):
    validate_version(version)
    info = {
        'CFBundleIdentifier': 'io.github.ssime-git.RWS',
        'CFBundleName': 'RWS', 'CFBundleDisplayName': 'RWS',
        'CFBundleExecutable': 'RWSApp', 'CFBundlePackageType': 'APPL',
        'CFBundleVersion': version, 'CFBundleShortVersionString': version,
        'LSMinimumSystemVersion': '13.0', 'NSPrincipalClass': 'NSApplication',
        'NSHighResolutionCapable': True, 'RWSUpdatesEnabled': production,
        'RWSBuildRevision': revision,
        'RWSBuildTimestamp': datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
    }
    if production:
        try:
            decoded = base64.b64decode(public_key, validate=True)
        except ValueError as error:
            raise ValueError('A valid Sparkle Ed25519 public key is required') from error
        if len(decoded) != 32 or decoded == bytes(32):
            raise ValueError('A 32-byte Sparkle Ed25519 public key is required')
        info.update(SUFeedURL=FEED, SUPublicEDKey=public_key,
                    SUEnableAutomaticChecks=True, SUAutomaticallyUpdate=False,
                    SUAllowsAutomaticUpdates=False,
                    SUVerifyUpdateBeforeExtraction=True)
    return info


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--production', action='store_true')
    parser.add_argument('--public-key', default='')
    parser.add_argument('--tag')
    args = parser.parse_args()
    version = cargo_version()
    if args.tag is not None:
        validate_tag(args.tag, version)
    if args.output:
        args.output.write_bytes(plistlib.dumps(bundle_info(version, args.production, args.public_key, git_revision())))
    else:
        print(version)
