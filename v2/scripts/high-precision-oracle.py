#!/usr/bin/env python3
"""Independently check JS toFixed oracle using exact Decimal.from_float arithmetic."""
import decimal, json, pathlib, struct
root = pathlib.Path(__file__).resolve().parents[1]
rows = json.loads((root / 'tests/fixtures/fixed-oracle.json').read_text())
checked = 0
with decimal.localcontext() as ctx:
    ctx.prec = 1200
    for row in rows:
        x = struct.unpack('>d', bytes.fromhex(row['bits']))[0]
        if not (-1e21 < x < 1e21):
            continue
        # JS tests x < 0 (negative zero itself is unsigned).
        sign = '-' if x < 0 else ''
        expected = sign + format(decimal.Decimal.from_float(abs(x)).quantize(decimal.Decimal('0.001'), rounding=decimal.ROUND_HALF_UP), 'f')
        assert expected == row['fixed'], (row, expected)
        checked += 1
print(f'Exact high-precision Decimal agrees for {checked} finite toFixed cases')
