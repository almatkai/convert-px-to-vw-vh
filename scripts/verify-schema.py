#!/usr/bin/env python3
"""Validate source/package manifests using the current mirrored host schema."""
import argparse
import json
from pathlib import Path
import zipfile
import jsonschema

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('archive', nargs='?', type=Path)
parser.add_argument('--host', type=Path, help='also check schema mirror against this host checkout')
args = parser.parse_args()
schema_path = ROOT / 'schemas/extension-v3.schema.json'
schema = json.loads(schema_path.read_text())
validator_type = jsonschema.validators.validator_for(schema)
validator_type.check_schema(schema)
validator = validator_type(schema)
validator.validate(json.loads((ROOT / 'extension.json').read_text()))
if args.host:
    assert schema_path.read_bytes() == (args.host / 'schemas/extension-v3.schema.json').read_bytes(), 'Schema mirror differs from current host'
if args.archive:
    with zipfile.ZipFile(args.archive) as archive:
        validator.validate(json.loads(archive.read('extension.json')))
print('Source' + (' and packaged' if args.archive else '') + ' manifest schema validation passed')
