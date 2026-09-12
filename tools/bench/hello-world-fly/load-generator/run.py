#!/usr/bin/env python3
"""Independent Vegeta attacks with bounded evidence; discard raw results after validation/counting."""
import base64, datetime, fcntl, json, os, pathlib, re, logging, logging.handlers, signal, subprocess, threading, time, urllib.request
ROOT = pathlib.Path('/results')
ROOT.mkdir(exist_ok=True)
lock = (ROOT/'run.lock').open('w')
fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
duration = int(os.environ.get('DURATION_SECONDS', '0'))
if not 0 <= duration <= 86400:
    raise ValueError('DURATION_SECONDS must be 0 (continuous) or between 1 and 86400')
targets = json.loads(pathlib.Path('/app/targets.json').read_text())
# No readiness gate: failed endpoints must continue receiving their scheduled load.
run_id = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
history = ROOT/'continuous-runs'
history.mkdir(exist_ok=True)
out = history/run_id
out.mkdir()
# Keep ten bounded summaries for continuous collector generations; preserve legacy runs.
import shutil
runs = sorted(p for p in history.iterdir() if p.is_dir())
for old in runs[:-10]: shutil.rmtree(old)
start = time.time()
state = {'id': run_id, 'start_utc': datetime.datetime.fromtimestamp(start, datetime.timezone.utc).isoformat(), 'configured_duration_seconds': duration, 'scheduled_end_utc': datetime.datetime.fromtimestamp(start+duration, datetime.timezone.utc).isoformat() if duration else None, 'supervisor_pid': os.getpid(), 'vegeta_version': subprocess.check_output(['vegeta','-version'],text=True).strip(), 'status': 'running', 'targets': {}}
processes = []
stop = threading.Event()
mutex = threading.Lock()
barrier = threading.Barrier(len(targets))

def save():
    with mutex:
        temp = out/'status.tmp'
        temp.write_text(json.dumps(state, indent=2)+'\n')
        temp.replace(out/'status.json')

def interrupted(signum, frame):
    state['status'] = 'stopping'
    stop.set()
    for p in processes:
        if p.poll() is None:
            p.send_signal(signal.SIGINT)
signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGINT, interrupted)

def attack(name, url):
    target = out/(name+'.targets')
    target.write_text('GET '+url+'\n')
    args = ['vegeta', 'attack', '-name='+name, '-targets='+str(target), '-rate=10/1s', '-duration='+str(duration)+'s', '-timeout=10s', '-workers=10', '-max-workers=2000', '-connections=100', '-max-connections=2000', '-keepalive=true', '-http2=false', '-redirects=0', '-max-body=32']
    row = {'url':url, 'command':args, 'requests':0, 'status_codes':{}, 'errors':{}, 'body_mismatches':0, 'first_request_unix':None, 'last_request_unix':None}
    state['targets'][name]=row
    barrier.wait()
    logger = logging.getLogger(name)
    handler = logging.handlers.RotatingFileHandler(out/(name+'.stderr'), maxBytes=1048576, backupCount=1)
    logger.addHandler(handler)
    def diagnostics(stream):
        for chunk in iter(lambda: stream.read(4096), b''):
            logger.warning(chunk.decode(errors='replace'))
    try:
        while not stop.is_set():
            p = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=dict(os.environ, GOMAXPROCS='1'))
            threading.Thread(target=diagnostics, args=(p.stderr,), daemon=True).start()
            processes.append(p); row['attack_pid']=p.pid
            encoder = subprocess.Popen(['vegeta','encode','-to=json'], stdin=p.stdout, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=dict(os.environ, GOMAXPROCS='1'))
            threading.Thread(target=diagnostics, args=(encoder.stderr,), daemon=True).start()
            p.stdout.close()
            for line in encoder.stdout:
                result=json.loads(line)
                timestamp=datetime.datetime.fromisoformat(re.sub(r'\.(\d+)', lambda match: '.' + (match[1] + '000000')[:6], result['timestamp']).replace('Z','+00:00')).timestamp()
                row['requests']+=1
                if row['first_request_unix'] is None: row['first_request_unix']=timestamp
                row['last_request_unix']=max(timestamp, row['last_request_unix'] or timestamp)
                code=str(result['code']);row['status_codes'][code]=row['status_codes'].get(code,0)+1
                error=result.get('error','')
                if error:
                    key=error[:300]
                    if key not in row['errors'] and len(row['errors'])>=20: key='other errors'
                    row['errors'][key]=row['errors'].get(key,0)+1
                if base64.b64decode(result.get('body') or '') != b'hello world': row['body_mismatches']+=1
            row['attack_exit_code']=p.wait()
            row['encoder_exit_code']=encoder.wait()
            if duration or stop.is_set(): break
            row['attack_restarts'] = row.get('attack_restarts', 0) + 1
            row['last_attack_restart_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
            processes.remove(p)
            stop.wait(2)
    except Exception as exc:
        row['supervisor_error']=str(exc)
        interrupted(signal.SIGTERM, None)
        state['collector_failed']=True
        for child in (locals().get('p'),locals().get('encoder')):
            if child and child.poll() is None:
                child.terminate()
                child.wait(timeout=15)
    finally:
        row['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()

# Publish a complete status before exposing this run to stop/status controls.
save()
(ROOT/'latest').write_text(str(out))
threads=[threading.Thread(target=attack,args=item) for item in targets.items()]
for t in threads:t.start()
previous={name:0 for name in targets};previous_time=start
while any(t.is_alive() for t in threads):
    time.sleep(5)
    now=time.time();state['elapsed_seconds']=round(now-start,3)
    for name,row in list(state['targets'].items()):
        row['observed_completion_qps_last_interval']=round((row['requests']-previous[name])/(now-previous_time),3)
        previous[name]=row['requests']
        first,last=row['first_request_unix'],row['last_request_unix']
        row['observed_request_qps']=round((row['requests']-1)/(last-first),3) if first and last>first else 0
    previous_time=now
    save()
    if round(now-start)%30<5:
        print(json.dumps({'run':run_id,'elapsed':round(now-start),'targets':{n:{k:r.get(k) for k in ['requests','observed_request_qps','status_codes','errors','body_mismatches']} for n,r in state['targets'].items()}}),flush=True)
for t in threads:t.join()
state['end_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
state['status']='stopped' if stop.is_set() else ('completed' if all(r.get('attack_exit_code')==0 and r.get('encoder_exit_code')==0 for r in state['targets'].values()) else 'failed')
save()
print(json.dumps({'run':run_id, 'status':state['status']}),flush=True)
if state['status'] == 'failed' or state.get('collector_failed'): raise SystemExit(1)
