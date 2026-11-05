#!/usr/bin/env python3
"""Verify a package ZIP offline and exercise its extracted executable.

Checks the closed SHA inventory, safe paths, executable/manifest identity, then
runs hello/initialize/session.open(view)/dispose/shutdown over JSONL with a
fake host that refuses every reverse request. With --protocol-tests, the cargo
real-pipe protocol suite (session/action flows answered by a recording stub
host) is also run against the *extracted* binary. Never installs anything.
"""
import argparse, hashlib, json, os, queue, stat, subprocess, tempfile, threading, uuid, zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
MAX = 16 * 1024 * 1024


def smoke(path, command=None, protocol_tests=False, expected_sha=None):
    data = path.read_bytes()
    if expected_sha:
        assert hashlib.sha256(data).hexdigest() == expected_sha, 'Archive SHA256 mismatch'
    with tempfile.TemporaryDirectory() as temp, zipfile.ZipFile(path) as archive:
        root = Path(temp) / 'package'
        root.mkdir()
        names = archive.namelist()
        assert len(names) == len(set(names)), 'Duplicate ZIP paths'
        assert len({n.casefold() for n in names}) == len(names), 'Case-folded duplicate ZIP paths'
        for item in archive.infolist():
            name = PurePosixPath(item.filename)
            assert not name.is_absolute() and '..' not in name.parts and '\\' not in item.filename and ':' not in item.filename, 'Unsafe ZIP path'
            assert not stat.S_ISLNK(item.external_attr >> 16) and not item.is_dir(), 'Non-regular ZIP entry'
        manifest = json.loads(archive.read('extension.json'))
        inventory = manifest['files']
        assert manifest['schemaVersion'] == 3 and manifest['execution'] == 'process' and manifest['protocolVersion'] == 1
        assert manifest['id'] == 'com.tezbar.native.' + manifest['slug']
        assert set(names) == set(inventory) | {'extension.json'}, 'ZIP/inventory mismatch'
        for name, sha in inventory.items():
            assert hashlib.sha256(archive.read(name)).hexdigest() == sha, 'SHA inventory mismatch: ' + name
        for name in ['LICENSE', 'NOTICE']:
            assert name in inventory, 'Missing ' + name
        archive.extractall(root)
        slug = manifest['slug']
        binary = root / 'bin' / ('tezbar-' + slug + ('.exe' if os.name == 'nt' else ''))
        assert binary.relative_to(root).as_posix() in inventory, 'Executable not inventoried'
        binary.chmod(0o755)
        env = {k: v for k, v in os.environ.items() if k in ('SystemRoot', 'WINDIR', 'TEMP', 'TMP')}
        env['PATH'] = os.path.join(env.get('SystemRoot', 'C:/Windows'), 'System32') if os.name == 'nt' else '/usr/bin:/bin'
        info = json.loads(subprocess.check_output([str(binary), '--protocol-info'], cwd=root, env=env, timeout=10))
        assert info['id'] == manifest['id'] and info['version'] == manifest['version'] and info['protocol'] == 1
        platform = 'Windows' if os.name == 'nt' else 'macOS'
        declared = [c['name'] for c in manifest['commands'] if c['runtime'] == 'rust' and platform in c['platforms']]
        assert sorted(info['commands']) == sorted(declared), 'Executable/manifest command mismatch'
        for icon in [manifest.get('icon')] + [c.get('icon') for c in manifest['commands']]:
            assert icon is None or icon in inventory, f'Icon not packaged: {icon}'
        view_commands = [c['name'] for c in manifest['commands'] if c.get('mode') == 'view' and c['name'] in declared]
        command = command or view_commands[0]
        process = subprocess.Popen([str(binary)], cwd=root, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None)
        messages = queue.Queue()

        def reader():
            try:
                while True:
                    line = process.stdout.readline(MAX + 1)
                    if not line:
                        messages.put(EOFError('Unexpected EOF'))
                        return
                    if len(line) > MAX:
                        raise ValueError('Oversized frame')
                    messages.put(json.loads(line))
            except Exception as error:
                messages.put(error)

        threading.Thread(target=reader, daemon=True).start()

        def receive():
            value = messages.get(timeout=8)
            if isinstance(value, Exception):
                raise value
            return value

        def request(method, params):
            ident = 'h:' + str(uuid.uuid4())
            process.stdin.write((json.dumps({'type': 'request', 'id': ident, 'method': method, 'params': params}) + '\n').encode())
            process.stdin.flush()
            while True:
                frame = receive()
                if frame['type'] == 'response' and frame['id'] == ident:
                    assert 'error' not in frame, frame
                    return frame['result']
                assert frame['type'] == 'event', 'View open must not request host effects: ' + str(frame)

        try:
            hello = receive()
            assert hello['type'] == 'hello' and hello['id'] == manifest['id'] and hello['version'] == manifest['version'] and hello['protocol'] == 1
            ready = request('initialize', {'protocol': 1, 'extensionId': manifest['id'], 'platform': platform, 'packageDir': str(root), 'dataDir': str(Path(temp) / 'data'), 'tempDir': str(Path(temp) / 'temp'), 'grantedCapabilities': manifest['capabilities'], 'reason': 'command', 'migrationPaths': {}})
            assert ready['ready'] is True
            view = request('session.open', {'sessionId': 'zip-smoke', 'commandName': command, 'preferences': {}, 'argumentValues': {}})
            assert view['mode'] == 'view' and view['root']['type'], view
            request('session.dispose', {'sessionId': 'zip-smoke'})
            request('shutdown', {})
            assert process.wait(timeout=5) == 0, 'Process did not exit cleanly'
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)
            process.stdin.close()
            process.stdout.close()
        print(slug, f'ZIP inventory + protocol-info/hello/initialize/open({command})/dispose/shutdown passed (offline, no installation)')
        if protocol_tests:
            run_env = dict(os.environ, TEZBAR_EXTENSION_BINARY=str(binary))
            subprocess.run(['cargo', 'test', '--locked', '-j2', '--test', 'protocol'], cwd=ROOT, env=run_env, check=True)
            print(slug, 'protocol session/action suite passed against the extracted executable')


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('archive', type=Path)
    p.add_argument('--command', help='view command to open (default: first declared view command)')
    p.add_argument('--sha256', help='expected archive SHA256')
    p.add_argument('--protocol-tests', action='store_true', help='also run tests/protocol.rs against the extracted binary')
    a = p.parse_args()
    smoke(a.archive, a.command, a.protocol_tests, a.sha256)
