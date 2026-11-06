#!/usr/bin/env python3
"""Build the root-layout package ZIP plus SHA256 sidecars. Never uploads.

Layout: extension.json, bin/tezbar-<slug>[.exe], resources/**, licenses/**,
LICENSE, NOTICE. `files` inventories every member except extension.json.
"""
import argparse, hashlib, json, re, shutil, stat, subprocess, tempfile, zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {'aarch64-apple-darwin': ('macOS', False), 'x86_64-pc-windows-msvc': ('Windows', True)}
PAYLOAD_DIRS = ['resources', 'licenses']
PAYLOAD_FILES = ['LICENSE', 'NOTICE']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_sdk():
    text = (ROOT / 'Cargo.toml').read_text()
    match = re.search(r'^tezbar-extension-sdk\s*=\s*\{([^}\n]*)\}', text, re.M)
    fields = match.group(1) if match else ''
    revision = re.search(r'\brev\s*=\s*"([0-9a-f]{40})"', fields)
    if not revision or re.search(r'\b(path|branch|tag)\s*=', fields) or not re.search(r'\bgit\s*=\s*"https://github.com/[^" ]+"', fields):
        raise SystemExit('Release blocked: the SDK must be pinned to an immutable git revision.')
    lock = (ROOT / 'Cargo.lock').read_text()
    entry = re.search(r'\[\[package\]\]\nname = "tezbar-extension-sdk"(.*?)(?=\[\[package\]\]|\Z)', lock, re.S)
    if not entry or not re.search(r'source = "git\+[^"\n]+#' + revision.group(1) + r'"', entry.group(1)):
        raise SystemExit('Release blocked: Cargo.lock must contain the same immutable SDK git revision.')


def build(target, binary, output):
    check_sdk()
    subprocess.run(["python3", str(ROOT / "scripts/vendor-licenses.py"), "--check"], cwd=ROOT, check=True)
    subprocess.run(["python3", str(ROOT / "scripts/check-msrv.py")], cwd=ROOT, check=True)
    platform, windows = TARGETS[target]
    manifest = json.loads((ROOT / 'extension.json').read_text())
    slug = manifest['slug']
    assert manifest['id'] == f'com.tezbar.native.{slug}', 'Canonical id mismatch'
    executable = f'bin/tezbar-{slug}' + ('.exe' if windows else '')
    if binary.suffix == '.exe' and not windows:
        raise SystemExit('Windows binary passed for a macOS target')
    info = json.loads(subprocess.check_output([str(binary.resolve()), '--protocol-info'], timeout=10))
    assert info['id'] == manifest['id'] and info['version'] == manifest['version'] and info['protocol'] == 1, 'Binary identity mismatch'
    expected = [c['name'] for c in manifest['commands'] if c['runtime'] == 'rust' and platform in c['platforms']]
    assert sorted(info['commands']) == sorted(expected), 'Binary/manifest command coverage mismatch'
    for icon in [manifest.get('icon')] + [c.get('icon') for c in manifest['commands']]:
        assert icon is None or (ROOT / icon).is_file(), f'Missing icon {icon}'
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temp:
        stage = Path(temp)
        (stage / 'bin').mkdir()
        shutil.copy2(binary, stage / executable)
        for name in PAYLOAD_DIRS:
            shutil.copytree(ROOT / name, stage / name)
        for name in PAYLOAD_FILES:
            shutil.copy2(ROOT / name, stage / name)
        files = {}
        for path in sorted(stage.rglob('*')):
            if path.is_symlink():
                raise ValueError('Symlinks cannot be packaged')
            if path.is_file():
                files[path.relative_to(stage).as_posix()] = digest(path)
        manifest['files'] = files
        # Published archive URL/digests belong to the trusted host catalog, not here.
        manifest['artifacts'] = {}
        (stage / 'extension.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
        archive = output / f'{slug}-{target}.zip'
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as z:
            for path in sorted(stage.rglob('*')):
                if path.is_file():
                    name = path.relative_to(stage).as_posix()
                    entry = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
                    entry.create_system = 3
                    mode = 0o755 if name == executable else 0o644
                    entry.external_attr = (stat.S_IFREG | mode) << 16
                    entry.compress_type = zipfile.ZIP_DEFLATED
                    z.writestr(entry, path.read_bytes())
        with zipfile.ZipFile(archive) as z:
            packaged = json.loads(z.read('extension.json'))
            assert set(z.namelist()) == set(files) | {'extension.json'}
            for name, sha in packaged['files'].items():
                assert hashlib.sha256(z.read(name)).hexdigest() == sha
        artifact = {'target': target, 'sha256': digest(archive), 'executable': executable, 'executableSha256': files[executable]}
        (output / f'{slug}-{target}.artifact.json').write_text(json.dumps(artifact, indent=2) + '\n')
        (output / f'{archive.name}.sha256').write_text(f"{artifact['sha256']}  {archive.name}\n")
        (output / f'{slug}-{target}.executable.sha256').write_text(f"{artifact['executableSha256']}  {executable}\n")
        (output / f'{slug}-{target}.files.json').write_text(json.dumps(files, indent=2, sort_keys=True) + '\n')
        print(archive)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--target', required=True, choices=TARGETS)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    build(args.target, args.binary, args.output)
