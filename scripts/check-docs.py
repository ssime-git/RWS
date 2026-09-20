#!/usr/bin/env python3
"""Check repository Markdown file links and heading anchors; no network access."""
from pathlib import Path
import re
import subprocess
import sys
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parent.parent

def without_fences(text):
    return re.sub(r'^(`{3,}|~{3,}).*?^\1[^\n]*$', '', text, flags=re.M | re.S)


def anchors(path):
    text = without_fences(path.read_text())
    found = set(re.findall(r'<(?:a|h[1-6])\b[^>]*\bid=["\']([^"\']+)', text))
    counts = {}
    for title in re.findall(r'^#{1,6}\s+(.+?)\s*#*$', text, flags=re.M):
        title = re.sub(r'\[([^\]]+)\]\([^)]*\)', r'\1', title)
        title = re.sub(r'<[^>]*>', '', title).lower()
        slug = ''.join(c for c in title if c.isalnum() or c in '_- ' ).replace(' ', '-')
        count = counts.get(slug, 0)
        counts[slug] = count + 1
        found.add(f'{slug}-{count}' if count else slug)
    return found


def main():
    names = subprocess.check_output(
        ['git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z'], cwd=ROOT
    ).decode().split('\0')
    files = sorted({ROOT / name for name in names if name.endswith('.md')})
    errors = []
    checked = 0
    for path in files:
        if not path.exists():
            continue
        content = without_fences(path.read_text())
        # Inline links and images; reference definitions are checked separately.
        links = re.findall(r'!?\[[^\]\n]*\]\(<?([^\s)>]+)>?(?:\s+"[^"]*")?\)', content)
        links += re.findall(r'^\s*\[[^\]]+\]:\s*<?([^\s>]+)>?', content, flags=re.M)
        for link in links:
            parts = urlsplit(link)
            if parts.scheme or parts.netloc:
                continue
            target = (path.parent / unquote(parts.path)).resolve() if parts.path else path
            checked += 1
            if not target.exists():
                errors.append(f'{path.relative_to(ROOT)}: missing target {link}')
            elif parts.fragment and target.suffix == '.md':
                if unquote(parts.fragment) not in anchors(target):
                    errors.append(f'{path.relative_to(ROOT)}: missing anchor {link}')
    if errors:
        print('\n'.join(errors), file=sys.stderr)
        return 1
    print(f'Checked {checked} local Markdown links across {len(files)} files.')
    print('External URLs and Mermaid rendering require separate review.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
