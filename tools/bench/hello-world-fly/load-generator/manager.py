"""Keep one collector active until explicitly disabled; recover generator failures."""
import datetime, json, os, pathlib, signal, subprocess, time
root = pathlib.Path('/results')
root.mkdir(exist_ok=True)
child = None
stopping = False

def terminate(signum, frame):
    global stopping
    stopping = True
    if child is not None and child.poll() is None:
        child.send_signal(signal.SIGTERM)

signal.signal(signal.SIGTERM, terminate)
signal.signal(signal.SIGINT, terminate)
while not stopping:
    if (root/'disabled').exists():
        time.sleep(1)
        continue
    settings = json.loads((root/'settings.json').read_text()) if (root/'settings.json').exists() else {}
    duration = int(settings.get('duration_seconds', os.environ.get('DURATION_SECONDS', '0')))
    env = dict(os.environ, DURATION_SECONDS=str(duration))
    child = subprocess.Popen(['python', '/app/run.py'], env=env)
    rc = child.wait()
    print(json.dumps({'collector_exit': rc, 'at': datetime.datetime.now(datetime.timezone.utc).isoformat()}), flush=True)
    if duration and rc == 0:
        (root/'disabled').touch()
    time.sleep(2)
