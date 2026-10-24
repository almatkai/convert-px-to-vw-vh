#!/usr/bin/env python3
"""Full draft 2020-12 validation using jsonschema, plus package metadata checks."""
import json, pathlib
from jsonschema import Draft202012Validator
root = pathlib.Path(__file__).resolve().parents[1]
schema = json.loads((root / 'tests/fixtures/beta/extension-v2.schema.json').read_text())
manifest = json.loads((root / 'manifest.json').read_text())
Draft202012Validator.check_schema(schema)
Draft202012Validator(schema).validate(manifest)
original = json.loads((root / 'tests/fixtures/upstream/package.json').read_text())
assert original['author'] == 'Atiipik'
assert 'Atiipik' in manifest['description'] and 'author' not in manifest
assert manifest['id'] == 'com.tezbar.native.convert-px-to-vw-vh'
assert manifest['replaces'] == ['raycast.convert-px-to-vw-vh']
assert manifest['icon'] == '/extensions/icons/convert-px-to-vw-vh.png'
assert [c['name'] for c in manifest['commands']] == [c['name'] for c in original['commands']]
for native, source in zip(manifest['commands'], original['commands']):
    for key in ['name', 'title', 'arguments']:
        assert native.get(key) == source.get(key), key
    assert native['runtime'] == 'rust'
    assert native['platforms'] == ['macOS', 'Windows']
assert all(v['state'] == 'not-run' for v in manifest['verification'])
print('Full pinned beta schema, canonical identity, command arguments, platforms and credits passed')
