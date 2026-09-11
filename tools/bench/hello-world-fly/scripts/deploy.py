#!/usr/bin/env python3
import json,subprocess,sys,pathlib,datetime
root=pathlib.Path(__file__).resolve().parents[1]
m=json.loads((root/'manifest.json').read_text())
(root/'artifacts').mkdir(exist_ok=True)
selected=sys.argv[1:] or [v['variant'] for v in m['variants']]
for v in m['variants']:
    if v['variant'] not in selected: continue
    app=v['app']; name=v['variant']
    exists=subprocess.run(['fly','status','-a',app,'--json'],capture_output=True)
    if exists.returncode:
        subprocess.run(['fly','apps','create',app,'--org',m['org']],check=True)
    before=json.loads(subprocess.check_output(['fly','machine','list','-a',app,'--json'],text=True))
    if len(before)>1: raise RuntimeError(f'{app}: expected at most one machine; refuse deployment')
    with (root/'artifacts'/f'{name}-deploy.log').open('w') as log:
        rc=subprocess.run(['fly','deploy','--remote-only','--ha=false','--strategy','immediate','--yes','--wait-timeout','3m'],cwd=root/name,stdout=log,stderr=subprocess.STDOUT).returncode
    print(name,'deployment exit',rc,flush=True)
    status=subprocess.run(['fly','machine','list','-a',app,'--json'],capture_output=True,text=True)
    (root/'artifacts'/f'{name}-machines.json').write_text(status.stdout)
    if rc: raise SystemExit(rc)
    machines=json.loads(status.stdout)
    if len(machines)!=1: raise RuntimeError(f'{app}: expected exactly one machine, got {len(machines)}')
