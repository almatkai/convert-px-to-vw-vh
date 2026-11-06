#!/usr/bin/env python3
"""Enforce declared MSRV for locked normal/build closures of both releases.

The native Rust 1.85 check complements this target-aware metadata check; it does
not claim a Windows compilation on a Mac without the Windows standard library.
"""
import importlib.util
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('notices', ROOT / 'scripts/vendor-licenses.py')
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)
msrv = re.search(r'^rust-version = "([^"]+)"', (ROOT / 'Cargo.toml').read_text(), re.M).group(1)
def version(value):
    return tuple((list(map(int, value.split('.'))) + [0, 0])[:3])
for item in notices.inventory():
    package = item['package']
    required = package.get('rust_version')
    assert required is None or version(required) <= version(msrv), (
        f"{package['name']} {package['version']} requires Rust {required} > {msrv}: {item['targets']}")
print(f'Locked normal/build dependencies for both targets support declared Rust {msrv}')
