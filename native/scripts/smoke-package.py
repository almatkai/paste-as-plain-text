#!/usr/bin/env python3
"""Verify a packaged ZIP offline and drive its extracted executable over JSONL.

Never installs anything. Reverse host requests are answered only from the
scenario file's scripted replies (a recording fake host); any unscripted reverse
request fails the smoke, so no real clipboard/window/keyboard effect can occur.

Scenario JSON: {"sessions":[{"open":{session.open params},
  "replies":[{"method":..., "params":..., "result":...}, ...],
  "expect":{"result":...} | {"error":"message"},
  "calls":[{"operation":..., "payload":{}, "expect":...}]}]}
"""
import argparse, hashlib, json, os, queue, stat, subprocess, tempfile, threading, uuid, zipfile
from pathlib import Path, PurePosixPath

FRAME = 16 * 1024 * 1024


def smoke(path, scenario, renderer_host=None):
    with tempfile.TemporaryDirectory() as temp, zipfile.ZipFile(path) as archive:
        root = Path(temp) / 'package'; root.mkdir()
        names = archive.namelist(); assert len(names) == len(set(names)), 'Duplicate ZIP paths'
        for item in archive.infolist():
            name = PurePosixPath(item.filename)
            assert not name.is_absolute() and '..' not in name.parts and '\\' not in item.filename and ':' not in item.filename, 'Unsafe ZIP path'
            assert not stat.S_ISLNK(item.external_attr >> 16), 'Symlink ZIP entry'
        manifest = json.loads(archive.read('extension.json')); inventory = manifest['files']
        assert set(names) == set(inventory) | {'extension.json'}, 'ZIP/inventory mismatch'
        for name, digest in inventory.items():
            assert hashlib.sha256(archive.read(name)).hexdigest() == digest, 'SHA inventory mismatch: ' + name
        for required in ['LICENSE', 'NOTICE', manifest['icon']]:
            assert required in inventory, 'Missing packaged file: ' + required
        archive.extractall(root)
        slug = manifest['slug']
        binary = root / 'bin' / ('tezbar-' + slug + ('.exe' if os.name == 'nt' else ''))
        assert (archive.getinfo(binary.relative_to(root).as_posix()).external_attr >> 16) & 0o111 or os.name == 'nt', 'Executable bit missing'
        # zipfile.extractall does not restore Unix mode bits. Check the archive
        # bit above, then restore it just as the host's verified installer does.
        if os.name != 'nt': binary.chmod(0o755)
        env = {k: v for k, v in os.environ.items() if k in ('SystemRoot', 'WINDIR', 'TEMP', 'TMP')}
        env['PATH'] = os.path.join(env.get('SystemRoot', 'C:/Windows'), 'System32') if os.name == 'nt' else '/usr/bin:/bin'
        info = json.loads(subprocess.check_output([str(binary), '--protocol-info'], cwd=root, env=env, timeout=10))
        assert info['id'] == manifest['id'] and info['version'] == manifest['version'] and info['protocol'] == 1
        platform = 'Windows' if os.name == 'nt' else 'macOS'
        expected = {c['name'] for c in manifest['commands'] if c['runtime'] == 'rust' and platform in c['platforms']}
        assert set(info['commands']) == expected, 'Command inventory mismatch'
        process = subprocess.Popen([str(binary)], cwd=root, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None)
        messages = queue.Queue()
        transcript = []
        reverse = []

        def reader():
            try:
                while True:
                    line = process.stdout.readline(FRAME + 1)
                    if not line: messages.put(EOFError('Unexpected EOF')); return
                    if len(line) > FRAME: raise ValueError('Oversized frame')
                    messages.put(json.loads(line))
            except Exception as error:
                messages.put(error)
        threading.Thread(target=reader, daemon=True).start()

        def receive():
            value = messages.get(timeout=10)
            if isinstance(value, Exception): raise value
            return value

        def send(value):
            process.stdin.write((json.dumps(value) + '\n').encode()); process.stdin.flush()

        def request(method, params, replies=()):
            reverse.clear()
            ident = 'h:' + str(uuid.uuid4())
            send({'type': 'request', 'id': ident, 'method': method, 'params': params})
            replies = list(replies)
            while True:
                frame = receive()
                if frame['type'] == 'response' and frame['id'] == ident:
                    assert not replies, 'Scripted reverse replies left unused: ' + json.dumps(replies)
                    return frame
                if frame['type'] == 'request':
                    reverse.append(frame)
                    assert replies, 'Unscripted reverse request (refusing real effects): ' + json.dumps(frame)
                    step = replies.pop(0)
                    assert frame['method'] == step['method'], f"Expected {step['method']}, got {frame['method']}"
                    if 'params' in step: assert frame['params'] == step['params'], json.dumps(frame['params'])
                    if 'error' in step:
                        send({'type': 'response', 'id': frame['id'], 'error': {'code': 'host-error', 'message': step['error']}})
                    else:
                        send({'type': 'response', 'id': frame['id'], 'result': step.get('result')})
                    continue
                raise AssertionError('Unexpected frame: ' + json.dumps(frame))

        def check(frame, expect):
            if 'error' in expect:
                assert frame.get('error', {}).get('message') == expect['error'], frame
            else:
                assert 'error' not in frame and frame.get('result') == expect['result'], frame
        try:
            hello = receive()
            assert hello['type'] == 'hello' and hello['id'] == manifest['id'] and hello['version'] == manifest['version'] and hello['protocol'] == 1
            init = request('initialize', {'protocol': 1, 'extensionId': manifest['id'], 'platform': 'Windows' if os.name == 'nt' else 'macOS',
                                          'packageDir': str(root), 'dataDir': str(Path(temp) / 'data'), 'tempDir': str(Path(temp) / 'temp'),
                                          'grantedCapabilities': manifest['capabilities'], 'reason': 'command', 'migrationPaths': {}})
            assert init.get('result', {}).get('ready') is True, init
            for index, session in enumerate(scenario['sessions']):
                params = dict(session['open']); params.setdefault('sessionId', f'zip-smoke-{index}')
                assert params['commandName'] in info['commands']
                frame = request('session.open', params, session.get('replies', []))
                check(frame, session['expect'])
                transcript.append({'open': params, 'frame': frame, 'reverse': list(reverse)})
                if 'error' not in session['expect']:
                    for call in session.get('calls', []):
                        check(request('session.call', {'sessionId': params['sessionId'],
                            'operation': call['operation'], 'payload': call.get('payload', {})},
                            call.get('replies', [])), call['expect'])
                    check(request('session.dispose', {'sessionId': params['sessionId']}), {'result': {'ok': True, 'mode': 'unchanged'}})
            assert 'result' in request('shutdown', {})
            assert process.wait(timeout=5) == 0, 'Process did not exit cleanly'
        finally:
            if process.poll() is None: process.kill(); process.wait(timeout=5)
            process.stdin.close(); process.stdout.close()
        if renderer_host:
            subprocess.run(['node', str(Path(__file__).with_name('renderer-consumer.mjs')), str(renderer_host)],
                           input=json.dumps(transcript), text=True, check=True)
        print(slug, f"ZIP inventory + protocol-info/hello/initialize/{len(scenario['sessions'])} scripted sessions/calls/dispose/shutdown passed (offline, no installation)")


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('archive', type=Path)
    p.add_argument('--scenario', type=Path, default=Path(__file__).resolve().parents[1] / 'tests' / 'smoke-scenario.json')
    p.add_argument('--renderer-host', type=Path, help='Read-only current host checkout; execute its actual renderer consumers on package results')
    a = p.parse_args(); smoke(a.archive, json.loads(a.scenario.read_text(encoding='utf-8')), a.renderer_host)
