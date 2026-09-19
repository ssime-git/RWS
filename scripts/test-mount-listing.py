#!/usr/bin/env python3
"""Live regression test. Creates a unique directory in an authorized mounted root.

Usage: python3 scripts/test-mount-listing.py /Volumes/RWS-test
Retains the test directory on failure for diagnosis; removes it on success.
"""
import os
from pathlib import Path
import sys
import tempfile
import unicodedata

root = Path(sys.argv[1]).resolve()
if not os.path.ismount(root):
    raise SystemExit("Expected a mounted filesystem root")
test = Path(tempfile.mkdtemp(prefix="rws-listing-", dir=root))
print(f"Test directory: {test}", flush=True)
for iteration in range(5):
    assert test.name in os.listdir(root), f"Root listing lost directory (read {iteration})"
    file = test / "été 日本 😀.txt"
    file.write_text("RWS listing test\n")
    expected = unicodedata.normalize("NFC", file.name)
    for repeat in range(3):
        names = {unicodedata.normalize("NFC", n) for n in os.listdir(test)}
        assert expected in names, f"Child listing lost file (read {repeat})"
    assert Path(unicodedata.normalize("NFD", str(file))).read_text() == "RWS listing test\n"
    file.unlink()
    assert not file.exists()
    assert expected not in {unicodedata.normalize("NFC", n) for n in os.listdir(test)}
# macFUSE may create AppleDouble sidecars for the files made by this test.
for sidecar in test.iterdir():
    if sidecar.name.startswith("._"):
        sidecar.unlink()
test.rmdir()
for iteration in range(3):
    assert test.name not in os.listdir(root), "Root listing retained removed directory"
print("PASS: repeated root/child listing, Unicode create/read/delete, root invalidation")
