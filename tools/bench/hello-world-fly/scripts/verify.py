#!/usr/bin/env python3
import datetime, json, pathlib, subprocess, sys, urllib.request
r = pathlib.Path(__file__).resolve().parents[1]
m = json.loads((r/'manifest.json').read_text())
phase = sys.argv[1] if len(sys.argv) > 1 else 'smoke'
evidence = {'timestamp_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'variants': []}
failures = []
for v in m['variants']:
    row = {'variant': v['variant']}
    try:
        machines = json.loads(subprocess.check_output(['fly', 'machine', 'list', '-a', v['app'], '--json']))
        if len(machines) != 1:
            raise RuntimeError(f"expected exactly one machine; found {len(machines)}")
        machine = machines[0]
        v.update(machine_id=machine['id'], image_ref=machine['image_ref'], instance_id=machine['instance_id'], restart_policy=machine['config']['restart'])
        row['machine'] = machine
        req = urllib.request.Request(v['url'], headers={'Cache-Control': 'no-cache'})
        with urllib.request.urlopen(req, timeout=5) as res:
            body = res.read(100)
            row.update(status=res.status, body_hex=body.hex(), headers=dict(res.headers))
            if res.status != 200 or body != b'hello world' or res.headers['content-type'] != 'text/plain; charset=utf-8':
                raise RuntimeError('response mismatch')
        print(v['variant'], machine['state'], res.status, repr(body), flush=True)
    except Exception as exc:
        row['verification_error'] = str(exc)
        failures.append(v['variant'])
        print(v['variant'], 'FAILED', str(exc), flush=True)
    evidence['variants'].append(row)
(r/'manifest.json').write_text(json.dumps(m, indent=2)+'\n')
(r/f'{phase}.json').write_text(json.dumps(evidence, indent=2)+'\n')
if failures:
    sys.exit('Verification failed: '+', '.join(failures))
