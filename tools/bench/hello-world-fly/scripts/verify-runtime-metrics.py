#!/usr/bin/env python3
"""Read-only deployment, private exporter, and active-load verification."""
import concurrent.futures, datetime, json, pathlib, shlex, subprocess, urllib.request, urllib.error
root=pathlib.Path(__file__).resolve().parents[1]
manifest=json.loads((root/'manifest.json').read_text())
def check(v):
    app=v['app'];name=v['variant']
    machines=json.loads(subprocess.check_output(['fly','machine','list','-a',app,'--json'],text=True))
    assert len(machines)==1,(app,len(machines))
    m=machines[0]
    result={'variant':name,'machine':m,'checked_utc':datetime.datetime.now(datetime.timezone.utc).isoformat()}
    if name.startswith(('node-','python-')):
        with urllib.request.urlopen('https://'+app+'.fly.dev/',timeout=15) as response:
            body=response.read(); assert response.status==200 and body==b'hello world',(app,body)
            assert response.headers['content-type']=='text/plain; charset=utf-8'
            result['hello']={'status':response.status,'body':body.decode(),'content_type':response.headers['content-type']}
        if name.startswith('node-'):
            code="require('node:http').get('http://127.0.0.1:9091/metrics',r=>r.pipe(process.stdout))"
            command='node -e '+shlex.quote(code)
        else:
            code="import urllib.request; print(urllib.request.urlopen('http://127.0.0.1:9091/metrics').read().decode(),end='')"
            command='python -c '+shlex.quote(code)
        raw=subprocess.check_output(['fly','ssh','console','-a',app,'-C',command],text=True)
        raw=raw[raw.index('# HELP'):];values={}
        for line in raw.splitlines():
            if line and not line.startswith('#'):
                metric,value=line.split();values[metric]=float(value)
        assert values['process_resident_memory_bytes']>0,values
        if name.startswith('node-'):
            assert values['nodejs_heap_size_total_bytes']>=values['nodejs_heap_size_used_bytes']>0
        else:
            assert values['python_tracemalloc_enabled']==1 and values['python_traced_memory_peak_bytes']>=values['python_traced_memory_current_bytes']>0
        result['metrics']=values
        result['metrics_exposition']=raw
        assert m['config'].get('metrics'),m['config']
        # Only HTTP 80/443 are publicly exposed by services; 9091 is scrape-only.
        assert all(port['port']!=9091 for svc in m['config']['services'] for port in svc['ports'])
        try:
            urllib.request.urlopen('https://'+app+'.fly.dev/metrics',timeout=10)
            raise AssertionError('Private metrics accidentally exposed on the public service')
        except urllib.error.HTTPError as e: assert e.code==404,e
    return app,result
rows=dict(concurrent.futures.ThreadPoolExecutor(max_workers=4).map(check,manifest['variants']))
(root/'runtime-metrics-verification.json').write_text(json.dumps(rows,indent=2)+'\n')
for a,row in rows.items():print(a,row['machine']['id'],row.get('metrics',{}))
s=subprocess.check_output(['python3',str(root/'scripts/load-control.py'),'status'],text=True);s=s[s.index('{'):];(root/'runtime-metrics-load-status.json').write_text(s);load=json.loads(s)
assert load['status']=='running' and load['configured_duration_seconds']==0
before=json.loads((root/'pre-runtime-metrics-load.json').read_text());assert before['id']==load['id'];assert all(load['targets'][n]['requests']>t['requests'] for n,t in before['targets'].items())
print('Same continuous run:',load['id'],load['elapsed_seconds'])
for n,t in load['targets'].items():print(n,t['observed_completion_qps_last_interval'],t['status_codes'])
