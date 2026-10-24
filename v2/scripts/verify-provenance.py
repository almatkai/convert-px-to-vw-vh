#!/usr/bin/env python3
"""Verify every pinned fixture and asset. --remote rechecks pinned upstream bytes."""
import hashlib, json, pathlib, sys, urllib.request
root = pathlib.Path(__file__).resolve().parents[1]
records = json.loads((root / 'provenance.json').read_text())
for record in records:
    content = (root / record['local']).read_bytes()
    assert hashlib.sha256(content).hexdigest() == record['sha256'], record['local']
    if '--remote' in sys.argv and record['origin'] == 'upstream':
        remote = 'https://raw.githubusercontent.com/raycast/extensions/' + record['commit'] + '/' + record['path']
        assert urllib.request.urlopen(remote).read() == content, remote
assert (root / 'icons/logo.png').stat().st_size == 182084
assert (root / 'LICENSE-MIT').read_bytes() == (root / 'tests/fixtures/upstream/LICENSE').read_bytes()
assert (root / 'LICENSE-APACHE-2.0').read_bytes() == (root / 'tests/fixtures/beta/LICENSE').read_bytes()
print(f'Verified {len(records)} pinned files, three untouched PNGs and both packaged licenses')
