#!/usr/bin/env python3
"""Sign and notarize a production bundle in an ephemeral CI keychain."""
import base64
import os
from pathlib import Path
import plistlib
import secrets
import subprocess
import tempfile
from macos_metadata import cargo_version


def run(*args, **kwargs):
    # Do not echo argv: signing/notarization arguments can contain CI secrets.
    subprocess.run(args, check=True, **kwargs)


def main():
    names = ('RWS_CERTIFICATE_P12', 'RWS_CERTIFICATE_PASSWORD', 'RWS_SIGNING_IDENTITY',
             'RWS_APPLE_ID', 'RWS_APPLE_TEAM_ID', 'RWS_APPLE_APP_PASSWORD')
    missing = [name for name in names if not os.environ.get(name)]
    if missing:
        raise SystemExit('Missing release secrets: ' + ', '.join(missing))
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise SystemExit('Signing script is restricted to ephemeral GitHub Actions runners')
    app = Path('dist/production/RWS.app').resolve()
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    if not info.get('RWSUpdatesEnabled'):
        raise SystemExit('Refusing to publish a development bundle')
    with tempfile.TemporaryDirectory(prefix='rws-sign-') as directory:
        folder = Path(directory)
        certificate = folder / 'certificate.p12'
        certificate.write_bytes(base64.b64decode(os.environ['RWS_CERTIFICATE_P12'], validate=True))
        certificate.chmod(0o600)
        keychain = str(folder / 'release.keychain-db')
        password = secrets.token_urlsafe(32)
        run('security', 'create-keychain', '-p', password, keychain)
        try:
            run('security', 'set-keychain-settings', '-lut', '21600', keychain)
            run('security', 'unlock-keychain', '-p', password, keychain)
            run('security', 'import', str(certificate), '-k', keychain,
                '-P', os.environ['RWS_CERTIFICATE_PASSWORD'], '-T', '/usr/bin/codesign')
            run('security', 'set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:',
                '-s', '-k', password, keychain, stdout=subprocess.DEVNULL)
            def sign(path):
                run('codesign', '--force', '--timestamp', '--options', 'runtime',
                    '--keychain', keychain, '--sign', os.environ['RWS_SIGNING_IDENTITY'], str(path))
            # Sign real Mach-O leaves first; frameworks use symlinks, never follow them twice.
            for path in sorted(app.rglob('*')):
                if path.is_file() and not path.is_symlink():
                    kind = subprocess.check_output(['file', '-b', str(path)], text=True)
                    if 'Mach-O' in kind:
                        sign(path)
            bundles = [p for p in app.rglob('*') if not p.is_symlink()
                       and p.is_dir() and p.suffix in ('.app', '.xpc', '.framework')]
            for path in sorted(bundles, key=lambda p: len(p.parts), reverse=True):
                sign(path)
            sign(app)
            run('codesign', '--verify', '--deep', '--strict', '--verbose=2', str(app))
            archive = folder / 'notarize.zip'
            run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app), str(archive))
            run('xcrun', 'notarytool', 'submit', str(archive), '--wait', '--timeout', '30m',
                '--apple-id', os.environ['RWS_APPLE_ID'], '--team-id', os.environ['RWS_APPLE_TEAM_ID'],
                '--password', os.environ['RWS_APPLE_APP_PASSWORD'])
            run('xcrun', 'stapler', 'staple', str(app))
            run('xcrun', 'stapler', 'validate', str(app))
            run('spctl', '--assess', '--type', 'execute', '--verbose=2', str(app))
            publish = Path('dist/publish')
            publish.mkdir(exist_ok=False)
            run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app),
                str(publish / ('RWS-' + cargo_version() + '-arm64.zip')))
        finally:
            subprocess.run(['security', 'delete-keychain', keychain], check=False)


if __name__ == '__main__':
    try:
        main()
    except subprocess.CalledProcessError as error:
        # CalledProcessError's default string would expose argv containing secrets.
        raise SystemExit('Signing/notarization command failed (exit %s); no release published.' % error.returncode)
