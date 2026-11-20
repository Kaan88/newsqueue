"""Isolated Linux container checks. No Usenet servers or live downloads."""
import json
import subprocess
import time
import urllib.error
import urllib.request

NAME = 'newsqueue-release-smoke'
KEY = 'isolated-container-release-test'
IMAGE = 'newsqueue:1.0.8-web.1'

def docker(*args):
    return subprocess.check_output(['docker', *args], text=True).strip()

def request(path, key=KEY):
    req = urllib.request.Request('http://127.0.0.1:18480' + path,
                                 headers={'X-Api-Key': key} if key else {})
    return urllib.request.urlopen(req, timeout=5)

try:
    docker('run', '-d', '--name', NAME, '-p', '127.0.0.1:18480:8480',
           '-e', 'NQ_API_KEY=' + KEY, '-e', 'PUID=1234', '-e', 'PGID=1234', IMAGE)
    for attempt in range(60):
        state = docker('inspect', '-f', '{{.State.Health.Status}}', NAME)
        if state == 'healthy':
            break
        time.sleep(1)
    assert state == 'healthy', docker('logs', NAME)
    assert 'Uid:\t1234\t1234' in docker('exec', NAME, 'cat', '/proc/1/status')
    for command in ['unrar', 'par2', 'setpriv']:
        docker('exec', NAME, 'sh', '-c', 'command -v ' + command)
    docker('exec', NAME, 'sh', '-c', 'command -v 7z || command -v 7zz')
    with request('/api/state') as response:
        assert json.load(response)['queue'] == []
    try:
        request('/api/state', key=None)
        raise AssertionError('Unauthenticated API request was accepted')
    except urllib.error.HTTPError as error:
        assert error.code == 401
    with request('/', key=None) as response:
        assert b'newsqueue' in response.read().lower()
    with request('/pkg/newsqueue-web_bg.wasm', key=None) as response:
        wasm = response.read()
        assert wasm[:4] == b'\x00asm'
        assert b'__nqAudit' not in wasm and b'__nqState' not in wasm
    settings = json.loads(docker('exec', NAME, 'cat', '/config/newsqueue-settings.json'))
    assert settings['download_dir'] == '/downloads'
    assert settings['auto_import_folder'] == '/watch'
    docker('stop', '-t', '15', NAME)
    assert docker('inspect', '-f', '{{.State.ExitCode}}', NAME) == '0'
    docker('start', NAME)
    for attempt in range(30):
        try:
            with request('/api/state') as response:
                assert json.load(response)['queue'] == []
            break
        except (OSError, urllib.error.URLError):
            time.sleep(1)
    else:
        raise AssertionError('Restart did not recover')
    assert json.loads(docker('exec', NAME, 'cat', '/config/newsqueue-settings.json'))['download_dir'] == '/downloads'
    print('PASS: health, non-root user, tools, authentication, production WASM, folders, shutdown and restart')
finally:
    subprocess.run(['docker', 'logs', NAME], check=False)
    subprocess.run(['docker', 'rm', '-f', '-v', NAME], check=False)
